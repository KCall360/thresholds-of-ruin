"""One bounded M/P/X mechanism contract through ordinary server/client actions.

Production Medusa, Phantom and Xeroc species, recipes, purchases and rules are
copied unchanged. The lit room and externally controlled foe are authored test
geometry/control. The explicitly synthetic living witness has durable Health,
low defenses and an explicit basic-melee check bonus with harmless-sized damage. These are mechanism
witnesses, not actual starter-kit survival, autonomous AI or balance evidence.

All five fresh encounters use seed 42, without retries or seed search. Assertions
use client-visible state and ordinary commands, never wizard edits or checkpoint
target lookup. The three X encounters separately exercise Take, Search and the
first outgoing attack; a revealed incarnation cannot legitimately rearm itself.

Existing Text/ASCII, native-input and stream-repair contract owners remain
test_items_process, test_ascii_interactions_process, test_ascii_process and
test_stream_recovery_process. This test adds the production content mechanism
sequence rather than multiplying those application-boundary witnesses.

After the compiler owner releases qualified binaries:
TOR_TEST_BINARIES=/absolute/path/to/debug python scripts/test_runner.py \
    test_rogue_perception_process -v
"""
import copy
import json
import shutil
import subprocess
import unittest

from arena_matrix import toml_source
from process_harness import ProcessTestCase
from process_work_budget import ProcessWorkBudget
from rogue_roster_package import PRODUCTION, encounter_sources
from scalar_wire import u64
from status_condition_package import combat, observation


GAZE = "rogue.confusing_gaze"
CONFUSED = "rogue.confused"
SEARCH = "rogue.search"
WITNESS = "Authored perception witness"


def perception_package(test, species, arm, *, turn_ticks=25):
    """Change only fixture geometry/control and the explicitly synthetic victim."""
    source = encounter_sources()
    manifest = {key: copy.deepcopy(source["manifest"][key]) for key in
                ("format", "version", "ruleset", "modules", "factions", "ai_profiles", "themes", "assets")}
    victim = copy.deepcopy(source["species"]["rogue.human"])
    victim["melee"]["strikes"][0]["on_success"] = [{"kind": "damage", "damage": {
        "kind": "components", "components": [{"category": "impact",
            "amount": {"kind": "fixed", "amount": 1}}]}}]
    victim["modifiers"] = [
        # NaturalAttack uses its skill check, not the legacy melee metadata bonus.
        # This explicit synthetic accuracy modifier keeps the real 3d6 check.
        {"scope": "always", "op": {"kind": "add", "stat": {
            "kind": "ability", "ability": "basic_melee", "stat": "check_bonus"}, "value": 100}},
        {"scope": "always", "op": {"kind": "add", "stat": {"kind": "health"}, "value": 500}},
        *[{"scope": "always", "op": {"kind": "add", "stat": {
            "kind": "defense", "defense": defense}, "value": -100}}
          for defense in ("physical", "spiritual")]]
    manifest["creatures"] = {"species": {"fixture.perception_witness": victim}}
    character = copy.deepcopy(source["manifest"]["characters"][0])
    character.update(anchor="1/start", turn_ticks=turn_ticks, loadout=[], known_identities=[])
    character["creature"].update(species="fixture.perception_witness", name=WITNESS,
                                 nutrition={"kind": "exempt"}, future_growth={"kind": "disabled"})
    manifest.update(id="rogue-perception-" + arm, default_character=1, characters=[character])
    region = {"id": 1, "name": "Authored lit perception encounter", "size": [7, 5, 2],
              "chamber": True, "gravity": [0, 0, -1], "anchors": {"start": [1, 2, 0]},
              "actors": [{"id": 3, "at": [2, 2, 0], "archetype": "rogue." + species,
                          "controller": "external"}]}
    package = test.directory / ("perception-" + arm)
    (package / "regions").mkdir(parents=True)
    (package / "catalog").mkdir()
    for module in manifest["modules"]:
        shutil.copy2(PRODUCTION / module, package / module)
    shutil.copy2(PRODUCTION / "rules.toml", package / "rules.toml")
    (package / "scenario.toml").write_text(toml_source(manifest), encoding="utf-8", newline="\n")
    (package / "regions/1.toml").write_text(toml_source(region), encoding="utf-8", newline="\n")
    result = subprocess.run([test.bin / ("tor-scenario" + test.suffix), "validate", package],
                            capture_output=True, text=True, encoding="utf-8", timeout=30)
    (test.directory / (arm + "-validation.log")).write_text(
        result.stdout + result.stderr, encoding="utf-8")
    test.assertEqual(result.returncode, 0, result.stderr)
    return package


