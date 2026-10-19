"""Actual starter care and a legal thaw escape against unchanged CH1 content.

Both actors use ordinary client actions. Controlled ice attacks isolate the
shared denial/escape contract; they are not evidence for autonomous pursuit or
mixed-floor survival. Bounded seed selection is a behavior witness, not a win rate.
"""
from scalar_wire import u64
import shutil
import subprocess
import unittest

from arena_matrix import toml_source
from process_harness import ProcessTestCase, ROOT, underfoot
from rogue_roster_study import encounter_sources, source_cases, study_encounter


class RogueStarterChillProcesses(ProcessTestCase):
    def package(self):
        source = encounter_sources()
        case = next(row for row in source_cases(source, "playable-unspent")
                    if row["species"] == "rogue.ice_monster")
        manifest, region = study_encounter(case, "candidate", "forward", source,
                                          reference="playable-unspent")
        del manifest["arena"]
        manifest["id"] = "rogue-starter-chill"
        region["actors"][0].update(at=[2, 2, 0], controller="external")
        del region["actors"][0]["ai"]
        package = self.directory / "starter-chill"
        (package / "regions").mkdir(parents=True)
        (package / "catalog").mkdir()
        production = ROOT / "scenarios/rogue-exploration"
        for name in ("creatures.toml", "actors.toml", "items.toml"):
            shutil.copy2(production / "catalog" / name, package / "catalog" / name)
        shutil.copy2(production / "rules.toml", package / "rules.toml")
        (package / "scenario.toml").write_text(toml_source(manifest), encoding="utf-8", newline="\n")
        (package / "regions/1.toml").write_text(toml_source(region), encoding="utf-8", newline="\n")
        validated = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                   capture_output=True, text=True, encoding="utf-8", timeout=30)
        self.assertEqual(validated.returncode, 0, validated.stderr)
        return package

    def test_actual_mage_kit_can_cancel_frozen_care_then_escape_during_targetwide_protection(self):
        package = self.package()
        outcomes = []
        witness = None
        # Shared actual-kit witness: seed 0 misses; seed 749 passes both checks.
        # These declared fixtures are not a seed-rate or survival sample.
        for seed in (0, 749):
            self.save = self.directory / f"starter-chill-{seed}.db"
            server = self.server(scenario=package, character=2, seed=seed)
            player, initial = self.client(actor=2)
            ice, ice_initial = self.client(actor=3)
            view = initial["state"]["observation"]
            self.assertFalse(initial["state"]["wizard_game"])
            # Ring mail reduces physical damage; it does not raise this defense.
            self.assertEqual(view["combat"]["own_stats"]["defenses"]["physical"], 14)
            self.assertEqual({row["name"]: int(row["quantity"]) for row in view["inventory"]},
                             {"mace": 1, "ring mail": 1, "clean bandage": 3, "food ration": 1})
            growth = self.command(player, {"type": "advancement", "command": "overview"})
            self.assertEqual(growth["advancement_page"]["entries"][0]["value"]["points"],
                             {"earned": "1", "spent": "0", "unspent": "1"})
            started = self.act(player, {"type": "use_ability", "id": "bind_wounds",
                "invocation": {"aim": {"kind": "actor_target", "target": view["self_target"]}, "weapon": {"kind": "default"}},
                "supplies": {"kind": "carried"}})
            self.assertIsNone(started["error"])
            target = next(row["id"] for row in ice_initial["state"]["observation"]["visible_actors"]
                          if row["name"] == "Mage")
            cast = self.act(ice, {"type": "use_ability", "id": "rogue.freezing_touch",
                "invocation": {"aim": {"kind": "actor_target", "target": target}, "weapon": {"kind": "default"}}})
            self.assertIsNone(cast["error"])
            def ready(frame):
                if frame.get("state") is None:
                    return False
                observed = frame["state"]["observation"]
                return (observed["ready"] and observed["combat"]["preparation_remaining"] is None
                        and int(observed["combat"]["recovery_remaining"]) == 0)
            if not ready(cast):
                self.frame(ice, ready)
            current = self.request(player, {"type": "snapshot"})
            # Advance only the controlled opponent while the player's retained
            # care runs. Never replace the real defense or force a successful hit.
            for _ in range(12):
                phases = current["state"]["observation"]["combat"]["own_stats"]["conditions"]
                frozen = any(row["condition"]["id"] == "rogue.frozen" and row["phase"] == "active"
                             for row in phases)
                if frozen or not current["intentions"]:
                    break
                self.assertIsNone(self.act(ice, {"type": "wait"})["error"])
                current = self.request(player, {"type": "snapshot"})
            outcomes.append({"seed": seed, "frozen_care": frozen})
            if frozen:
                witness = (server, player, ice, initial, current)
                break
            player.stop()
            ice.stop()
            server.stop()
        self.assertIsNotNone(witness, f"No actual-kit freeze witness in bounded seed set: {outcomes}")
        self.assertEqual(outcomes, [{"seed": 0, "frozen_care": False},
                                    {"seed": 749, "frozen_care": True}])
        server, player, ice, initial, paused = witness
        self.assertTrue(any(row["phase"] == "paused" for row in paused["intentions"]))
        self.assertEqual(u64(paused["state"]["observation"]["combat"]["hp"]),
                         u64(initial["state"]["observation"]["combat"]["hp"]))
        here = underfoot(paused["state"])["key"]
        denied = self.command(player, {"type": "cancel_intention", "step": "west"})
        self.assertIsNotNone(denied["error"], "Frozen must deny an escape step")
        self.assertEqual(underfoot(denied["state"])["key"], here)
        self.assertEqual(denied["intentions"], paused["intentions"])
        cancelled = self.command(player, {"type": "cancel_intention"})
        self.assertIsNone(cancelled["error"])
        self.assertEqual(underfoot(cancelled["state"])["key"], here)
        self.assertEqual(cancelled["intentions"], [])
        current = cancelled
        for _ in range(12):
            view = current["state"]["observation"]
            protected = any(row["condition"]["id"] == "rogue.frozen" and row["phase"] == "protection"
                            for row in view["combat"]["own_stats"]["conditions"])
            if protected and view["ready"]:
                break
            self.assertIsNone(self.act(ice, {"type": "wait"})["error"])
            current = self.request(player, {"type": "snapshot"})
        else:
            self.fail("The real kit did not gain a ready escape opportunity during thaw protection")
        escaped = self.act(player, {"type": "move", "direction": "west"})
        self.assertIsNone(escaped["error"])
        self.assertNotEqual(underfoot(escaped["state"])["key"], here)
        stock = {row["name"]: int(row["quantity"]) for row in escaped["state"]["observation"]["inventory"]}
        self.assertEqual(stock["clean bandage"], 3, "Unresolved cancelled care must not spend the held bandage")
        player.stop()
        ice.stop()
        server.stop()


if __name__ == "__main__":
    unittest.main()
