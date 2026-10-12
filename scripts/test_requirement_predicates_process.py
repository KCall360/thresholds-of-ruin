"""Permanent requirement predicates through the authoritative compiler and server."""
import subprocess
import tomllib

from arena_matrix import toml_source
from process_harness import ProcessTestCase, ROOT, WIZARD_TOKEN, content_ids, creature_package


class RequirementPredicateProcesses(ProcessTestCase):
    def test_owned_parent_and_permanent_predicates_survive_dormancy_and_restart(self):
        package = creature_package(self, "permanent-predicates", abilities=())
        rules_path = package / "rules.toml"
        rules = tomllib.loads(rules_path.read_text(encoding="utf-8"))
        rules["talents"].extend([
            {"id": "fixture_parent", "name": "Fixture parent", "prerequisites": [],
             "requirement": {"kind": "all", "requirements": [
                 {"kind": "subtype_in", "subtypes": ["aquatic"]},
                 {"kind": "species_flag", "flag": "magical", "present": True}]},
             "modifiers": [{"scope": "always", "op": {
                 "kind": "add", "stat": {"kind": "health"}, "value": 2}}]},
            {"id": "fixture_child", "name": "Fixture child", "prerequisites": [],
             "requirement": {"kind": "owned_talent", "talent": "fixture_parent"},
             "modifiers": [{"scope": "always", "op": {
                 "kind": "add", "stat": {"kind": "health"}, "value": 3}}]},
        ])
        rules_path.write_text(toml_source(rules), encoding="utf-8", newline="\n")
        manifest_path = package / "scenario.toml"
        manifest = tomllib.loads(manifest_path.read_text(encoding="utf-8"))
        manifest["creatures"]["species"]["human"]["subtypes"] = ["aquatic"]
        manifest["creatures"]["templates"] = {
            "land_bound": {"priority": 0, "remove_subtypes": ["aquatic"]}}
        manifest_path.write_text(toml_source(manifest), encoding="utf-8", newline="\n")
        compiled = subprocess.run(
            [self.bin / ("tor-scenario" + self.suffix), "validate", package],
            capture_output=True, text=True, encoding="utf-8", timeout=20)
        self.assertEqual(compiled.returncode, 0, compiled.stderr)
        server = self.server(scenario=package, wizard=True)
        observer, _ = self.client(WIZARD_TOKEN, observe=True)
        wizard, _ = self.text_client(WIZARD_TOKEN, observe=True)

        def snapshot():
            return self.request(observer, {"type": "snapshot"})["state"]

        # Authoring/loading alone is not enough: live ownership must gate the
        # child, and removing a permanent prerequisite must disable both nodes.
        initial = snapshot()
        self.assertIn("Server error", wizard.command("wizard creature talent 1 2 fixture_child"))
        self.assertEqual(snapshot(), initial)
        for command in ("wizard creature talent 1 1 fixture_parent",
                        "wizard creature talent 1 2 fixture_child"):
            self.assertNotIn("Server error", wizard.command(command), command)
        active = snapshot()
        active_stats = active["observation"]["combat"]["own_stats"]
        self.assertEqual(set(content_ids(active_stats["active_talents"])),
                         {"fixture_parent", "fixture_child"})
        self.assertNotIn("Server error", wizard.command("wizard creature template 1 land_bound on"))
        dormant = snapshot()
        dormant_stats = dormant["observation"]["combat"]["own_stats"]
        self.assertEqual(set(content_ids(dormant_stats["dormant_talents"])),
                         {"fixture_parent", "fixture_child"})
        self.assertEqual(dormant_stats["active_talents"], [])
        self.assertEqual(dormant["observation"]["combat"]["max_hp"],
                         active["observation"]["combat"]["max_hp"] - 5)
        self.assertIsNone(self.request(observer, {"type": "save"})["error"])
        observer.stop()
        wizard.stop()
        server.stop()
        rules_path.unlink()
        self.server(scenario=ROOT / "scenarios/two-room", seed=None, wizard=True)
        observer, restored = self.client(WIZARD_TOKEN, observe=True)
        self.assertEqual(restored["state"], dormant)
        wizard, _ = self.text_client(WIZARD_TOKEN, observe=True)
        self.assertNotIn("Server error", wizard.command("wizard creature template 1 land_bound off"))
        self.assertEqual(snapshot()["observation"]["combat"]["own_stats"], active_stats)
