"""Real selected-character entry for explicit diagnostics and promoted production.

Later-floor starts are bounded receipt diagnostics with the actual HD-one kit.
They do not stand in for earned descent, dangerous encounters or escape evidence.
"""
import copy
import json
import shutil
import sqlite3
import subprocess
import tomllib
import unittest

from checkpoint_payload import crc32c, decode_checkpoint_payload
from process_harness import ProcessTestCase, ROOT, hit_die_counts, inspect_save
from process_work_budget import ProcessWorkBudget
from rogue_activation_candidate import conditional_bands, create_candidate
from status_condition_package import actor_creature, checkpoint


class RogueActivationCandidateProcesses(ProcessTestCase):
    def setUp(self):
        super().setUp()
        self.work_budget = ProcessWorkBudget(requests=512, actor_ticks=3000,
                                             transcript_bytes=32 * 1024 * 1024)

    def tearDown(self):
        report = self.work_budget.report()
        (self.directory / "work-budget.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
        print("Process budget " + self.id() + ": " + json.dumps(report, sort_keys=True), flush=True)
        if not self._failed():
            self.work_budget.check()
        super().tearDown()

    def client(self, *args, **kwargs):
        kwargs["omit_memory"] = True  # Assertions use current disclosure and saved authority.
        client, frame = super().client(*args, **kwargs)
        self.work_budget.track(client, frame)
        return client, frame

    def command(self, client, value):
        self.work_budget.before()
        result = super().command(client, value)
        self.work_budget.after(client, result)
        return result

    def frame(self, client, predicate, seconds=15):
        return self.work_budget.frame_wait(super().frame, client, predicate, seconds=seconds)

    def flush_save(self, *, actor=None):
        # private_receipts uses this separate synchronous save connection.
        self.work_budget.before()
        super().flush_save(actor=actor)
        self.work_budget.check()

    def complete_checkpoint(self, player):
        """Include referenced dormant records, never stale region history."""
        saved = checkpoint(self, player)
        with sqlite3.connect(self.save) as database:
            payload = database.execute("SELECT payload FROM checkpoint").fetchone()[0]
            document = json.loads(decode_checkpoint_payload(payload))
            game, shared = document["game"], document["shared"]
            lifecycle = shared["lifecycles"][game["lifecycle"]]
            for record in lifecycle["detached"].values():
                frame = database.execute("SELECT frame FROM regions WHERE record=?", (record,)).fetchone()[0]
                self.assertEqual(frame[:4], b"TORR")
                self.assertEqual(len(frame) - 24, int.from_bytes(frame[16:20], "little"))
                self.assertEqual(crc32c(frame[4:20] + frame[24:]), int.from_bytes(frame[20:24], "little"))
                detached = json.loads(frame[24:])
                for key in ("actors", "items"):
                    self.assertFalse(set(saved[key]) & set(detached[key]), "Owned identity cannot be duplicated")
                    saved[key].update(detached[key])
                saved["frozen_at"].update(detached["stamps"])
        return saved

    def candidate_restart(self, selected, depth, checkpoint_interval, *, seed=42):
        output = self.directory / f"candidate-{selected}-{depth}-{checkpoint_interval}-{seed}"
        provenance = {"kind": "repository_package", "package": "scenarios/rogue-exploration",
                      "reference": "repository:scenarios/rogue-exploration"}
        inventory = create_candidate(output, self.bin / ("tor-scenario" + self.suffix),
            source_package=ROOT / "scenarios/rogue-exploration", source_provenance=provenance,
            start_depth=depth)
        self.assertEqual(inventory["format"], "tor-rogue-activation-inputs-v2")
        self.assertEqual(inventory["source_provenance"], provenance)
        self.assertEqual(inventory["source_population_shape"], "canonical_thirteen_species")
        self.assertEqual(inventory["artifact_purpose"], "diagnostic_population_package")
        self.assertIs(inventory["production_mutated"], False)
        eligible = {choice["archetype"] for band in conditional_bands()
                    if band["depth"]["minimum"] <= depth
                    and (band["depth"]["kind"] == "open" or depth <= band["depth"]["maximum"])
                    for choice in band["choices"]}
        return self.package_restart(output, selected, depth, checkpoint_interval, eligible, seed=seed)

    def package_restart(self, output, selected, depth, checkpoint_interval, eligible, *, seed=42):
        """Inspect already prepared content without generating or replacing its policy."""
        package = output / "package"
        source = tomllib.loads((package / "catalog/actors.toml").read_text(encoding="utf-8"))["actors"]
        self.save = self.directory / f"{output.name}.db"
        server = self.server("--checkpoint-interval", checkpoint_interval, scenario=package,
                             character=selected, seed=seed)
        player, initial = self.client(actor=selected)
        view = initial["state"]["observation"]
        self.assertFalse(initial["state"]["wizard_game"])
        self.assertEqual({row["name"]: int(row["quantity"]) for row in view["inventory"]},
                         {"mace": 1, "ring mail": 1, "clean bandage": 3, "food ration": 1})
        self.assertEqual(hit_die_counts(view["combat"]["own_stats"]["hit_dice"]),
                         {("class", "warrior" if selected == 1 else "mage"): 1})
        names = {row["id"]: row["name"] for row in view["inventory"]}
        self.assertEqual({names[row["item"]]: row["slot"]["id"]
                          for row in view["interactions"]["inventory"] if row["equipped_slot"] is not None},
                         {"mace": "weapon", "ring mail": "body_armor"})
        receipts = self.private_receipts(actor=selected)
        initial_save = inspect_save(self.save)
        self.assertEqual(initial_save["scenario"]["package"]["selected"], selected)
        self.assertEqual(len(receipts), 1)
        receipt = receipts[0]
        self.assertEqual(receipt["scope"], f"dungeon/{depth}")
        self.assertEqual(receipt["entrant"], selected)
        self.assertEqual((receipt["profile"], receipt["profile_version"], receipt["cause"]),
                         ("rogue.low_depth", 1, "initial"))
        self.assertEqual(len(receipt["decisions"]), 4)
        self.assertEqual([stage["id"] for stage in receipt["decisions"]],
                         ["rogue.eight_species_foes", "rogue.bandages", "rogue.rations", "rogue.restore_strength"])
        foe_result = receipt["decisions"][0]["result"]
        self.assertEqual(foe_result["kind"], "evaluated")
        self.assertEqual(foe_result["requested"], 4)
        self.assertEqual(len(foe_result["attempts"]), 4)
        for row in foe_result["attempts"]:
            self.assertIn(row["content"], eligible)
            self.assertIn(row["kind"], ("placed", "skipped"))
            if row["kind"] == "skipped":
                self.assertEqual(row["reason"], "no_eligible_cell")
        placed = [row for row in foe_result["attempts"] if row["kind"] == "placed"]
        self.assertTrue(placed, "The diagnostic must actually activate ordinary creature content")
        self.assertEqual(len({row["entity"]["id"] for row in placed}), len(placed))
        for row in placed:
            self.assertIn(row["content"], eligible)
            self.assertEqual(row["entity"]["kind"], "actor")
            stock = row["entity"]["carried_loot"]
            if row["content"] in ("rogue.centaur", "rogue.yeti", "rogue.troll", "rogue.vampire"):
                self.assertIn(stock["kind"], ("carried", "not_carried"))
                if stock["kind"] == "carried":
                    self.assertIn(stock["content"], ("rogue.ration", "rogue.clean_bandage"))
                    self.assertEqual(stock["quantity"], 1)
            else:
                self.assertIsNone(stock)
        # Repeated observations and save barriers are not fresh entry or wake.
        for _ in range(3):
            observed = self.request(player, {"type": "snapshot"})
            self.assertEqual(observed["state"], initial["state"])
        self.assertEqual(self.private_receipts(actor=selected), receipts)
        # One ordinary action creates a real journal/checkpoint tail. Births are
        # outside initial sight, so this is a receipt test, not an all-hit duel.
        waited = self.act(player, {"type": "wait"})
        self.assertIsNone(waited["error"])
        self.assertEqual(self.private_receipts(actor=selected), receipts)
        saved = self.complete_checkpoint(player) if checkpoint_interval == 1 else None
        if saved is not None:
            for row in placed:
                recipe = source[row["content"]]["creature"]
                born = actor_creature(saved, row["entity"]["id"])
                expected = len(recipe["hit_dice"]) + max(0, depth - 26)
                self.assertEqual(len(born["build"]["hd"]), expected)
                self.assertEqual(born["build"]["species"], row["content"])
                self.assertEqual(len(born["build"]["restoration"]["entries"]), expected)
                expected_choices = [{"purchases": die["purchases"], "attribute": die.get("attribute")}
                                    for die in recipe["hit_dice"]]
                expected_sources = [{"kind": "racial"} if die["source"] == "racial" else
                                    {"kind": "class", "id": die["source"]} for die in recipe["hit_dice"]]
                expected_sources += [{"kind": "racial"}] * (expected - len(expected_sources))
                expected_choices += [{"purchases": [], "attribute": None}] * (expected - len(expected_choices))
                self.assertEqual([die["source"] for die in born["build"]["hd"]], expected_sources)
                self.assertEqual(born["build"]["choices"], expected_choices)
                self.assertEqual(born["build"]["restoration"], {"kind": "authored", "entries": [
                    {"source": hd_source, "choices": choice}
                    for hd_source, choice in zip(expected_sources, expected_choices, strict=True)]})
                for health_seed in [born["build"]["health_root_seed"],
                                    *(die["health_seed"] for die in born["build"]["hd"])]:
                    self.assertIs(type(health_seed), int)
                    self.assertGreaterEqual(health_seed, 0)
                    self.assertLess(health_seed, 1 << 64)
                stock = row["entity"]["carried_loot"]
                owned_items = {identity: item for identity, item in saved["items"].items()
                               if item["location"] == {"Carried": row["entity"]["id"]}}
                if stock is not None and stock["kind"] == "carried":
                    item = saved["items"][str(stock["item"])]
                    self.assertEqual(item["location"], {"Carried": row["entity"]["id"]})
                    self.assertEqual(item["quantity"], stock["quantity"])
                    self.assertEqual(set(owned_items), {str(stock["item"])})
                else:
                    self.assertFalse(owned_items, "A missed/no-stock birth cannot own an undeclared item")
            self.assertEqual(len(actor_creature(saved, selected)["build"]["hd"]), 1,
                             "Later-depth starts never grant the player progression")
            mage_or_warrior = actor_creature(saved, selected)
            self.assertEqual(mage_or_warrior["build"]["hd"][0]["source"],
                             {"kind": "class", "id": "warrior" if selected == 1 else "mage"})
            self.assertEqual(mage_or_warrior["build"]["choices"], [{"purchases": [], "attribute": None}])
            self.assertEqual(mage_or_warrior["earned_progress"]["earned_xp"], 0)
        before = self.request(player, {"type": "snapshot"})
        self.assertEqual(before["state"]["observation"]["inventory"], view["inventory"],
                         "A diagnostic wait must not change the actual starter kit or finite stock")
        player.stop()
        server.stop()
        with sqlite3.connect(self.save) as database:
            checkpoint_row = database.execute("SELECT sequence FROM checkpoint").fetchone()
            self.assertEqual(checkpoint_row is not None, checkpoint_interval == 1)
        shutil.rmtree(output)
        self.assertFalse(output.exists(), "Restore must use saved source without the diagnostic package")
        server = self.server("--checkpoint-interval", checkpoint_interval, "--regions", 2,
                             seed=None, save_actor=selected)
        player, restored = self.client(actor=selected)
        self.assertEqual(restored["state"], before["state"])
        self.assertEqual(restored["history"], before["history"])
        self.assertEqual(self.private_receipts(actor=selected), receipts)
        self.assertEqual(inspect_save(self.save)["scenario"]["package"]["selected"], selected)
        if saved is not None:
            self.assertEqual(self.complete_checkpoint(player), saved,
                             "Source-free replay retains complete owned rows, stock and Health state")
        player.stop()
        server.stop()
        return receipts

    def test_warrior_candidate_entry_keeps_actual_kit_and_journal_receipt_after_source_deletion(self):
        self.candidate_restart(1, 1, 0)

    def test_mage_candidate_entry_keeps_actual_kit_and_checkpoint_receipt_after_source_deletion(self):
        self.candidate_restart(2, 1, 1)

    def test_ice_quagga_candidate_band_survives_source_free_checkpoint_restart(self):
        self.candidate_restart(1, 11, 1)

    def test_depth_fourteen_wraith_birth_has_owned_rows_zero_stock_and_source_free_restart(self):
        # Exercise production activation directly. Only the Warrior's authored
        # entry anchor changes; no rebuilt bands, traversal or private guidance.
        production = ROOT / "scenarios/rogue-exploration"
        output = self.directory / "production-warrior-depth-14"
        package = output / "package"
        shutil.copytree(production, package, ignore=shutil.ignore_patterns(".compiled"))
        source_text = (production / "scenario.toml").read_text(encoding="utf-8")
        source_manifest = tomllib.loads(source_text)
        manifest = copy.deepcopy(source_manifest)
        warrior = next(row for row in manifest["characters"] if row["id"] == 1)
        old_anchor = f'id = 1\nanchor = "{warrior["anchor"]}"'
        self.assertEqual(source_text.count(old_anchor), 1)
        warrior["anchor"] = "dungeon/14/up"
        diagnostic_text = source_text.replace(old_anchor, 'id = 1\nanchor = "dungeon/14/up"', 1)
        self.assertEqual(tomllib.loads(diagnostic_text), manifest,
                         "The diagnostic may change only the selected Warrior's anchor")
        (package / "scenario.toml").write_text(diagnostic_text, encoding="utf-8", newline="\n")
        for relative in [source_manifest["ruleset"], *source_manifest["modules"]]:
            self.assertEqual((package / relative).read_bytes(), (production / relative).read_bytes())
        validated = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                   capture_output=True, text=True, encoding="utf-8", timeout=30)
        self.assertEqual(validated.returncode, 0, validated.stderr)
        self.assertEqual((production / "scenario.toml").read_text(encoding="utf-8"), source_text)
        receipts = self.package_restart(output, 1, 14, 1, {
            "rogue.zombie", "rogue.centaur", "rogue.quagga", "rogue.yeti", "rogue.troll", "rogue.wraith"}, seed=42)
        placed = [row for row in receipts[0]["decisions"][0]["result"]["attempts"]
                  if row["kind"] == "placed" and row["content"] == "rogue.wraith"]
        self.assertTrue(placed, "Fixed depth-14 entry must actually construct W")
        self.assertTrue(all(row["entity"]["carried_loot"] is None for row in placed))

    def test_quagga_unicorn_candidate_boundary_survives_source_free_journal_restart(self):
        self.candidate_restart(2, 17, 0)

    def test_depth_twenty_six_candidate_retains_unicorn_vampire_receipts_without_granting_earned_hd(self):
        self.candidate_restart(1, 26, 1)

    def test_depth_twenty_seven_open_tail_retains_nine_vampire_rows_after_source_deletion(self):
        self.candidate_restart(2, 27, 1)

    def test_production_mage_depth_twenty_seven_keeps_vampire_growth_and_stock_after_source_deletion(self):
        # This source is actual production. Never call the candidate generator
        # or rebuild its floor policy here: only the Mage's diagnostic anchor
        # changes. It remains a birth/restore test, not deep combat survival.
        production = ROOT / "scenarios/rogue-exploration"
        output = self.directory / "production-mage-depth-27"
        package = output / "package"
        shutil.copytree(production, package, ignore=shutil.ignore_patterns(".compiled"))
        source_text = (production / "scenario.toml").read_text(encoding="utf-8")
        source_manifest = tomllib.loads(source_text)
        manifest = copy.deepcopy(source_manifest)
        mage = next(row for row in manifest["characters"] if row["id"] == 2)
        old_anchor = f'id = 2\nanchor = "{mage["anchor"]}"'
        self.assertEqual(source_text.count(old_anchor), 1)
        mage["anchor"] = "dungeon/27/up"
        diagnostic_text = source_text.replace(old_anchor, 'id = 2\nanchor = "dungeon/27/up"', 1)
        self.assertEqual(tomllib.loads(diagnostic_text), manifest,
                         "The diagnostic may change only the selected Mage's anchor")
        (package / "scenario.toml").write_text(diagnostic_text, encoding="utf-8", newline="\n")
        for relative in [source_manifest["ruleset"], *source_manifest["modules"]]:
            self.assertEqual((package / relative).read_bytes(), (production / relative).read_bytes())

        activation = manifest["floor_activation"]
        self.assertEqual(activation["protagonist"], "selected_character")
        self.assertEqual(activation["families"], {"dungeon": "rogue.low_depth"})
        stages = activation["profiles"]["rogue.low_depth"]["stages"]
        self.assertEqual([row["id"] for row in stages],
                         ["rogue.eight_species_foes", "rogue.bandages", "rogue.rations", "rogue.restore_strength"])
        self.assertEqual(stages[0]["count"], [4, 4])
        self.assertEqual(stages[0]["depth_growth"], {"kind": "racial_after", "depth": 26})
        self.assertEqual(stages[0]["ai"], "rogue_hostile")
        self.assertIs(stages[0]["avoid_protagonist_sight"], True)
        self.assertEqual(stages[0]["bands"][-1], {
            "depth": {"kind": "open", "minimum": 27}, "choices": [{
                "archetype": "rogue.vampire", "weight": 1, "carried_loot": {
                    "version": 1, "pool": "rogue.vampire_supplies",
                    "chance": {"numerator": 1, "denominator": 5}}}]})
        self.assertEqual(activation["carried_item_pools"]["rogue.vampire_supplies"], {
            "version": 1, "choices": [
                {"archetype": "rogue.ration", "weight": 1, "quantity": 1},
                {"archetype": "rogue.clean_bandage", "weight": 1, "quantity": 1}]})
        actors = tomllib.loads((package / "catalog/actors.toml").read_text(encoding="utf-8"))["actors"]
        expected_base = [{"source": "racial", "purchases": [{
            "pool": "advancement", "selection": {"kind": "talent", "talent": talent}}]}
            for talent in ("rogue.drain_vitality", "rogue.regeneration")]
        expected_base += [{"source": "racial", "purchases": []}] * 6
        self.assertEqual(actors["rogue.vampire"]["creature"]["hit_dice"], expected_base,
                         "DR1/RG1 must remain owned by the first/second Vampire racial rows")
        self.assertEqual(mage["creature"]["hit_dice"], [{"source": "mage", "purchases": []}])
        self.assertEqual(mage["creature"]["starting_skills"], ["spellcasting", "heavy_weaponry"])
        validated = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                   capture_output=True, text=True, encoding="utf-8", timeout=30)
        self.assertEqual(validated.returncode, 0, validated.stderr)
        self.assertEqual((production / "scenario.toml").read_text(encoding="utf-8"), source_text)
        self.package_restart(output, 2, 27, 1, {"rogue.vampire"})

    def test_depth_274_open_tail_retains_256_owned_vampire_rows_after_source_deletion(self):
        self.candidate_restart(1, 274, 1)

    def test_open_tail_birth_stock_success_and_miss_remain_pinned_after_source_deletion(self):
        seen = set()
        samples = []
        # A bounded deterministic mechanism witness, not a frequency estimate.
        for seed in range(8):
            receipts = self.candidate_restart(2, 27, 1, seed=seed)
            decisions = [row["entity"]["carried_loot"]["kind"]
                         for row in receipts[0]["decisions"][0]["result"]["attempts"]
                         if row["kind"] == "placed"]
            seen.update(decisions)
            samples.append((seed, decisions))
            if seen == {"carried", "not_carried"}:
                break
        print("Vampire generated birth-stock samples: " + json.dumps(samples), flush=True)
        self.assertEqual(seen, {"carried", "not_carried"}, samples)


