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
from rogue_activation_candidate import conditional_bands, create_candidate, population_shape
from rogue_magic_receipts import (MAGIC_ITEM_STAGE_IDS, assert_magic_item_receipts,
    EQUIPMENT_ITEM_STAGE_IDS, assert_equipment_item_receipts, assert_finite_item_identity)
from rogue_test_support import assert_room_gold_receipt
from status_condition_package import actor_creature, checkpoint


def eligible_actors(manifest, depth):
    stages = manifest["floor_activation"]["profiles"]["rogue.low_depth"]["stages"]
    return {choice["archetype"] for stage in stages if stage["kind"] == "actors"
            for band in stage["bands"] if band["depth"]["minimum"] <= depth
            and (band["depth"]["kind"] == "open" or depth <= band["depth"]["maximum"])
            for choice in band["choices"]}


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
        source_manifest = tomllib.loads((ROOT / "scenarios/rogue-exploration/scenario.toml").read_text(encoding="utf-8"))
        self.assertEqual(inventory["source_population_shape"], population_shape(source_manifest))
        self.assertEqual(inventory["artifact_purpose"], "diagnostic_population_package")
        self.assertIs(inventory["production_mutated"], False)
        eligible = {choice["archetype"] for band in conditional_bands()
                    if band["depth"]["minimum"] <= depth
                    and (band["depth"]["kind"] == "open" or depth <= band["depth"]["maximum"])
                    for choice in band["choices"]}
        return self.package_restart(output, selected, depth, checkpoint_interval, eligible, seed=seed)

    def package_restart(self, output, selected, depth, checkpoint_interval, eligible, *,
                        seed=42, production_policy=False):
        """Inspect already prepared content without generating or replacing its policy."""
        package = output / "package"
        source = tomllib.loads((package / "catalog/actors.toml").read_text(encoding="utf-8"))["actors"]
        manifest = tomllib.loads((package / "scenario.toml").read_text(encoding="utf-8"))
        stage = manifest["floor_activation"]["profiles"]["rogue.low_depth"]["stages"][0]
        carriers = {choice["archetype"] for band in stage["bands"] for choice in band["choices"]
                    if choice.get("carried_loot") is not None}
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
        expected_stages = ["rogue.eight_species_foes", "rogue.bandages", "rogue.rations",
                           "rogue.restore_strength"]
        if production_policy:
            expected_stages.extend(["rogue.room_gold", *MAGIC_ITEM_STAGE_IDS, *EQUIPMENT_ITEM_STAGE_IDS])
        self.assertEqual([stage["id"] for stage in receipt["decisions"]], expected_stages)
        identities = []
        if production_policy:
            identities.extend(assert_room_gold_receipt(self, receipt["decisions"][4], depth))
            identities.extend(assert_magic_item_receipts(self, receipt["decisions"][5:8]))
            identities.extend(assert_equipment_item_receipts(self, receipt["decisions"][8:]))
        for supply, (content, quantity) in zip(receipt["decisions"][1:4], (
                ("rogue.clean_bandage", 2), ("rogue.ration", 1), ("rogue.restore_strength", 1)),
                strict=True):
            self.assertEqual(supply["version"], 1)
            result = supply["result"]
            self.assertEqual((result["kind"], result["requested"]), ("evaluated", 1))
            self.assertEqual(len(result["attempts"]), 1)
            for attempt in result["attempts"]:
                self.assertEqual(attempt["content"], content)
                self.assertIn(attempt["kind"], ("placed", "skipped"))
                if attempt["kind"] == "skipped":
                    self.assertEqual(attempt["reason"], "no_eligible_cell")
                    continue
                entity = attempt["entity"]
                self.assertEqual((entity["kind"], entity["quantity"]), ("item", quantity))
                identities.append(("item", entity["id"]))
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
            identities.append(("actor", row["entity"]["id"]))
            stock = row["entity"]["carried_loot"]
            if row["content"] == "rogue.leprechaun":
                self.assertTrue(production_policy, "The retained diagnostic has no Leprechaun births")
                self.assertEqual((stock["kind"], stock["content"]), ("carried", "rogue.gold"))
                self.assertGreaterEqual(stock["quantity"], 2)
                self.assertLessEqual(stock["quantity"], 51 + 10 * depth)
            elif row["content"] in carriers:
                self.assertIn(stock["kind"], ("carried", "not_carried"))
                if row["content"] == "rogue.dragon":
                    self.assertEqual(stock["kind"], "carried", "Dragon's birth chance is exactly 100%")
                if stock["kind"] == "carried":
                    self.assertIn(stock["content"], ("rogue.ration", "rogue.clean_bandage"))
                    self.assertEqual(stock["quantity"], 1)
            else:
                self.assertIsNone(stock)
            if stock is not None and stock["kind"] == "carried":
                identities.append(("item", stock["item"]))
        self.assertEqual(len(identities), len(set(identities)),
                         "Every placed or carried item must retain a distinct owner identity")
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
            if production_policy:
                for stock_stage in receipt["decisions"][5:]:
                    for attempt in stock_stage["result"]["attempts"]:
                        if attempt["kind"] == "placed":
                            item = saved["items"][str(attempt["entity"]["id"])]
                            self.assertEqual(item["location"], {"Ground": attempt["location"]})
                            self.assertEqual(item["quantity"], 1)
                            assert_finite_item_identity(self, item, attempt["content"])
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
                authored = next(choice for band in stage["bands"]
                                if band["depth"]["minimum"] <= depth and
                                (band["depth"]["kind"] == "open" or depth <= band["depth"]["maximum"])
                                for choice in band["choices"] if choice["archetype"] == row["content"])
                for future in authored.get("advancement", []):
                    if future["ordinal"] <= expected:
                        expected_choices[future["ordinal"] - 1] = {
                            "purchases": future["purchases"], "attribute": future.get("attribute")}
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

    def production_anchor_package(self, selected, depth):
        """Change only one actual starter anchor; retain production population verbatim."""
        production = ROOT / "scenarios/rogue-exploration"
        role = "warrior" if selected == 1 else "mage"
        output = self.directory / f"production-{role}-depth-{depth}"
        package = output / "package"
        shutil.copytree(production, package, ignore=shutil.ignore_patterns(".compiled"))
        source_text = (production / "scenario.toml").read_text(encoding="utf-8")
        source_manifest = tomllib.loads(source_text)
        manifest = copy.deepcopy(source_manifest)
        character = next(row for row in manifest["characters"] if row["id"] == selected)
        old_anchor = f'id = {selected}\nanchor = "{character["anchor"]}"'
        self.assertEqual(source_text.count(old_anchor), 1)
        character["anchor"] = f"dungeon/{depth}/up"
        diagnostic_text = source_text.replace(old_anchor, f'id = {selected}\nanchor = "{character["anchor"]}"', 1)
        self.assertEqual(tomllib.loads(diagnostic_text), manifest,
                         "The diagnostic may change only the selected character's anchor")
        (package / "scenario.toml").write_text(diagnostic_text, encoding="utf-8", newline="\n")
        for relative in [source_manifest["ruleset"], *source_manifest["modules"]]:
            self.assertEqual((package / relative).read_bytes(), (production / relative).read_bytes())
        validated = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                   capture_output=True, text=True, encoding="utf-8", timeout=30)
        self.assertEqual(validated.returncode, 0, validated.stderr)
        self.assertEqual((production / "scenario.toml").read_text(encoding="utf-8"), source_text)
        return output, manifest

    def test_depth_fourteen_wraith_birth_has_owned_rows_zero_stock_and_source_free_restart(self):
        output, manifest = self.production_anchor_package(1, 14)
        # Fixed seed 2 selects W/Y/N/L, including the required zero-stock Wraith.
        receipts = self.package_restart(output, 1, 14, 1, eligible_actors(manifest, 14),
                                        seed=2, production_policy=True)
        placed = [row for row in receipts[0]["decisions"][0]["result"]["attempts"]
                  if row["kind"] == "placed" and row["content"] == "rogue.wraith"]
        self.assertTrue(placed, "Fixed depth-14 entry must actually construct W")
        self.assertTrue(all(row["entity"]["carried_loot"] is None for row in placed))

    def test_depth_twenty_one_jabberwock_birth_has_owned_rows_stock_and_source_free_restart(self):
        output, manifest = self.production_anchor_package(1, 21)
        # Fixed production streams select M/F/P/J; the J carries one ration.
        receipts = self.package_restart(output, 1, 21, 1, eligible_actors(manifest, 21),
                                        seed=2, production_policy=True)
        placed = [row for row in receipts[0]["decisions"][0]["result"]["attempts"]
                  if row["kind"] == "placed" and row["content"] == "rogue.jabberwock"]
        self.assertTrue(placed, "Fixed depth-21 production entry must actually construct J")
        for row in placed:
            stock = row["entity"]["carried_loot"]
            self.assertEqual((stock["kind"], stock["content"], stock["quantity"]),
                             ("carried", "rogue.ration", 1))

    def test_depth_thirty_one_dragon_birth_owns_rank_three_stock_and_source_free_restart(self):
        # The actual full-26 policy owns the rank-three purchase at racial HD15.
        # Only the starting anchor changes; this is birth/restore, not survival.
        output, manifest = self.production_anchor_package(2, 31)
        receipts = self.package_restart(output, 2, 31, 1, eligible_actors(manifest, 31),
                                        seed=42, production_policy=True)
        placed = [row for row in receipts[0]["decisions"][0]["result"]["attempts"]
                  if row["kind"] == "placed" and row["content"] == "rogue.dragon"]
        self.assertTrue(placed, "Fixed depth-31 entry must actually construct Dragon with its HD15 purchase")
        for row in placed:
            self.assertEqual(row["entity"]["carried_loot"]["kind"], "carried")
            self.assertEqual(row["entity"]["carried_loot"]["quantity"], 1)

    def test_quagga_unicorn_candidate_boundary_survives_source_free_journal_restart(self):
        self.candidate_restart(2, 17, 0)

    def test_depth_twenty_six_candidate_retains_unicorn_vampire_jabberwock_receipts_without_earned_hd(self):
        self.candidate_restart(1, 26, 1)

    def test_depth_twenty_seven_mixed_tail_retains_exact_owned_rows_after_source_deletion(self):
        self.candidate_restart(2, 27, 1)

    def test_production_mage_depth_twenty_seven_keeps_mixed_growth_and_stock_after_source_deletion(self):
        # This source is actual production. Never call the candidate generator
        # or rebuild its floor policy here: only the Mage's diagnostic anchor
        # changes. It remains a birth/restore test, not deep combat survival.
        output, manifest = self.production_anchor_package(2, 27)
        package = output / "package"
        mage = next(row for row in manifest["characters"] if row["id"] == 2)
        activation = manifest["floor_activation"]
        self.assertEqual(activation["protagonist"], "selected_character")
        self.assertEqual(activation["families"], {"dungeon": "rogue.low_depth"})
        stages = activation["profiles"]["rogue.low_depth"]["stages"]
        self.assertEqual([row["id"] for row in stages],
                         ["rogue.eight_species_foes", "rogue.bandages", "rogue.rations",
                          "rogue.restore_strength", "rogue.room_gold", *MAGIC_ITEM_STAGE_IDS, *EQUIPMENT_ITEM_STAGE_IDS])
        self.assertEqual(stages[0]["count"], [4, 4])
        self.assertEqual(stages[0]["depth_growth"], {"kind": "racial_after", "depth": 26})
        self.assertEqual(stages[0]["ai"], "rogue_hostile")
        self.assertIs(stages[0]["avoid_protagonist_sight"], True)
        choices = [{"archetype": "rogue." + species, "weight": 1, "carried_loot": {
            "version": 1, "pool": "rogue.vampire_supplies",
            "chance": {"numerator": numerator, "denominator": denominator}}}
            for species, numerator, denominator in (("vampire", 1, 5), ("griffin", 1, 5),
                                                   ("jabberwock", 7, 10), ("medusa", 2, 5))]
        choices.append({"archetype": "rogue.dragon", "weight": 1,
            "carried_loot": {"version": 1, "pool": "rogue.vampire_supplies",
                             "chance": {"numerator": 1, "denominator": 1}},
            "advancement": [{"ordinal": ordinal, "purchases": [{"pool": "advancement", "selection": {
                "kind": "skill_rank", "skill": "breath"}}]} for ordinal in (15, 20, 25)]})
        at_entry = next(band for band in stages[0]["bands"] if band["depth"]["minimum"] == 27)
        self.assertEqual(at_entry, {"depth": {"kind": "finite", "minimum": 27, "maximum": 27},
                                    "choices": choices})
        self.assertEqual(stages[0]["bands"][-1], {
            "depth": {"kind": "open", "minimum": 32}, "choices": choices})
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
        self.package_restart(output, 2, 27, 1, eligible_actors(manifest, 27),
                                        seed=42, production_policy=True)

    def test_depth_267_mixed_tail_keeps_exact_owned_rows_after_source_deletion(self):
        # Every possible recipe fits here. Sampled J refuses entry from268;
        # valid all-V274 and rejected mixed268 entries belong to server growth tests.
        receipts = self.candidate_restart(1, 267, 1, seed=42)
        self.assertTrue(any(row["kind"] == "placed" and row["content"] == "rogue.jabberwock"
                            for row in receipts[0]["decisions"][0]["result"]["attempts"]),
                        "The fixed depth267 witness must reach the exact J256 owned-row assertion")

    def test_open_tail_birth_stock_success_and_miss_remain_pinned_after_source_deletion(self):
        # Seed 1 selects a stocked J and unstocked G/G/V; no process-time seed search.
        receipts = self.candidate_restart(2, 27, 1, seed=1)
        decisions = [row["entity"]["carried_loot"]["kind"]
                     for row in receipts[0]["decisions"][0]["result"]["attempts"]
                     if row["kind"] == "placed"]
        self.assertEqual(set(decisions), {"carried", "not_carried"})


