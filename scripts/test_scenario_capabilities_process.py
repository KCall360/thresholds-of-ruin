"""Scenario capability switches and authored growth schedules in real processes.

A locked owner build grows only along its authored schedule; owner choices are
refused by the server, and disabled corpses leave no body item on the ground.
"""
import shutil
import subprocess
import tomllib
import unittest

from arena_matrix import toml_source
from process_harness import ProcessTestCase, ROOT
from scalar_wire import u64
from test_player_advancement_process import earned_growth_package, observation


def locked_growth_package(test):
    """The earned-growth encounter with owner choices locked to one scheduled row."""
    source = earned_growth_package(test)
    package = test.directory / "locked-growth"
    shutil.copytree(source, package)
    manifest = tomllib.loads((package / "scenario.toml").read_text(encoding="utf-8"))
    manifest["id"] = "locked-growth"
    manifest["capabilities"] = {"npc_stair_traversal": True, "owner_advancement": False,
                                "corpses": False}
    build = manifest["characters"][0]["creature"]
    build["restoration"] = "authored"
    build["future_growth"] = {"kind": "authored"}
    build["growth_schedule"] = [{"source": "racial", "purchases": [
        {"pool": "practice", "selection": {"kind": "skill_rank", "skill": "persuasion"}}]}]
    (package / "scenario.toml").write_text(toml_source(manifest), encoding="utf-8", newline="\n")
    compiler = test.bin / ("tor-scenario" + test.suffix)
    validated = subprocess.run([str(compiler), "validate", str(package)], capture_output=True,
                               text=True, encoding="utf-8", timeout=60)
    test.assertEqual(validated.returncode, 0, validated.stderr)
    return package


def overview(test, player):
    frame = test.command(player, {"type": "advancement", "command": "overview"})
    test.assertIsNone(frame["error"])
    return frame["advancement_page"]["entries"][0]["value"]


class ScenarioCapabilityProcesses(ProcessTestCase):
    def test_locked_owner_grows_its_scheduled_row_and_the_kill_leaves_no_corpse(self):
        self.server(scenario=locked_growth_package(self))
        player, initial = self.client()
        partner, _ = self.client(actor=2)
        before = overview(self, player)
        self.assertTrue(before["locked"])
        self.assertFalse(before["ready"])
        self.assertEqual(before["growth"]["future"], {"kind": "authored"})
        refused = self.command(player, {"type": "advancement", "command": "source racial"})
        self.assertIsNotNone(refused["error"])
        self.assertEqual(overview(self, player)["growth"], before["growth"])

        target = next(row["id"] for row in observation(initial)["visible_actors"]
                      if row["name"] == "partner")
        self.assertIsNone(self.act(player, {"type": "use_ability", "id": "basic_melee", "invocation": {
            "aim": {"kind": "actor_target", "target": target}, "weapon": {"kind": "default"}}})["error"])
        # The normal opponent turn advances the authored 100-tick wind-up.
        self.assertIsNone(self.act(partner, {"type": "wait"})["error"])
        after = overview(self, player)
        self.assertEqual(after["growth"]["earned_xp"], "100")
        self.assertEqual(u64(after["growth"]["highest_earned_tier"]), 2)
        # The scheduled row's practice rank is applied, not left for a choice.
        self.assertEqual(u64(after["points"]["spent"]), u64(before["points"]["spent"]) + 1)
        self.assertTrue(after["locked"])
        ground = observation(self.request(player, {"type": "snapshot"}))["ground_items"]
        self.assertFalse([row for row in ground if row["item"]["name"].endswith("corpse")])
        partner.stop()

    def test_rogue_package_starts_locked_with_rogue_kit_and_no_care(self):
        self.server(scenario=ROOT / "scenarios/rogue")
        player, initial = self.client()
        value = overview(self, player)
        self.assertTrue(value["locked"])
        self.assertEqual(value["growth"]["future"], {"kind": "authored"})
        names = sorted(row["name"] for row in observation(initial)["inventory"])
        self.assertIn("shortbow", names)
        self.assertIn("arrow", names)
        self.assertFalse([name for name in names if "bandage" in name])
        abilities = {row["id"] for row in observation(initial)["combat"]["own_stats"]["abilities"]}
        self.assertNotIn("bind_wounds", abilities)
        self.assertIn("basic_melee", abilities)

    def check_mutated_rogue(self, edit):
        package = self.directory / "rogue-mutated"
        shutil.copytree(ROOT / "scenarios/rogue", package, dirs_exist_ok=True)
        manifest = tomllib.loads((package / "scenario.toml").read_text(encoding="utf-8"))
        edit(manifest)
        (package / "scenario.toml").write_text(toml_source(manifest), encoding="utf-8", newline="\n")
        compiler = self.bin / ("tor-scenario" + self.suffix)
        return subprocess.run([str(compiler), "check", str(package)], capture_output=True,
                              text=True, encoding="utf-8", timeout=120)

    def test_compiler_rejects_a_schedule_without_authored_growth_and_unknown_switches(self):
        self.assertEqual(self.check_mutated_rogue(lambda manifest: None).returncode, 0)

        def choose_again(manifest):
            manifest["characters"][0]["creature"]["restoration"] = "choose_again"

        def selected(manifest):
            manifest["characters"][0]["creature"]["future_growth"] = {
                "kind": "selected", "source": {"kind": "class", "class": "warrior"}}

        def over_cap(manifest):
            manifest["characters"][0]["creature"]["growth_schedule"][0]["purchases"] *= 2

        def unknown(manifest):
            manifest["capabilities"]["bandages"] = False

        for edit in (choose_again, selected, over_cap, unknown):
            with self.subTest(edit=edit.__name__):
                self.assertNotEqual(self.check_mutated_rogue(edit).returncode, 0)


if __name__ == "__main__":
    unittest.main()