class RogueActivationSourceTests(unittest.TestCase):
    def test_wraith_depth14_changes_only_production_warrior_anchor(self):
        from pathlib import Path
        import tempfile
        from unittest.mock import patch
        driver = RogueActivationCandidateProcesses()
        receipt = [{"decisions": [{"result": {"attempts": [{"kind": "placed",
            "content": "rogue.wraith", "entity": {"carried_loot": None}}]}}]}]
        production = ROOT / "scenarios/rogue-exploration"
        original = tomllib.loads((production / "scenario.toml").read_text(encoding="utf-8"))
        expected = copy.deepcopy(original)
        next(row for row in expected["characters"] if row["id"] == 1)["anchor"] = "dungeon/14/up"
        def restart(output, selected, depth, interval, eligible, *, seed):
            self.assertEqual((selected, depth, interval, seed), (1, 14, 1, 42))
            self.assertEqual(eligible, {"rogue.zombie", "rogue.centaur", "rogue.quagga",
                                        "rogue.yeti", "rogue.troll", "rogue.wraith"})
            package = output / "package"
            self.assertEqual(tomllib.loads((package / "scenario.toml").read_text()), expected)
            for relative in [original["ruleset"], *original["modules"]]:
                self.assertEqual((package / relative).read_bytes(), (production / relative).read_bytes())
            return receipt
        with tempfile.TemporaryDirectory() as directory:
            driver.directory, driver.bin, driver.suffix = Path(directory), Path("compiler-bin"), ""
            with patch.object(driver, "candidate_restart", side_effect=AssertionError(
                    "Production witness must not rebuild a candidate policy")), patch.object(
                    driver, "package_restart", side_effect=restart) as reopened, patch(
                    "subprocess.run", return_value=subprocess.CompletedProcess([], 0, "", "")) as compiled:
                driver.test_depth_fourteen_wraith_birth_has_owned_rows_zero_stock_and_source_free_restart()
            self.assertEqual(reopened.call_count, 1)
            self.assertEqual(compiled.call_count, 1)
            self.assertEqual(compiled.call_args.args[0], [driver.bin / "tor-scenario", "validate",
                driver.directory / "production-warrior-depth-14/package"])
            self.assertEqual(compiled.call_args.kwargs["timeout"], 30)
        self.assertEqual(tomllib.loads((production / "scenario.toml").read_text()), original)


if __name__ == "__main__":
    unittest.main()
