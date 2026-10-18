"""Bounded production A/D mechanism cases, not balance or long-survival evidence.

The creature recipes, owned purchases, rules and selected Warrior kit are copied
unchanged. Only the lit encounter geometry and external foe control differ.
Each case uses one fixed seed and ordinary client commands. Public item/creature
state is checked again after deleting the copied source and reopening its save.
Text/ASCII controls and stream repair remain covered by their existing owners.
"""
import copy
import json
import shutil
import subprocess
import unittest

from arena_matrix import toml_source
from process_harness import ProcessTestCase
from process_work_budget import ProcessWorkBudget
from rogue_roster_package import PRODUCTION, encounter_sources, unrelated_fallback
from scalar_wire import u64
from status_condition_package import combat, observation
from rogue_test_support import disclosed_frames, resolved_events


def mechanism_package(test, species):
    source = encounter_sources()
    identity = "rogue." + species
    if identity not in source["actors"]:
        raise ValueError("The mechanism case requires an actual production recipe")
    manifest = {key: copy.deepcopy(source["manifest"][key]) for key in
                ("format", "version", "ruleset", "modules", "factions", "ai_profiles", "themes", "assets")}
    character = copy.deepcopy(source["manifest"]["characters"][0])
    character["anchor"] = "1/start"
    manifest.update(id="rogue-mechanism-" + species, default_character=1, characters=[character])
    region = {"id": 1, "name": "Authored Rogue mechanism encounter", "size": [9, 5, 2],
              "chamber": True, "gravity": [0, 0, -1], "anchors": {"start": [2, 2, 0]},
              "actors": [{"id": 3, "at": [3, 2, 0], "archetype": identity,
                          "controller": "external"}]}
    package = test.directory / ("mechanism-" + species)
    (package / "regions").mkdir(parents=True)
    (package / "catalog").mkdir()
    for module in manifest["modules"]:
        shutil.copy2(PRODUCTION / module, package / module)
    shutil.copy2(PRODUCTION / "rules.toml", package / "rules.toml")
    (package / "scenario.toml").write_text(toml_source(manifest), encoding="utf-8", newline="\n")
    (package / "regions/1.toml").write_text(toml_source(region), encoding="utf-8", newline="\n")
    result = subprocess.run([test.bin / ("tor-scenario" + test.suffix), "validate", package],
                            capture_output=True, text=True, encoding="utf-8", timeout=30)
    (test.directory / (species + "-validation.log")).write_text(
        result.stdout + result.stderr, encoding="utf-8")
    test.assertEqual(result.returncode, 0, result.stderr)
    return package


def armor_item(frame):
    return next(item for item in observation(frame)["inventory"] if item["class"] == "armor")


def armor_reductions(frame):
    values = []
    item = armor_item(frame)["id"]
    interaction = next(row for row in observation(frame)["interactions"]["inventory"]
                       if row["item"] == item)
    for modifier in interaction["known_equipment"]["modifiers"]:
        if modifier["id"] == "reduction":
            values.extend(int(field["value"]["value"]) for field in modifier["fields"]
                          if field["id"] == "amount")
    return sorted(values)