class RogueMagicReceiptTests(unittest.TestCase):
    """Pure receipt fixtures retain the finite stock and ownership boundaries."""
    def receipts(self):
        return [{"id": identity, "version": 1, "result": {
            "kind": "evaluated", "requested": 1, "attempts": [{
                "kind": "placed", "content": content,
                "location": {"region": 1, "position": {"x": index, "y": 2, "z": 0}},
                "entity": {"kind": "item", "id": 100 + index, "quantity": 1}}]}}
            for index, (identity, content) in enumerate((
                ("rogue.magic_potions", "rogue.healing"),
                ("rogue.magic_scrolls", "rogue.identify_potion"),
                ("rogue.magic_devices", "rogue.wand_fire")))]

    def test_all_three_stages_return_every_placed_identity_for_cross_stage_duplicate_checks(self):
        self.assertEqual(assert_magic_item_receipts(self, self.receipts(), require_placed=True),
                         [("item", 100), ("item", 101), ("item", 102)])

    def test_every_reviewed_subtype_is_eligible_only_in_its_magic_stage(self):
        for index, choices in enumerate((
                ("rogue.healing", "rogue.confusion", "rogue.blindness"),
                ("rogue.identify_potion", "rogue.identify_scroll", "rogue.identify_wand", "rogue.sleep"),
                ("rogue.wand_fire", "rogue.wand_cold", "rogue.wand_lightning"))):
            for choice in choices:
                with self.subTest(stage=index, content=choice):
                    receipts = self.receipts()
                    receipts[index]["result"]["attempts"][0]["content"] = choice
                    self.assertEqual(len(assert_magic_item_receipts(self, receipts)), 3)
                    receipts[(index + 1) % 3]["result"]["attempts"][0]["content"] = choice
                    with self.assertRaises(AssertionError):
                        assert_magic_item_receipts(self, receipts)

    def test_no_eligible_cell_skip_has_no_owned_item_and_cannot_satisfy_seeded_placement(self):
        receipts = self.receipts()
        for stage in receipts:
            stage["result"]["attempts"] = [{"kind": "skipped",
                "content": stage["result"]["attempts"][0]["content"], "reason": "no_eligible_cell"}]
        self.assertEqual(assert_magic_item_receipts(self, receipts), [])
        with self.assertRaises(AssertionError):
            assert_magic_item_receipts(self, receipts, require_placed=True)
        for mutation in ({"reason": "chance_miss"}, {"entity": {"kind": "item", "id": 100, "quantity": 1}}):
            changed = copy.deepcopy(receipts)
            changed[0]["result"]["attempts"][0].update(mutation)
            with self.subTest(mutation=mutation), self.assertRaises(AssertionError):
                assert_magic_item_receipts(self, changed)

    def test_omitted_reordered_or_extra_magic_stages_are_rejected(self):
        receipts = self.receipts()
        for changed in (receipts[:2], receipts[::-1], receipts + [receipts[0]]):
            with self.subTest(ids=[stage["id"] for stage in changed]), self.assertRaises(AssertionError):
                assert_magic_item_receipts(self, changed)

    def test_count_quantity_identity_and_owned_location_contracts_are_strict(self):
        attempt = ("result", "attempts", 0)
        mutations = (
            (("version",), 2), (("version",), True),
            (("result", "kind"), "chance_miss"),
            (("result", "requested"), 0), (("result", "requested"), 2),
            (("result", "requested"), True), (("result", "attempts"), []),
            (("result", "attempts"), self.receipts()[0]["result"]["attempts"] * 2),
            ((*attempt, "kind"), "unknown"),
            ((*attempt, "entity", "kind"), "actor"),
            ((*attempt, "entity", "id"), 0), ((*attempt, "entity", "id"), 1 << 64),
            ((*attempt, "entity", "id"), True), ((*attempt, "entity", "id"), 101),
            ((*attempt, "entity", "quantity"), 0), ((*attempt, "entity", "quantity"), 2),
            ((*attempt, "entity", "quantity"), True),
            ((*attempt, "entity", "carried_loot"), None),
            ((*attempt, "location", "region"), 0), ((*attempt, "location", "region"), True),
            ((*attempt, "location", "position", "x"), "0"),
        )
        for path, value in mutations:
            receipts = self.receipts()
            target = receipts[0]
            for key in path[:-1]:
                target = target[key]
            target[path[-1]] = value
            with self.subTest(path=path, value=value), self.assertRaises(AssertionError):
                assert_magic_item_receipts(self, receipts)


