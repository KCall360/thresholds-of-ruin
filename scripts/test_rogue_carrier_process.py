"""Generated C/Y/T/J composition, not an actual-kit, earned-route or balance study.

A labeled authored HD16 player provokes C/Y/J or waits for unprovoked mean T
acquisition, witnesses the natural sequence, then kills the foe ordinarily.
Fixed birth seeds avoid process searches; exact chance boundaries, pool
permutations and conditional rider outcomes have lower-level owners.
"""
import json
import shutil
import unittest

from process_harness import ProcessTestCase, settle_rogue_action
from process_work_budget import ProcessWorkBudget
from rogue_roster_package import encounter_sources, unrelated_fallback
from rogue_test_support import cardinal_melee_candidate, disclosed_actor_candidates, disclosed_contact_step
from rogue_vampire_package import generated_carrier_package
from scalar_wire import u64
from status_condition_package import actor_creature, checkpoint, combat, observation


class RogueCarrierProcesses(ProcessTestCase):
    def setUp(self):
        super().setUp()
        self.work_budget = ProcessWorkBudget(requests=300, actor_ticks=12000,
                                             transcript_bytes=16 * 1024 * 1024)

    def tearDown(self):
        report = self.work_budget.report()
        (self.directory / "work-budget.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
        print("Process budget " + self.id() + ": " + json.dumps(report, sort_keys=True), flush=True)
        if not self._failed():
            self.work_budget.check()
        super().tearDown()

    def client(self, *args, **kwargs):
        self.work_budget.check()
        kwargs["omit_memory"] = True  # This witness never asserts accumulated map memory.
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
        self.work_budget.before()
        super().flush_save(actor=actor)
        self.work_budget.check()

    def execute(self, player, action):
        current = settle_rogue_action(self, player, self.act(player, action))
        self.assertIsNone(current["error"])
        self.assertFalse(combat(current)["dead"], "Authored mechanism player must survive")
        return current

    def target(self, current, species):
        return next((row for row in disclosed_actor_candidates(observation(current)["visible_actors"])
                     if row["name"] == species), None)

    def approach(self, player, current, species):
        for _ in range(8):
            target = self.target(current, species)
            self.assertIsNotNone(target)
            if cardinal_melee_candidate(target):
                return current, target
            _, direction = disclosed_contact_step(observation(current), target)
            current = self.execute(player, {"type": "move", "direction": direction})
        self.fail("Carrier was not reached within eight ordinary steps")

    def observed_sequence(self, player, start, target, strikes, *, ability="basic_melee"):
        for raw in player.transcript[start:]:
            frame = json.loads(raw)
            view = (frame.get("state") or {}).get("observation")
            if view is None:
                continue
            events = [row for row in view["combat"]["events"] if row["type"] == "ability"
                      and row["caster"] == target and row["ability"]["id"] == ability]
            if [row["strike"] for row in events] == list(range(strikes)):
                return True
        return False

    def generated_carrier(self, species, seed, depth, strikes, stock_content, *, passive=True,
                          ability="basic_melee"):
        package = generated_carrier_package(self, species)
        server = self.server("--checkpoint-interval", 1, scenario=package, seed=seed)
        player, current = self.client()
        self.assertFalse(current["state"]["wizard_game"])
        self.assertEqual(u64(combat(current)["own_stats"]["hit_dice"]["total"]), 16)
        receipts = self.private_receipts()
        self.assertEqual(len(receipts), 1)
        self.assertEqual(receipts[0]["scope"], f"dungeon/{depth}")
        attempts = receipts[0]["decisions"][0]["result"]["attempts"]
        self.assertEqual(len(attempts), 1)
        born = attempts[0]
        self.assertEqual((born["kind"], born["content"]), ("placed", "rogue." + species))
        actor_id = born["entity"]["id"]
        stock = born["entity"]["carried_loot"]
        self.assertEqual((stock["kind"], stock["content"], stock["quantity"]),
                         ("carried", stock_content, 1))
        item_id = str(stock["item"])
        target = self.target(current, species)
        self.assertIsNotNone(target, "Tiny lit mechanism must disclose its ordinary newborn")
        target_id = target["id"]
        health = combat(current)["hp"]
        weapon = next(row["id"] for row in observation(current)["inventory"] if row["name"] == "mace")
        if passive:
            # Ordinary unequip gives a low-damage provocation without altering either
            # creature sheet. Re-equip only after observing the full natural sequence.
            current = self.execute(player, {"type": "unequip", "item": weapon})
            current, target = self.approach(player, current, species)
            current = self.execute(player, {"type": "wait"})
            before = checkpoint(self, player)
            self.assertEqual(combat(current)["hp"], health, "Passive newborn must not attack on sight or approach")
            self.assertIsNone(before["actors"][str(actor_id)]["behavior"]["engagement"])
        else:
            # No attack, approach or provocation: the first ordinary Wait also
            # creates the checkpoint; retain any strikes observed during it.
            start = len(player.transcript)
            current = self.execute(player, {"type": "wait"})
            before = checkpoint(self, player)
        creature = actor_creature(before, actor_id)
        source = encounter_sources()
        recipe = source["actors"]["rogue." + species]["creature"]
        build = creature["build"]
        body_count, personal_count, health_kind, threat = {
            "centaur": (1, 4, "monstrous_humanoid", 5),
            "yeti": (4, 0, "giant", 4),
            "troll": (6, 0, "giant", 5),
            "jabberwock": (15, 0, "aberration", 11),
        }[species]
        form = "rogue.body." + species
        self.assertEqual(recipe["body_package"], form)
        body_recipe = source["body_packages"][form]
        self.assertEqual(len(body_recipe["hit_dice"]), body_count)
        self.assertEqual(body_recipe["hit_dice"], recipe["hit_dice"][:body_count])
        self.assertEqual(len(recipe["hit_dice"]), body_count + personal_count)
        self.assertEqual(recipe.get("personal_body_grants", []), [])
        self.assertEqual(build["species"], "rogue." + species)
        self.assertEqual(len(build["hd"]), body_count + personal_count)
        sources = ([{"kind": "racial"}] * body_count +
                   [{"kind": "class", "id": "warrior"}] * personal_count)
        self.assertEqual([row["source"] for row in recipe["hit_dice"]],
                         ["racial"] * body_count + ["warrior"] * personal_count)
        owners = ([{"kind": "body", "form": form, "ordinal": ordinal}
                   for ordinal in range(body_count)] +
                  [{"kind": "personal", "id": ordinal + 1} for ordinal in range(personal_count)])
        expected_choices = [{
            "purchase_highwater": len(row["purchases"]),
            "purchases": [dict(purchase, id={"owner": owners[ordinal], "serial": serial})
                          for serial, purchase in enumerate(row["purchases"], 1)],
            "attribute": row.get("attribute"),
        } for ordinal, row in enumerate(recipe["hit_dice"])]
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
        self.assertEqual(body["personal_seed_offset"], body_count)
        self.assertEqual(body["birth_personal_count"], personal_count)
        self.assertEqual(body["next_personal_id"], personal_count + 1)
        self.assertEqual(body["personal_restoration"], {"kind": "authored", "entries": [
            {"id": ordinal + 1, "source": sources[body_count + ordinal],
             "choices": expected_choices[body_count + ordinal]} for ordinal in range(personal_count)]})
        self.assertEqual(body["personal_body_grants"], [])
        self.assertEqual(body["inactive"], [])
        self.assertEqual(body["drained"], [])
        self.assertEqual(body["acquisition_species"],
                         [{"owner": owner, "species": recipe["species"]} for owner in owners])
        root_seed = body["health_root"]
        self.assertIs(type(root_seed), int)
        self.assertGreaterEqual(root_seed, 0)
        self.assertLess(root_seed, 1 << 64)
        # Pin the approved composition: Centaur has a new maximized d8 body
        # plus4 seeded Warrior d10 rows; Y/T/J keep their racial d8 streams.
        # Do not alter encounter seeds or retaliation/death work budgets.
        mask = (1 << 64) - 1
        expected_rows = []
        for ordinal, owner in enumerate(owners):
            word = (root_seed + 0x9e3779b97f4a7c15 * (ordinal + 1)) & mask
            word = ((word ^ (word >> 30)) * 0xbf58476d1ce4e5b9) & mask
            word = ((word ^ (word >> 27)) * 0x94d049bb133111eb) & mask
            expected_rows.append({
                "source": sources[ordinal], "owner": owner,
                "health_seed": word ^ (word >> 31),
                "health_policy": "maximized" if ordinal == 0 else "seeded",
                "health_origin": {"templates": [], "kind": health_kind,
                                  "die": 8 if ordinal < body_count else 10},
                "health_improvement": 0,
            })
        self.assertEqual(build["hd"], expected_rows)
        # Centaur retains4 personal Warrior milestones, whereas Y/T/J have0.
        # Threat includes full effective HD, talents, Yeti's subtype and the
        # Centaur's single distinct class contribution (410points -> tier5).
        self.assertEqual(creature["earned_progress"], {
            "authored_baseline": personal_count, "highest_applied_tier": personal_count,
            "highest_earned_tier": personal_count,
            "peak_threat": threat, "earned_xp": 0, "spent_xp": 0, "held_xp": 0,
        })
        self.assertEqual(before["items"][item_id]["location"], {"Carried": actor_id})
        self.assertEqual(before["items"][item_id]["quantity"], 1)
        self.assertEqual({identity for identity, item in before["items"].items()
                          if item["location"] == {"Carried": actor_id}}, {item_id},
                         "A carrier owns exactly its single finite birth-stock stack")
        if passive:
            # The paid passive Wait can let a roaming carrier leave contact.
            # Reacquire only its current disclosed cardinal position.
            current, target = self.approach(player, current, species)
            self.assertEqual(target["id"], target_id)
            start = len(player.transcript)
            current = self.execute(player, {"type": "use_ability", "id": "basic_melee",
                "invocation": {"aim": {"kind": "actor_target", "target": target_id}, "weapon": {"kind": "default"}}})
        for _ in range(8):
            if self.observed_sequence(player, start, target_id, strikes, ability=ability):
                break
            current = self.execute(player, {"type": "wait"})
        else:
            self.fail(f"{species} did not resolve its {strikes}-strike sequence")
        if passive:
            current = self.execute(player, {"type": "equip", "item": weapon, "slot": 0})
        else:
            engaged = checkpoint(self, player)["actors"][str(actor_id)]["behavior"]["engagement"]
            self.assertEqual(engaged["target"], 1, "Mean Troll acquires the unprovoking player")
        for _ in range(24):
            target = self.target(current, species)
            if target is None:
                break
            current = self.execute(player, {"type": "use_ability", "id": "basic_melee",
                "invocation": {"aim": {"kind": "actor_target", "target": target["id"]}, "weapon": {"kind": "default"}}})
        else:
            self.fail("Carrier survived 24 ordinary finishing attacks")
        killed = checkpoint(self, player)
        dead = actor_creature(killed, actor_id)
        self.assertTrue(dead["dead"])
        self.assertEqual(dead["build"], creature["build"], "Combat preserves owned rows, purchases and Health seeds")
        self.assertGreater(actor_creature(killed, 1)["earned_progress"]["earned_xp"], 0)
        self.assertIn("Ground", killed["items"][item_id]["location"])
        self.assertEqual(killed["items"][item_id]["quantity"], 1)
        self.assertEqual(killed["items"][item_id]["spec"], before["items"][item_id]["spec"])
        self.assertEqual(self.private_receipts(), receipts, "Ordinary death cannot reroll birth stock")
        before_restart = self.request(player, {"type": "snapshot"})
        player.stop(); server.stop()
        shutil.rmtree(package)
        server = self.server("--checkpoint-interval", 1, scenario=unrelated_fallback(self), seed=None)
        player, restored = self.client()
        self.assertDisclosedStateEqual(restored["state"], before_restart["state"])
        self.assertEqual(restored["history"], before_restart["history"])
        self.assertEqual(checkpoint(self, player), killed)
        self.assertEqual(self.private_receipts(), receipts)
        player.stop(); server.stop()

    def test_generated_centaur_retaliates_and_retains_birth_stock_through_death_and_restart(self):
        self.generated_carrier("centaur", 2, 7, 3, "rogue.ration")

    def test_generated_yeti_retaliates_and_retains_birth_stock_through_death_and_restart(self):
        self.generated_carrier("yeti", 0, 11, 2, "rogue.clean_bandage")

    def test_generated_troll_acquires_unprovoked_and_retains_stock_through_death_and_restart(self):
        self.generated_carrier("troll", 1, 13, 3, "rogue.ration", passive=False)

    def test_generated_jabberwock_retaliates_with_rending_pair_and_retains_stock_after_restart(self):
        # This is the existing authored HD16 mechanism, not a depth21 starter
        # survival claim. Visible signature primaries do not disclose the raw
        # rider; the shared sequence/Service owners assert its threshold/once.
        self.generated_carrier("jabberwock", 0, 21, 2, "rogue.ration", ability="rending_pair")


class RogueCarrierObserverTests(unittest.TestCase):
    def test_carrier_cases_keep_fixed_seeds_ordinary_basics_and_explicit_j_signature(self):
        from unittest.mock import patch
        driver = RogueCarrierProcesses()
        for method, expected, options in (
            ("test_generated_centaur_retaliates_and_retains_birth_stock_through_death_and_restart",
             ("centaur", 2, 7, 3, "rogue.ration"), {}),
            ("test_generated_yeti_retaliates_and_retains_birth_stock_through_death_and_restart",
             ("yeti", 0, 11, 2, "rogue.clean_bandage"), {}),
            ("test_generated_troll_acquires_unprovoked_and_retains_stock_through_death_and_restart",
             ("troll", 1, 13, 3, "rogue.ration"), {"passive": False}),
            ("test_generated_jabberwock_retaliates_with_rending_pair_and_retains_stock_after_restart",
             ("jabberwock", 0, 21, 2, "rogue.ration"), {"ability": "rending_pair"}),
        ):
            with self.subTest(method=method), patch.object(driver, "generated_carrier") as witness:
                getattr(driver, method)()
                witness.assert_called_once_with(*expected, **options)

    def test_expected_signature_is_exact_and_cannot_weaken_basic_melee_witnesses(self):
        from types import SimpleNamespace
        driver = RogueCarrierProcesses()

        def player(ability, ordinals=(0, 1), caster="foe"):
            events = [{"type": "ability", "caster": caster, "strike": ordinal,
                       "ability": {"id": ability}} for ordinal in ordinals]
            return SimpleNamespace(transcript=[json.dumps({"state": {"observation": {
                "combat": {"events": events}}}})])

        basic, signature = player("basic_melee"), player("rending_pair")
        self.assertTrue(driver.observed_sequence(basic, 0, "foe", 2))
        self.assertFalse(driver.observed_sequence(signature, 0, "foe", 2))
        self.assertTrue(driver.observed_sequence(signature, 0, "foe", 2, ability="rending_pair"))
        self.assertFalse(driver.observed_sequence(basic, 0, "foe", 2, ability="rending_pair"))
        for invalid in (player("rending_pair", (0,)), player("rending_pair", (1, 0)),
                        player("rending_pair", (0, 1, 1)), player("rending_pair", caster="other")):
            self.assertFalse(driver.observed_sequence(invalid, 0, "foe", 2, ability="rending_pair"))
        self.assertFalse(driver.observed_sequence(signature, 1, "foe", 2, ability="rending_pair"))


if __name__ == "__main__":
    unittest.main()