class RogueCorrosionRayProcesses(ProcessTestCase):
    def setUp(self):
        super().setUp()
        self.work_budget = ProcessWorkBudget(requests=600, actor_ticks=20000,
                                             transcript_bytes=128 * 1024 * 1024)

    def tearDown(self):
        report = self.work_budget.report()
        (self.directory / "work-budget.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
        print("Corrosion/ray process budget: " + json.dumps(report, sort_keys=True), flush=True)
        if not self._failed():
            self.work_budget.check()
        super().tearDown()

    def launch(self, *args, **kwargs):
        self.work_budget.check()
        process = super().launch(*args, **kwargs)
        self.work_budget.output.paths += (process.transcript.path,)
        self.work_budget.check()
        return process

    def client(self, *args, **kwargs):
        kwargs.update(omit_memory=True, compact_background=True)
        client, frame = super().client(*args, **kwargs)
        self.work_budget.track(client, frame)
        return client, frame

    def command(self, client, value):
        self.work_budget.before()
        frame = super().command(client, value)
        self.work_budget.after(client, frame)
        return frame

    def frame(self, client, predicate, seconds=15):
        return self.work_budget.frame_wait(super().frame, client, predicate, seconds=seconds)

    def snapshot(self, client):
        return self.request(client, {"type": "snapshot"})

    def advance_until(self, clients, predicate):
        for _ in range(24):
            frames = tuple(self.snapshot(client) for client in clients)
            self.assertEqual(observation(frames[0])["tick"], observation(frames[1])["tick"])
            self.assertFalse(any(combat(frame)["dead"] for frame in frames))
            if predicate(frames):
                return frames
            ready = [index for index, frame in enumerate(frames) if observation(frame)["ready"]]
            self.assertEqual(len(ready), 1)
            self.assertIsNone(self.act(clients[ready[0]], {"type": "wait"})["error"])
        self.fail("Mechanism did not settle within 24 ordinary decisions")

    def reopen_without_source(self, package, server, clients):
        before = tuple(self.snapshot(client) for client in clients)
        self.flush_save()
        for client in clients:
            client.stop()
        server.stop()
        shutil.rmtree(package)
        server = self.server(scenario=unrelated_fallback(self), seed=None)
        reopened = [self.client(actor=actor) for actor in (1, 3)]
        for (_, after), prior in zip(reopened, before):
            self.assertEqual(after["state"], prior["state"])
            self.assertEqual(after["history"], prior["history"])
        for client, _ in reopened:
            client.stop()
        server.stop()

    def test_production_aquator_rusts_owned_armor_to_floor_without_changing_defense(self):
        package = mechanism_package(self, "aquator")
        server = self.server("--checkpoint-interval", 4096, scenario=package, seed=42)
        player, initial = self.client(actor=1)
        foe, initial_foe = self.client(actor=3)
        clients = (player, foe)
        self.assertFalse(initial["state"]["wizard_game"])
        self.assertEqual(u64(combat(initial_foe)["own_stats"]["hit_dice"]["total"]), 5)
        self.assertEqual(observation(initial_foe)["inventory"], [])
        self.assertEqual(armor_reductions(initial), [2, 2])
        item = armor_item(initial)["id"]
        hp = combat(initial)["hp"]
        physical = combat(initial)["own_stats"]["defenses"]["physical"]
        target = next(actor["id"] for actor in observation(initial_foe)["visible_actors"]
                      if actor["name"] == "Warrior")
        seen = {2}
        floor_contacts = 0
        for _ in range(32):
            before = self.snapshot(foe)
            start = len(foe.transcript)
            accepted = self.command(foe, {"type": "act", "action": {
                "type": "use_ability", "id": "rogue.corrode",
                "invocation": {"kind": "actor_target", "target": target},
                "supplies": {"kind": "carried"}}})
            self.assertIsNone(accepted["error"])
            current, _ = self.advance_until(clients, lambda frames: not frames[1]["intentions"])
            self.assertEqual(armor_item(current)["id"], item)
            self.assertEqual(combat(current)["hp"], hp)
            self.assertEqual(combat(current)["own_stats"]["defenses"]["physical"], physical)
            reductions = armor_reductions(current)
            self.assertIn(reductions, ([2, 2], [1, 1], []))
            remaining = reductions[0] if reductions else 0
            seen.add(remaining)
            events = list(resolved_events(disclosed_frames(foe, start), before,
                                          "rogue.corrode", target))
            self.assertTrue(events, "The actual completed contact must be disclosed")
            if remaining == 0 and any(event["outcome"] != "miss" for event in events):
                # Include the success which reaches zero, then require another
                # successful zero-damage contact at the exhausted armor floor.
                floor_contacts += 1
                if floor_contacts == 2:
                    break
        else:
            self.fail("Fixed-seed production contacts did not reach and retain the armor floor")
        self.assertEqual(seen, {0, 1, 2})
        self.reopen_without_source(package, server, clients)


if __name__ == "__main__":
    unittest.main()
