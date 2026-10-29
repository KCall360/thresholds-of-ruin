"""Actual applications: exact racial birth rows and retryable floor capacity refusal."""
import copy
import json
import shutil
import sqlite3
import subprocess
import unittest
from contextlib import closing

from arena_matrix import toml_source
from checkpoint_payload import crc32c, decode_checkpoint_payload
from process_harness import ProcessTestCase, explore_to_stairs, underfoot
from process_work_budget import ProcessWorkBudget
from rogue_roster_package import PRODUCTION, encounter_sources, unrelated_fallback


class PopulationGrowthProcesses(ProcessTestCase):
    def setUp(self):
        super().setUp()
        self.work_budget = ProcessWorkBudget(requests=512, actor_ticks=3000,
                                             transcript_bytes=32 * 1024 * 1024)

    def tearDown(self):
        report = self.work_budget.report()
        (self.directory / "work-budget.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
        print("Process work budget: " + json.dumps(report, sort_keys=True), flush=True)
        if not self._failed():
            self.work_budget.check()
        super().tearDown()

    def client(self, *args, **kwargs):
        kwargs["omit_memory"] = True  # Current disclosure and saved authority are the test evidence.
        client, frame = super().client(*args, **kwargs)
        self.work_budget.track(client, frame)
        return client, frame

    def frame(self, client, predicate, seconds=15):
        return self.work_budget.frame_wait(super().frame, client, predicate, seconds=seconds)

    def command(self, client, value):
        self.work_budget.before()
        return super().command(client, value)

    def flush_save(self, *, actor=None):
        # The private save-barrier socket is a request even though it bypasses
        # the ordinary headless command helper.
        self.work_budget.before()
        super().flush_save(actor=actor)
        self.work_budget.check()

    def saved_records(self):
        self.flush_save()
        with closing(sqlite3.connect(self.save)) as database:
            rows = database.execute(
                "SELECT sequence,frame FROM journal WHERE sequence>0 "
                "UNION ALL SELECT sequence,frame FROM history WHERE sequence>0 "
                "ORDER BY sequence").fetchall()
        return [json.loads(frame[24:])["record"] for _, frame in rows]

    def saved_creature(self, actor):
        self.flush_save()
        with closing(sqlite3.connect(self.save.resolve().as_uri() + "?mode=ro", uri=True)) as database:
            payload, checksum = database.execute("SELECT payload, checksum FROM checkpoint").fetchone()
        self.assertEqual(crc32c(payload), checksum)
        checkpoint = json.loads(decode_checkpoint_payload(payload))
        self.assertFalse(checkpoint["wizard_game"])
        actors = checkpoint["shared"]["actors"][checkpoint["game"]["actors"]]
        return actors[str(actor)]["combat"]["creature"]

    def execution_refusal(self, player, *, since=0):
        # command()/client() consume response frames through their ready frame.
        # A scheduler error can arrive during that exchange; retain its typed
        # wire message rather than waiting for an error already consumed there.
        for line in player.transcript[since:]:
            if line.startswith("{"):
                message = json.loads(line).get("message") or {}
                if message.get("type") == "error":
                    break
        else:
            frame = self.frame(player, lambda row: (row.get("message") or {}).get("type") == "error")
            message = frame["message"]
        self.assertIsNone(message["request_id"], "refusal belongs to admitted execution")
        self.assertEqual(message["code"], "resource_limit")
        self.assertIn("capacity", message["message"])
        self.assertIn("no movement or population was committed", message["message"])
        self.assertNotIn("unicorn", message["message"])
        self.assertNotIn("snake", message["message"])
        self.assertNotIn("vampire", message["message"])
        return message

    def fixture(self, start, *, early_carrier=False, foe="rogue.black_unicorn"):
        source = encounter_sources()
        manifest = copy.deepcopy(source["manifest"])
        manifest["id"] = "population-growth"
        manifest["characters"] = [{"id": 1, "anchor": f"dungeon/{start}/up", "turn_ticks": 100}]
        manifest["default_character"] = 1
        manifest.pop("objective", None)
        manifest.pop("arena", None)
        manifest["procedural_worlds"]["dungeon"]["grid"] = [1, 1]
        manifest["floor_activation"] = {
            "version": 1, "protagonist": 1, "families": {"dungeon": "growth"},
            "profiles": {"growth": {"version": 1, "stages": [
                {"id": "foes", "version": 1, "kind": "actors", "count": [1, 1],
                 "depth_growth": {"kind": "racial_after", "depth": 26},
                 "bands": [{"depth": {"kind": "open", "minimum": start + 1},
                            "choices": [{"archetype": foe, "weight": 1}]}]},
            ]}},
        }
        if early_carrier:
            # This stage can fully construct its grown racial/Health rows and
            # ordinary three-item stack before the later 257-HD refusal.
            manifest["floor_activation"]["carried_item_pools"] = {
                "supplies": {"version": 1, "choices": [
                    {"archetype": "rogue.clean_bandage", "weight": 1, "quantity": 3},
                ]},
            }
            manifest["floor_activation"]["profiles"]["growth"]["stages"].insert(0, {
                "id": "early-carrier", "version": 1, "kind": "actors", "count": [1, 1],
                "depth_growth": {"kind": "racial_after", "depth": 26},
                "bands": [{"depth": {"kind": "open", "minimum": start + 1}, "choices": [{
                    "archetype": "rogue.snake", "weight": 1,
                    "carried_loot": {"version": 1, "pool": "supplies",
                                     "chance": {"numerator": 1, "denominator": 1}},
                }]}],
            })
        package = self.directory / f"growth-{start}"
        (package / "catalog").mkdir(parents=True)
        for module in manifest["modules"]:
            shutil.copy2(PRODUCTION / module, package / module)
        shutil.copy2(PRODUCTION / "rules.toml", package / "rules.toml")
        (package / "scenario.toml").write_text(toml_source(manifest), encoding="utf-8", newline="\n")
        result = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                text=True, capture_output=True, encoding="utf-8", timeout=60)
        self.assertEqual(result.returncode, 0, result.stderr)
        return package

    def growth_restart(self, foe, expected_hd):
        package = self.fixture(26, foe=foe)
        server = self.server("--checkpoint-interval", 1, scenario=package)
        player, ready = self.client(omit_memory=True)
        state = explore_to_stairs(self, player, ready["state"])
        self.assertTrue(underfoot(state)["stairs_down"])
        entered = self.act(player, {"type": "move", "direction": "down"})
        self.assertIsNone(entered["error"])
        self.assertTrue(underfoot(entered["state"])["stairs_up"])
        receipts = self.private_receipts()
        self.assertEqual([row["scope"] for row in receipts], ["dungeon/26", "dungeon/27"])
        actor = receipts[1]["decisions"][0]["result"]["attempts"][0]["entity"]["id"]
        # Generated actors are outside the player's startup control grant.
        # Inspect owned rows through saved authority, without granting control.
        born = self.saved_creature(actor)
        build = born["build"]
        self.assertEqual(len(build["hd"]), expected_hd)
        self.assertEqual([row["source"] for row in build["hd"]], [{"kind": "racial"}] * expected_hd)
        source = encounter_sources()
        recipe = source["actors"][foe]["creature"]
        # Authored native counts are7/8; the eighth/ninth effective HD is a
        # separately funded personal racial row, not a larger monster package.
        native_count, health_kind, health_die = {
            "rogue.black_unicorn": (7, "magical_beast", 10),
            "rogue.vampire": (8, "undead", 12),
        }[foe]
        form = recipe["body_package"]
        self.assertEqual(form, "rogue.body." + foe.removeprefix("rogue."))
        body_recipe = source["body_packages"][form]
        self.assertEqual(len(body_recipe["hit_dice"]), native_count)
        self.assertEqual(body_recipe["hit_dice"], recipe["hit_dice"])
        self.assertEqual(recipe.get("personal_body_grants", []), [])
        self.assertEqual(expected_hd, native_count + 1)
        owners = [{"kind": "body", "form": form, "ordinal": ordinal}
                  for ordinal in range(native_count)] + [{"kind": "personal", "id": 1}]
        expected_choices = [{
            "purchase_highwater": len(row["purchases"]),
            "purchases": [dict(purchase, id={"owner": owners[ordinal], "serial": serial})
                          for serial, purchase in enumerate(row["purchases"], 1)],
            "attribute": row.get("attribute"),
        } for ordinal, row in enumerate(body_recipe["hit_dice"])]
        personal_empty = {"purchase_highwater": 0, "purchases": [], "attribute": None}
        expected_choices.append(personal_empty)
        self.assertEqual(build["choices"], expected_choices)
        self.assertEqual(build["restoration"], {"kind": "choose_again"})
        self.assertIsNone(build["health_root_seed"])
        self.assertEqual(build["drained_hd"], [])
        body = build["body_build"]
        self.assertEqual(body["base"], body["active"])
        self.assertEqual(body["base"]["id"], form)
        self.assertEqual(body["base"]["species"], recipe["species"])
        self.assertEqual(body["base"]["body"], body_recipe["body"])
        self.assertEqual(body["base"]["advancement"], [{
            "source": {"kind": "racial"}, "choices": {
                "purchase_highwater": 0,
                "purchases": [dict(purchase, id=None) for purchase in row["purchases"]],
                "attribute": row.get("attribute"),
            }} for row in body_recipe["hit_dice"]])
        self.assertEqual(body["personal_seed_offset"], native_count)
        self.assertEqual(body["birth_personal_count"], 1)
        self.assertEqual(body["next_personal_id"], 2)
        self.assertEqual(body["personal_restoration"], {"kind": "authored", "entries": [
            {"id": 1, "source": {"kind": "racial"}, "choices": personal_empty}]})
        self.assertEqual(body["personal_body_grants"], [])
        self.assertEqual(body["inactive"], [])
        self.assertEqual(body["drained"], [])
        self.assertEqual(body["acquisition_species"],
                         [{"owner": owner, "species": recipe["species"]} for owner in owners])
        root_seed = body["health_root"]
        self.assertIs(type(root_seed), int)
        self.assertGreaterEqual(root_seed, 0)
        self.assertLess(root_seed, 1 << 64)
        # Independent SplitMix64 root/ordinal stream: neither ownership changes
        # nor the one personal appended row can reroll a native Health seed.
        expected_rows = []
        mask = (1 << 64) - 1
        for ordinal, owner in enumerate(owners):
            word = (root_seed + 0x9e3779b97f4a7c15 * (ordinal + 1)) & mask
            word = ((word ^ (word >> 30)) * 0xbf58476d1ce4e5b9) & mask
            word = ((word ^ (word >> 27)) * 0x94d049bb133111eb) & mask
            expected_rows.append({
                "source": {"kind": "racial"}, "owner": owner,
                "health_seed": word ^ (word >> 31),
                "health_policy": "maximized" if ordinal == 0 else "seeded",
                "health_origin": {"templates": [], "kind": health_kind, "die": health_die},
                "health_improvement": 0,
            })
        self.assertEqual(build["hd"], expected_rows)
        # Born depth rows establish PERSONAL milestone1; combat threat still
        # counts effective8/9HD plus2 trained skills and2 native talents.
        # Canonical weights70HD+5rank+10species+10type+10talent yield tier7.
        self.assertEqual(born["earned_progress"], {
            "authored_baseline": 1, "highest_applied_tier": 1, "highest_earned_tier": 1,
            "peak_threat": 7, "earned_xp": 0, "spent_xp": 0, "held_xp": 0,
        })
        before = self.request(player, {"type": "snapshot"})
        player.stop(); server.stop()
        shutil.rmtree(package)
        self.assertFalse(package.exists(), "restart must use only pinned saved source")
        server = self.server(scenario=unrelated_fallback(self), seed=None)
        player, restored = self.client(omit_memory=True)
        self.assertDisclosedStateEqual(restored["state"], before["state"])
        self.assertEqual(restored["history"], before["history"])
        # Real restore validates the root against every owned ordinal; compare
        # the complete creature to retain all rows, seeds and applied choices.
        self.assertEqual(self.saved_creature(actor), born)
        self.assertEqual(self.private_receipts(), receipts)
        player.stop(); server.stop()

    def capacity_restart(self, start_depth, foe):
        package = self.fixture(start_depth, early_carrier=True, foe=foe)
        server = self.server("--checkpoint-interval", 1, scenario=package)
        player, ready = self.client(omit_memory=True)
        state = explore_to_stairs(self, player, ready["state"])
        self.assertTrue(underfoot(state)["stairs_down"])
        before = self.request(player, {"type": "snapshot"})
        receipts = self.private_receipts()
        start = len(player.transcript)
        accepted = self.command(player, {"type": "act", "action": {"type": "move", "direction": "down"}})
        self.assertIsNone(accepted["error"], "the entry command must first be admitted")
        refused = self.execution_refusal(player, since=start)
        pending = self.request(player, {"type": "snapshot"})
        self.assertEqual(pending["state"]["observation"], before["state"]["observation"])
        self.assertEqual(self.private_receipts(), receipts)
        queued = [row for row in pending["intentions"] if row["phase"] == "queued"]
        self.assertEqual(len(queued), 1)
        records = self.saved_records()
        self.assertEqual(records[-1]["entry"]["id"], queued[0]["entry_id"])
        # Stop the server while the controller is connected, retaining the
        # flushed queued admission. Startup itself must perform the suspension.
        server.stop(); player.stop()
        shutil.rmtree(package)
        self.assertFalse(package.exists(), "restart must use only pinned saved source")
        server = self.server(scenario=unrelated_fallback(self), seed=None)
        player, restored = self.client(omit_memory=True)
        self.assertEqual(restored["state"]["observation"], before["state"]["observation"])
        self.assertEqual(self.private_receipts(), receipts)
        self.assertEqual(restored["intentions"], [dict(queued[0], phase="suspended")])
        suspended_records = self.saved_records()
        self.assertEqual(suspended_records[:-1], records)
        suspended = suspended_records[-1]
        self.assertEqual(suspended["entry"]["content"]["type"], "intention_changed")
        self.assertEqual(suspended["entry"]["content"]["change"], "suspended")
        self.assertEqual(suspended["entry"]["content"]["admission"], queued[0]["entry_id"])
        self.assertIsNone(suspended["receipt"])
        # Resume the original saved intention through ordinary client controls;
        # admitting another move would mask loss/replacement of that identity.
        start = len(player.transcript)
        resumed = self.command(player, {"type": "resume_intention"})
        self.assertIsNone(resumed["error"])
        self.assertEqual(resumed["intentions"], pending["intentions"])
        self.assertEqual(resumed["state"]["observation"], before["state"]["observation"])
        retried = self.execution_refusal(player, since=start)
        self.assertEqual(retried["code"], refused["code"])
        self.assertEqual(retried["message"], refused["message"])
        after_retry = self.request(player, {"type": "snapshot"})
        self.assertEqual(after_retry["state"]["observation"], before["state"]["observation"])
        self.assertEqual(after_retry["intentions"], pending["intentions"])
        self.assertEqual(after_retry["history"], resumed["history"])
        self.assertEqual(self.private_receipts(), receipts)
        retried_records = self.saved_records()
        self.assertEqual(retried_records[:-1], suspended_records)
        resumed_record = retried_records[-1]
        self.assertEqual(resumed_record["entry"]["content"]["type"], "intention_changed")
        self.assertEqual(resumed_record["entry"]["content"]["change"], "resumed")
        self.assertEqual(resumed_record["entry"]["content"]["admission"], queued[0]["entry_id"])
        self.assertFalse(any(record["entry"]["content"]["type"] == "intention_failed"
                             for record in retried_records))
        player.stop(); server.stop()


    def test_depth_26_to_27_materializes_rows_once_and_survives_source_deleted_restart(self):
        self.growth_restart("rogue.black_unicorn", 8)

    def test_vampire_depth_26_to_27_retains_nine_rows_and_purchases_without_source(self):
        self.growth_restart("rogue.vampire", 9)

    def test_capacity_refusal_preserves_floor_time_and_pending_entry_across_restart(self):
        self.capacity_restart(275, "rogue.black_unicorn")

    def test_vampire_depth_275_refuses_257_hd_and_earlier_stock_across_restart(self):
        self.capacity_restart(274, "rogue.vampire")

if __name__ == "__main__":
    unittest.main()