def tick(frame):
    return u64(observation(frame)["tick"])


def ability(identity, target):
    # Contact targets intentionally use the ordinary opaque actor_target wire
    # variant; only the server may resolve its private target domain.
    return {"type": "use_ability", "id": identity,
            "invocation": {"kind": "actor_target", "target": target},
            "supplies": {"kind": "carried"}}


def cooldown(frame, identity=GAZE):
    return u64(next(row["cooldown_remaining"] for row in combat(frame)["own_stats"]["abilities"]
                    if row["id"] == identity))


def confused(frame):
    return next((row for row in combat(frame)["own_stats"]["conditions"]
                 if row["condition"]["id"] == CONFUSED), None)


def other_actors(frame):
    view = observation(frame)
    return [row for row in view["visible_actors"] if row["id"] != view["self_target"]]


class RoguePerceptionProcesses(ProcessTestCase):
    def setUp(self):
        super().setUp()
        # One non-resetting diagnostic budget across M, P and all three X arms.
        # These are work/output guards, not production performance thresholds.
        self.work_budget = ProcessWorkBudget(requests=500, actor_ticks=12000,
                                             transcript_bytes=32 * 1024 * 1024)

    def tearDown(self):
        report = self.work_budget.report()
        (self.directory / "work-budget.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
        print("Perception process budget: " + json.dumps(report, sort_keys=True), flush=True)
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

    def start_encounter(self, species, arm, *, turn_ticks=25):
        package = perception_package(self, species, arm, turn_ticks=turn_ticks)
        self.save = self.directory / (arm + ".db")
        server = self.server(scenario=package, character=1, seed=42)
        player, initial = self.client(actor=1)
        foe, foe_initial = self.client(actor=3)
        self.assertFalse(initial["state"]["wizard_game"])
        self.assertEqual(observation(initial)["inventory"], [])
        self.assertEqual((tick(initial), tick(foe_initial)), (0, 0))
        return server, (player, foe), (initial, foe_initial)

    def advance_until(self, clients, predicate):
        """Advance only the publicly ready actor, using bounded ordinary Waits."""
        for _ in range(64):
            frames = tuple(self.snapshot(client) for client in clients)
            self.assertEqual(tick(frames[0]), tick(frames[1]))
            self.assertFalse(any(combat(frame)["dead"] for frame in frames))
            if predicate(frames):
                return frames
            ready = [index for index, frame in enumerate(frames) if observation(frame)["ready"]]
            self.assertEqual(len(ready), 1, "The fixture must expose exactly one ordinary decision owner")
            self.assertIsNone(self.act(clients[ready[0]], {"type": "wait"})["error"])
        self.fail("Perception mechanism did not reach its boundary in 64 ordinary decisions")

    def advance_to(self, clients, at):
        frames = self.advance_until(clients, lambda frames: tick(frames[0]) >= at)
        self.assertEqual(tick(frames[0]), at, "The fixture's 25-tick witness must expose the exact boundary")
        return frames

    def queue_foe(self, clients, action):
        # act() waits for execution of queued intentions. Use command() for the
        # non-ready foe, then let the actual ready player's Wait admit execution.
        accepted = self.command(clients[1], {"type": "act", "action": action})
        self.assertIsNone(accepted["error"])
        self.assertIsNone(self.act(clients[0], {"type": "wait"})["error"])
        return tuple(self.snapshot(client) for client in clients)

    def victim_target(self, frame):
        return next(row["id"] for row in other_actors(frame) if row["name"] == WITNESS)

    def assert_anonymous_contact(self, frame):
        rows = other_actors(frame)
        self.assertTrue(rows, "An adjacent invisible body must retain anonymous physical contact")
        targets = {row["id"] for row in rows}
        self.assertEqual(len(targets), 1)
        for row in rows:
            self.assertEqual(row["name"], "")
            self.assertEqual(row["description"], "")
            self.assertIsNone(row.get("asset"))
            self.assertIsNone(row.get("appearance_seed"))
        self.assertFalse(any(row["actor"] in targets for row in combat(frame)["actors"]),
                         "Anonymous contact must not disclose faction, alliance or injury")
        return next(iter(targets))

    def assert_mace_guise(self, frame):
        self.assertEqual(other_actors(frame), [])
        ground = observation(frame)["ground_items"]
        self.assertEqual(len(ground), 1)
        row, = ground
        item = row["item"]
        self.assertEqual((item["name"], item["appearance"], item["description"]), ("mace", "mace", ""))
        self.assertEqual((item["class"], item["asset"], item["identified"], u64(item["quantity"])),
                         ("weapon", "item.weapon.mace", True, 1))
        self.assertIsNone(item["known_supply"])
        self.assertFalse(row["reachable"], "Adjacent guise keeps the ordinary same-cell pickup affordance")
        self.assertFalse(row["supply_reachable"])
        return item["id"]

    def assert_xeroc_revealed(self, frame):
        self.assertEqual(observation(frame)["ground_items"], [])
        self.assertEqual(observation(frame)["inventory"], [], "A guise is never a carried item")
        rows = other_actors(frame)
        self.assertTrue(rows)
        self.assertTrue(all(row["name"] == "xeroc" and row.get("asset") == "creature.xeroc" for row in rows))

    def medusa_contract(self):
        server, clients, initial = self.start_encounter("medusa", "m")
        action = ability(GAZE, self.victim_target(initial[1]))
        preparing = self.queue_foe(clients, action)
        self.assertEqual(tick(preparing[0]), 25)
        self.assertEqual(u64(combat(preparing[1])["preparation_remaining"]), 75)
        self.assertTrue(combat(preparing[1])["preparation_active"])
        self.assertEqual(cooldown(preparing[1]), 675)
        self.assertIsNone(confused(preparing[0]))

        affected = self.advance_to(clients, 100)
        self.assertEqual((confused(affected[0])["phase"], u64(confused(affected[0])["remaining_ticks"])),
                         ("active", 300))
        self.assertEqual(combat(affected[0])["hp"], combat(initial[0])["hp"])
        self.assertIsNone(combat(affected[1])["preparation_remaining"])
        self.assertEqual(u64(combat(affected[1])["recovery_remaining"]), 100)
        self.assertEqual(cooldown(affected[1]), 600)

        recovered = self.advance_to(clients, 200)
        self.assertEqual(u64(combat(recovered[1])["recovery_remaining"]), 0)
        self.assertEqual(cooldown(recovered[1]), 500)
        foe_ready = self.advance_until(clients, lambda frames: observation(frames[1])["ready"])
        self.assertEqual(tick(foe_ready[1]), 200)
        denied = self.command(clients[1], {"type": "act", "action": action})
        self.assertIsNotNone(denied["error"], "Recovery alone must not bypass the actual gaze cooldown")
        self.assertEqual((tick(denied), cooldown(denied)), (200, 500))
        self.assertIsNone(combat(denied)["preparation_remaining"])

        protected = self.advance_to(clients, 400)
        self.assertEqual((confused(protected[0])["phase"], u64(confused(protected[0])["remaining_ticks"])),
                         ("protection", 400))
        cooled = self.advance_to(clients, 700)
        self.assertEqual(cooldown(cooled[1]), 0)
        self.assertEqual((confused(cooled[0])["phase"], u64(confused(cooled[0])["remaining_ticks"])),
                         ("protection", 100))
        expired = self.advance_to(clients, 800)
        self.assertIsNone(confused(expired[0]))
        for process in (*clients, server):
            process.stop()

    def phantom_contract(self):
        server, clients, initial = self.start_encounter("phantom", "p")
        contact = self.assert_anonymous_contact(initial[0])
        started = self.act(clients[0], ability("basic_melee", contact))
        self.assertIsNone(started["error"], "Ordinary basic melee must accept the disclosed contact target")
        attacked = self.advance_until(clients, lambda frames:
            u64(combat(frames[1])["hp"]) < u64(combat(initial[1])["hp"])
            and observation(frames[0])["ready"]
            and combat(frames[0])["preparation_remaining"] is None)
        self.assertEqual(self.assert_anonymous_contact(attacked[0]), contact)
        self.assertFalse(combat(attacked[1])["dead"])
        before_hp = u64(combat(attacked[0])["hp"])
        self.queue_foe(clients, ability("basic_melee", self.victim_target(attacked[1])))
        retaliated = self.advance_until(clients, lambda frames: u64(combat(frames[0])["hp"]) < before_hp)
        self.assertEqual(self.assert_anonymous_contact(retaliated[0]), contact,
                         "Neither receiving nor making an attack removes living Phantom invisibility")
        for process in (*clients, server):
            process.stop()

    def xeroc_contract(self, probe):
        server, clients, initial = self.start_encounter("xeroc", "x-" + probe,
                                                       turn_ticks=25 if probe == "attack" else 100)
        item = self.assert_mace_guise(initial[0])
        if probe == "take":
            # Quantity denials must not spend the otherwise valid physical probe.
            for quantity in ("0", "2"):
                denied = self.act(clients[0], {"type": "take", "item": item, "quantity": quantity})
                self.assertIsNotNone(denied["error"])
                self.assertEqual(tick(denied), 0)
                self.assertEqual(self.assert_mace_guise(denied), item)
            touched = self.act(clients[0], {"type": "take", "item": item, "quantity": None})
            self.assertIsNone(touched["error"])
            self.assert_xeroc_revealed(touched)
            self.assertEqual((tick(touched), u64(combat(touched)["recovery_remaining"])), (0, 50))
            ready = self.advance_to(clients, 50)
            self.assertTrue(observation(ready[0])["ready"])
        elif probe == "search":
            started = self.act(clients[0], ability(SEARCH, observation(initial[0])["self_target"]))
            self.assertIsNone(started["error"])
            self.assertEqual((tick(started), u64(combat(started)["preparation_remaining"])), (0, 50))
            self.assertEqual(self.assert_mace_guise(started), item)
            # The other actor's 100-tick Wait spans effect completion at 50 and
            # the separate 50 recovery. Its next published decision is at 100.
            ready = self.advance_to(clients, 100)
            self.assertTrue(observation(ready[0])["ready"])
            self.assertIsNone(combat(ready[0])["preparation_remaining"])
            self.assertEqual(u64(combat(ready[0])["recovery_remaining"]), 0)
        else:
            self.assertEqual(probe, "attack")
            started = self.queue_foe(clients, ability("basic_melee", self.victim_target(initial[1])))
            self.assertEqual(tick(started[0]), 25)
            self.assertIsNotNone(combat(started[1])["preparation_remaining"])
            self.assertTrue(combat(started[1])["preparation_active"])
            self.assert_xeroc_revealed(started[0])
            self.assertEqual(combat(started[0])["hp"], combat(initial[0])["hp"],
                             "The first outgoing preparation reveals before its checked damage")
            ready = self.advance_until(clients, lambda frames:
                u64(combat(frames[0])["hp"]) < u64(combat(initial[0])["hp"])
                and combat(frames[1])["preparation_remaining"] is None)
        self.assert_xeroc_revealed(ready[0])
        # Free repeated reads must not restore the globally consumed disguise.
        self.assert_xeroc_revealed(self.snapshot(clients[0]))
        for process in (*clients, server):
            process.stop()

    def test_production_m_p_x_preserve_perception_and_paid_lifecycles(self):
        self.medusa_contract()
        self.phantom_contract()
        for probe in ("take", "search", "attack"):
            self.xeroc_contract(probe)


if __name__ == "__main__":
    unittest.main()