class RogueEquipmentReceiptTests(unittest.TestCase):
    """Pure receipt fixtures retain the finite stock and ownership boundaries."""
    def receipts(self):
        return [{"id": identity, "version": 1, "result": {
            "kind": "evaluated", "requested": 1, "attempts": [{
                "kind": "placed", "content": content,
                "location": {"region": 1, "position": {"x": index, "y": 2, "z": 0}},
                "entity": {"kind": "item", "id": 100 + index, "quantity": 1}}]}}
            for index, (identity, content) in enumerate((
                ("rogue.equipment", "rogue.generated_mace"),
                ("rogue.equipment_scrolls", "rogue.enchant_armor")))]

    def test_both_equipment_stages_return_every_placed_identity_for_cross_stage_duplicate_checks(self):
        self.assertEqual(assert_equipment_item_receipts(self, self.receipts(), require_placed=True),
                         [("item", 100), ("item", 101)])

    def test_every_reviewed_subtype_is_eligible_only_in_its_equipment_stage(self):
        for index, choices in enumerate((
                ("rogue.generated_mace", "rogue.long_sword", "rogue.dagger", "rogue.two_handed_sword",
                 "rogue.spear", "rogue.leather_armor", "rogue.generated_ring_mail", "rogue.studded_leather",
                 "rogue.scale_mail", "rogue.chain_mail", "rogue.splint_mail", "rogue.banded_mail", "rogue.plate_mail"),
                ("rogue.enchant_armor", "rogue.protect_armor", "rogue.enchant_weapon", "rogue.remove_curse",
                 "rogue.identify_weapon", "rogue.identify_armor"))):
            for choice in choices:
                with self.subTest(stage=index, content=choice):
                    receipts = self.receipts()
                    receipts[index]["result"]["attempts"][0]["content"] = choice
                    self.assertEqual(len(assert_equipment_item_receipts(self, receipts)), 2)
                    receipts[(index + 1) % 2]["result"]["attempts"][0]["content"] = choice
                    with self.assertRaises(AssertionError):
                        assert_equipment_item_receipts(self, receipts)

    def test_no_eligible_cell_skip_has_no_owned_item_and_cannot_satisfy_seeded_placement(self):
        receipts = self.receipts()
        for stage in receipts:
            stage["result"]["attempts"] = [{"kind": "skipped",
                "content": stage["result"]["attempts"][0]["content"], "reason": "no_eligible_cell"}]
        self.assertEqual(assert_equipment_item_receipts(self, receipts), [])
        with self.assertRaises(AssertionError):
            assert_equipment_item_receipts(self, receipts, require_placed=True)
        for mutation in ({"reason": "chance_miss"}, {"entity": {"kind": "item", "id": 100, "quantity": 1}}):
            changed = copy.deepcopy(receipts)
            changed[0]["result"]["attempts"][0].update(mutation)
            with self.subTest(mutation=mutation), self.assertRaises(AssertionError):
                assert_equipment_item_receipts(self, changed)

    def test_omitted_reordered_or_extra_equipment_stages_are_rejected(self):
        receipts = self.receipts()
        for changed in (receipts[:1], receipts[::-1], receipts + [receipts[0]]):
            with self.subTest(ids=[stage["id"] for stage in changed]), self.assertRaises(AssertionError):
                assert_equipment_item_receipts(self, changed)

    def test_count_quantity_identity_and_owned_location_contracts_are_strict(self):
        attempt = ("result", "attempts", 0)
        mutations = (
            (("version",), 2), (("version",), True),
            (("result", "kind"), "chance_miss"),
            (("result", "requested"), 0), (("result", "requested"), 2),
            (("result", "requested"), True), (("result", "attempts"), []),
            (("result", "attempts"), self.receipts()[0]["result"]["attempts"] * 2),
            ((*attempt, "kind"), "unknown"),
            ((*attempt, "entity", "kind"), "actor"),
            ((*attempt, "entity", "id"), 0), ((*attempt, "entity", "id"), 1 << 64),
            ((*attempt, "entity", "id"), True), ((*attempt, "entity", "id"), 101),
            ((*attempt, "entity", "quantity"), 0), ((*attempt, "entity", "quantity"), 2),
            ((*attempt, "entity", "quantity"), True),
            ((*attempt, "entity", "carried_loot"), None),
            ((*attempt, "location", "region"), 0), ((*attempt, "location", "region"), True),
            ((*attempt, "location", "position", "x"), "0"),
        )
        for path, value in mutations:
            receipts = self.receipts()
            target = receipts[0]
            for key in path[:-1]:
                target = target[key]
            target[path[-1]] = value
            with self.subTest(path=path, value=value), self.assertRaises(AssertionError):
                assert_equipment_item_receipts(self, receipts)


    def test_generated_aliases_require_exact_archetype_and_shared_identity(self):
        for content, identity in (("rogue.generated_mace", "rogue.mace"),
                                  ("rogue.generated_ring_mail", "rogue.ring_mail"),
                                  ("rogue.long_sword", "rogue.long_sword"),
                                  ("rogue.enchant_armor", "rogue.enchant_armor"),
                                  ("rogue.healing", "rogue.healing")):
            item = {"spec": {"archetype": content, "identity": identity}}
            assert_finite_item_identity(self, item, content)
            for field in ("archetype", "identity"):
                forged = copy.deepcopy(item)
                forged["spec"][field] = "rogue.other"
                with self.subTest(content=content, field=field), self.assertRaises(AssertionError):
                    assert_finite_item_identity(self, forged, content)


class RogueActivationSourceTests(unittest.TestCase):
    def test_current_candidate_source_shape_reaches_receipt_driver(self):
        from pathlib import Path
        from unittest.mock import patch
        from rogue_activation_candidate import population_shape
        from rogue_roster_package import encounter_sources
        driver = RogueActivationCandidateProcesses()
        driver.directory, driver.bin, driver.suffix = Path("unused-output"), Path("compiler-bin"), ""
        manifest = encounter_sources()["manifest"]
        shape = population_shape(manifest)
        self.assertEqual(shape, "reviewed_twenty_six_species_input")

        def candidate(output, compiler, **kwargs):
            return {"format": "tor-rogue-activation-inputs-v2",
                    "source_provenance": kwargs["source_provenance"],
                    "source_population_shape": shape,
                    "artifact_purpose": "diagnostic_population_package", "production_mutated": False}

        with patch("rogue_activation_candidate.create_candidate", side_effect=AssertionError(
                "No compiler may run in this pure test")), patch(
                __name__ + ".create_candidate", side_effect=candidate), patch.object(
                driver, "package_restart", return_value=[]) as reopened:
            self.assertEqual(driver.candidate_restart(1, 21, 1), [])
        # Current production is accepted as input; the generated diagnostic
        # deliberately retains its historical eighteen-species population.
        diagnostic_eligible = {"rogue." + species for species in (
            "troll", "wraith", "venus_flytrap", "black_unicorn", "vampire", "griffin", "jabberwock")}
        self.assertEqual(eligible_actors(manifest, 21),
                         diagnostic_eligible | {"rogue.medusa", "rogue.phantom", "rogue.xeroc"})
        self.assertEqual(reopened.call_args.args[1:], (1, 21, 1, diagnostic_eligible))

    def check_production_anchor(self, method, selected, depth, seed, receipt):
        from pathlib import Path
        import tempfile
        from unittest.mock import patch
        driver = RogueActivationCandidateProcesses()
        production = ROOT / "scenarios/rogue-exploration"
        original = tomllib.loads((production / "scenario.toml").read_text(encoding="utf-8"))
        expected = copy.deepcopy(original)
        next(row for row in expected["characters"] if row["id"] == selected)["anchor"] = f"dungeon/{depth}/up"
        def restart(output, actual_selected, actual_depth, interval, actual_eligible, *, seed, production_policy):
            self.assertIs(production_policy, True)
            self.assertEqual((actual_selected, actual_depth, interval), (selected, depth, 1))
            self.assertEqual(actual_eligible, eligible_actors(original, depth))
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
                getattr(driver, method)()
            self.assertEqual(reopened.call_count, 1)
            self.assertEqual(reopened.call_args.kwargs["seed"], seed)
            self.assertEqual(compiled.call_count, 1)
            self.assertEqual(compiled.call_args.args[0], [driver.bin / "tor-scenario", "validate",
                driver.directory / f"production-{'warrior' if selected == 1 else 'mage'}-depth-{depth}/package"])
            self.assertEqual(compiled.call_args.kwargs["timeout"], 30)
        self.assertEqual(tomllib.loads((production / "scenario.toml").read_text()), original)

    def test_wraith_depth14_changes_only_production_warrior_anchor(self):
        receipt = [{"decisions": [{"result": {"attempts": [{"kind": "placed",
            "content": "rogue.wraith", "entity": {"carried_loot": None}}]}}]}]
        self.check_production_anchor(
            "test_depth_fourteen_wraith_birth_has_owned_rows_zero_stock_and_source_free_restart", 1, 14, 2,
            receipt)

    def test_jabberwock_depth21_changes_only_production_warrior_anchor(self):
        receipt = [{"decisions": [{"result": {"attempts": [{"kind": "placed",
            "content": "rogue.jabberwock", "entity": {"carried_loot": {
                "kind": "carried", "content": "rogue.ration", "quantity": 1}}}]}}]}]
        self.check_production_anchor(
            "test_depth_twenty_one_jabberwock_birth_has_owned_rows_stock_and_source_free_restart", 1, 21, 2,
            receipt)

    def test_dragon_depth31_changes_only_production_mage_anchor(self):
        receipt = [{"decisions": [{"result": {"attempts": [{"kind": "placed",
            "content": "rogue.dragon", "entity": {"carried_loot": {
                "kind": "carried", "content": "rogue.ration", "quantity": 1}}}]}}]}]
        self.check_production_anchor(
            "test_depth_thirty_one_dragon_birth_owns_rank_three_stock_and_source_free_restart", 2, 31, 42,
            receipt)

    def test_mixed_depth27_changes_only_production_mage_anchor(self):
        self.check_production_anchor(
            "test_production_mage_depth_twenty_seven_keeps_mixed_growth_and_stock_after_source_deletion", 2, 27, 42,
            [])


if __name__ == "__main__":
    unittest.main()
