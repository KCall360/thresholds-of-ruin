"""One bounded ordinary L/N/O mechanism sequence through the actual server.

The generated floor, finite stock, synthetic durable victim and neutral factions
are authored fixture context. Production L/N/O species, purchases, AI and shared
Filch rules are copied unchanged. This is not starter survival or balance data.
The victim's harmless natural contact provokes normally. Its fixture-owned +32
basic-melee flat bonus affects an explicitly mundane fixture mace's Impact primary,
ending recovery on one successful hit before unrelated flee-chasing. The empty
unarmed effect branch stays harmless. No runtime edits or seed search.

Unchanged application-boundary assertion owners (not multiplied per species):
* test_items_process.ItemProcesses.test_direct_quantities_merge_drop_and_restart:
  Text inventory counts and durable restart; test_adventure_quantities_count_from_
  one_stack: unknown potion appearance without identity disclosure.
* test_items_process.ItemProcesses.test_ascii_quantity_picker_and_drop_present_
  authoritative_counts: launched ASCII inventory quantities and hidden identity.
* test_ascii_interactions_process.AsciiInteractionProcesses.test_native_equipment_
  removal_and_unknown_potion_choices: genuine native inventory/equipment input.
* test_ascii_process.AsciiProcesses.test_read_only_spectator_window_streams_actions_
  and_resumes: read-only native spectator disclosure, inventory/history and restart.
* test_stream_recovery_process.StreamRecoveryProcesses.test_text_delayed_delivery_
  gap_and_relaunch and NativeStreamRecoveryProcesses.test_ascii_delayed_delivery_
  gap_and_relaunch: atomic transport repair and client relaunch.

Local invocation after the compiler owner releases a qualified binary directory:
TOR_TEST_BINARIES=/absolute/path/to/debug python scripts/test_runner.py \
    test_rogue_theft_process -v
"""
import copy
import json
import shutil
import subprocess
import unittest

from arena_matrix import toml_source
from process_harness import ProcessTestCase, SPECTATOR_TOKEN, settle_rogue_action, underfoot
from process_work_budget import ProcessWorkBudget
from rogue_roster_package import PRODUCTION, encounter_sources, unrelated_fallback
from rogue_test_support import cardinal_melee_candidate, disclosed_actor_candidates, disclosed_contact_step
from status_condition_package import actor_creature, checkpoint, combat, observation


GOLD = "rogue.gold"
POTION = "rogue.restore_strength"
MUNDANE_MACE = "fixture.mundane_mace"


def theft_package(test, species):
    """Finite controlled context, with genuine generated scope and production foes."""
    source = encounter_sources()
    manifest = {key: copy.deepcopy(source["manifest"][key]) for key in
                ("format", "version", "ruleset", "modules", "themes", "assets")}
    manifest.update(id="rogue-theft-" + species, default_character=1,
                    factions={"delvers": [], "dungeon_foes": []},
                    ai_profiles={"ordinary": {}}, capabilities={"npc_stair_traversal": False})
    victim = copy.deepcopy(source["species"]["rogue.human"])
    victim["melee"]["bonus"] = 100
    victim["melee"]["strikes"][0]["on_success"] = []
    # Initial species attributes obey the shared cap of five. This labeled
    # mechanism's effective Strength/Speed use the ordinary source modifier
    # fold instead, retaining the intended bounded combat and pursuit timings.
    victim["modifiers"] = [
        *[{"scope": "always", "op": {"kind": "set", "stat": {
            "kind": "attribute", "attribute": attribute}, "value": value}}
          for attribute, value in (("strength", 20), ("speed", 10))],
        {"scope": "always", "op": {"kind": "add", "stat": {"kind": "health"}, "value": 500}},
        # Mundane mace minimum 2d4+32 =34 exceeds L/N native Health and
        # the approved Orc racial-body+Warrior Health. A flat modifier cannot
        # create effects in the empty unarmed
        # branch; the ordinary selected-weapon check still must succeed.
        {"scope": "always", "op": {"kind": "add", "stat": {
            "kind": "ability", "ability": "basic_melee", "stat": "flat_bonus"}, "value": 32}},
        *[{"scope": "always", "op": {"kind": "add", "stat": {
            "kind": "defense", "defense": defense}, "value": -100}}
          for defense in ("physical", "cognitive")]]
    manifest["creatures"] = {"species": {"fixture.theft_witness": victim}}
    character = copy.deepcopy(source["manifest"]["characters"][0])
    body = copy.deepcopy(source["body_packages"][character["creature"]["body_package"]])
    body["species"] = "fixture.theft_witness"
    manifest["creatures"]["body_packages"] = {"fixture.theft_witness.body": body}
    character["creature"]["body_package"] = "fixture.theft_witness.body"
    character.update(anchor="dungeon/1/start", known_identities=[GOLD, MUNDANE_MACE, "rogue.ring_mail"],
                     loadout=[{"archetype": MUNDANE_MACE, "quantity": 1},
                              {"archetype": "rogue.ring_mail", "quantity": 1},
                              {"archetype": GOLD, "quantity": 9 if species == "orc" else 2}])
    character["creature"].update(species="fixture.theft_witness", name="Authored theft witness",
                                 nutrition={"kind": "exempt"}, future_growth={"kind": "disabled"})
    if species == "nymph":
        character["loadout"].append({"archetype": POTION, "quantity": 1})
    manifest["characters"] = [character]
    manifest["generation_recipes"] = {"room": {"kind": "grid_rooms", "version": 1,
        "room_width": [7, 7], "room_height": [5, 5], "extra_connections": [0, 0]}}
    manifest["procedural_worlds"] = {"dungeon": {"kind": "grid_chain", "version": 1,
        "base_region": 1, "grid": [1, 1], "size": [9, 7, 2], "recipe": "room",
        "domain": {"kind": "open", "max_depth": 2}}}
    choice = {"archetype": "rogue." + species, "weight": 1}
    pools = {}
    if species == "leprechaun":
        pools["rogue.leprechaun_gold"] = copy.deepcopy(
            source["manifest"]["floor_activation"]["carried_item_pools"]["rogue.leprechaun_gold"])
        choice["carried_loot"] = {"version": 1, "pool": "rogue.leprechaun_gold",
                                  "chance": {"numerator": 1, "denominator": 1}}
    manifest["floor_activation"] = {"version": 1, "protagonist": "selected_character",
        "families": {"dungeon": "witness"}, "carried_item_pools": pools,
        "profiles": {"witness": {"version": 1, "stages": [{"id": "ordinary_foe", "version": 1,
            "kind": "actors", "count": [1, 1], "ai": "ordinary", "bands": [{
                "depth": {"kind": "open", "minimum": 1}, "choices": [choice]}]}]}}}
    package = test.directory / ("theft-" + species)
    (package / "catalog").mkdir(parents=True)
    for module in manifest["modules"]:
        shutil.copy2(PRODUCTION / module, package / module)
    # The production starter mace now carries magical +1/+1 birth state and is
    # correctly eligible for theft while unequipped. This mechanism witness
    # needs mundane recovery gear while its empty natural contact provokes.
    mundane = copy.deepcopy(source["items"]["rogue.mace"])
    mundane.update(identity=MUNDANE_MACE, name="mundane witness mace",
                   equipment_generation={"kind": "authored"})
    fixture_module = "catalog/theft-fixture.toml"
    manifest["modules"].append(fixture_module)
    (package / fixture_module).write_text(toml_source(
        {"namespace": "fixture", "items": {MUNDANE_MACE: mundane}}),
        encoding="utf-8", newline="\n")
    shutil.copy2(PRODUCTION / "rules.toml", package / "rules.toml")
    (package / "scenario.toml").write_text(toml_source(manifest), encoding="utf-8", newline="\n")
    result = subprocess.run([test.bin / ("tor-scenario" + test.suffix), "validate", package],
                            capture_output=True, text=True, encoding="utf-8", timeout=30)
    (test.directory / (species + "-validation.log")).write_text(
        result.stdout + result.stderr, encoding="utf-8")
    test.assertEqual(result.returncode, 0, result.stderr)
    return package


def carried(saved, actor, identity):
    return {key: item for key, item in saved["items"].items()
            if item["location"] == {"Carried": actor} and item["spec"]["identity"] == identity}


def quantity(saved, actor, identity):
    return sum(item["quantity"] for item in carried(saved, actor, identity).values())


def inventory(frame, name):
    return [item for item in observation(frame)["inventory"] if item["name"] == name]


class RogueTheftProcesses(ProcessTestCase):
    def setUp(self):
        super().setUp()
        # Three finite arms share one non-resetting aggregate budget. The first
        # retained L trace projects to 6.02 MB through restored checkpoint, before
        # death/recovery; allow 16 MiB per complete arm and 48 MiB total. These
        # bound fixture diagnostics, not production performance or game balance.
        self.work_budget = ProcessWorkBudget(requests=500, actor_ticks=20000,
                                             transcript_bytes=48 * 1024 * 1024)
        self.arm_budgets = {}
        self.arm_budget = None

    def budgets(self):
        return [self.work_budget, *([self.arm_budget] if self.arm_budget is not None else [])]

    def check_budgets(self):
        for budget in self.budgets():
            budget.check()

    def tearDown(self):
        report = self.work_budget.report()
        report["arms"] = {species: budget.report() for species, budget in self.arm_budgets.items()}
        (self.directory / "work-budget.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
        print("Theft process budget: " + json.dumps(report, sort_keys=True), flush=True)
        if not self._failed():
            self.check_budgets()
        super().tearDown()

    def launch(self, *args, **kwargs):
        self.check_budgets()
        process = super().launch(*args, **kwargs)
        # Include the Text spectator and server logs, as well as headless output.
        for budget in self.budgets():
            budget.output.paths += (process.transcript.path,)
        self.check_budgets()
        return process

    def client(self, *args, **kwargs):
        self.check_budgets()
        # Assertions use current state/history and raw lifecycle updates, never
        # accumulated memory or repeated state on palette/map/waiting frames.
        kwargs["omit_memory"] = True
        kwargs["compact_background"] = True
        client, frame = super().client(*args, **kwargs)
        for budget in self.budgets():
            budget.track(client, frame)
        return client, frame

    def command(self, client, value):
        for budget in self.budgets():
            budget.before()
        result = super().command(client, value)
        for budget in self.budgets():
            budget.after(client, result)
        return result

    def frame(self, client, predicate, seconds=15):
        self.check_budgets()

        def checked(frame):
            if self.arm_budget is not None:
                self.arm_budget.after(client, frame)
            return predicate(frame)

        return self.work_budget.frame_wait(super().frame, client, checked, seconds=seconds)

    def execute(self, player, action):
        return settle_rogue_action(self, player, self.act(player, action))

    def target(self, frame, species):
        return next((row for row in disclosed_actor_candidates(observation(frame)["visible_actors"])
                     if row["name"] == species), None)

    def approach(self, player, current, species):
        for _ in range(16):
            target = self.target(current, species)
            self.assertIsNotNone(target, "The one-room witness must disclose its ordinary foe")
            if cardinal_melee_candidate(target):
                return current, target
            _, direction = disclosed_contact_step(observation(current), target)
            current = self.execute(player, {"type": "move", "direction": direction})
        self.fail(f"The disclosed {species} could not be approached in sixteen ordinary steps")

    def ability_seen(self, player, start, caster, ability):
        return any(event["caster"] == caster and event["ability"]["id"] == ability
                   for raw in player.transcript[start:] if raw.startswith("{")
                   for frame in [json.loads(raw)] if frame.get("state") is not None
                   for event in combat(frame)["events"] if event["type"] == "ability")

    def assert_vanished_at_transfer(self, player, start, initial, species, stolen_name):
        """The same observation publication removes inventory and moves the thief."""
        prior = initial
        for raw in player.transcript[start:]:
            frame = json.loads(raw)
            if frame.get("state") is None:
                continue
            if inventory(prior, stolen_name) and not inventory(frame, stolen_name):
                self.assertEqual(underfoot(prior["state"])["key"], underfoot(frame["state"])["key"])
                before, after = self.target(prior, species), self.target(frame, species)
                self.assertIsNotNone(before)
                self.assertIsNotNone(after, "The lit room must disclose the relocated actor")
                self.assertEqual(before["id"], after["id"])
                self.assertNotEqual(before["position"], after["position"])
                self.assertEqual(after["position"]["z"], 0)
                for z in (0, 1):
                    destination = next(cell for cell in observation(frame)["visible_cells"]
                                       if cell["position"] == {**after["position"], "z": z})
                    self.assertFalse(destination["wall"], "Both production body cells must fit")
                return
            prior = frame
        self.fail("No atomic successful-transfer/vanish publication was observed")

    def take_dropped(self, player, current, name):
        for _ in range(16):
            stacks = [row for row in observation(current)["ground_items"] if row["item"]["name"] == name]
            self.assertTrue(stacks, f"Ordinary death must disclose recoverable {name}")
            stack = min(stacks, key=lambda row: (abs(row["position"]["x"]) + abs(row["position"]["y"]),
                                                 row["position"]["x"], row["position"]["y"]))
            p = stack["position"]
            if p == {"x": 0, "y": 0, "z": 0}:
                return self.execute(player, {"type": "take", "item": stack["item"]["id"], "quantity": None})
            direction = ("east" if p["x"] > 0 else "west") if p["x"] else ("south" if p["y"] > 0 else "north")
            current = self.execute(player, {"type": "move", "direction": direction})
        self.fail("Dropped loot was not recovered within the bounded room")

    def orc_collects_public_gold(self, player, current, actor):
        current, _ = self.approach(player, current, "orc")
        coin = inventory(current, "gold coin")[0]
        current = self.execute(player, {"type": "drop", "item": coin["id"], "quantity": "7"})
        occupied = {(row["position"]["x"], row["position"]["y"])
                    for row in observation(current)["visible_actors"] if row["position"]["z"] == 0}
        directions = {(-1, 0): "west", (1, 0): "east", (0, -1): "north", (0, 1): "south"}
        choices = [cell for cell in observation(current)["visible_cells"]
                   if cell["position"]["z"] == 0 and not cell["wall"]
                   and (cell["position"]["x"], cell["position"]["y"]) in directions
                   and (cell["position"]["x"], cell["position"]["y"]) not in occupied]
        self.assertTrue(choices)
        cell = min(choices, key=lambda row: (row["position"]["x"], row["position"]["y"]))
        current = self.execute(player, {"type": "move", "direction": directions[
            cell["position"]["x"], cell["position"]["y"]]})
        for _ in range(12):
            # Decide only from current public ground disclosure. Private saved
            # ownership is checked afterward, never used as the movement oracle.
            if not any(row["item"]["name"] == "gold coin" for row in observation(current)["ground_items"]):
                saved = checkpoint(self, player)
                self.assertEqual(quantity(saved, actor, GOLD), 7)
                self.assertEqual(quantity(saved, 1, GOLD), 2, "An Orc cannot inspect or steal the private purse")
                return current
            current = self.execute(player, {"type": "wait"})
        self.fail("Passive Orc did not acquire the ordinarily dropped, visible seven gold")

    def assert_newborn_ownership(self, creature, species):
        source = encounter_sources()
        recipe = source["actors"]["rogue." + species]["creature"]
        build = creature["build"]
        body_count, personal_count, health_kind, racial_die, threat = {
            "leprechaun": (3, 0, "fey", 6, 3),
            "nymph": (3, 0, "fey", 6, 3),
            "orc": (1, 1, "humanoid", 8, 2),
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
        # L/N retain3 racial d6 body rows. Orc keeps the approved racial
        # d8 body plus1 personal Warrior d10 row; only its body is maximized.
        # Theft, finite stock and the original recovery budget stay unchanged.
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
                                  "die": racial_die if ordinal < body_count else 10},
                "health_improvement": 0,
            })
        self.assertEqual(build["hd"], expected_rows)
        # Personal tiers are0 for L/N and1 for Orc. Threat is independent:
        # L/N each260points->tier3; Orc190points (including class)->tier2.
        self.assertEqual(creature["earned_progress"], {
            "authored_baseline": personal_count, "highest_applied_tier": personal_count,
            "highest_earned_tier": personal_count,
            "peak_threat": threat, "earned_xp": 0, "spent_xp": 0, "held_xp": 0,
        })

    def test_fixed_generated_l_n_o_keep_finite_loot_recoverable_after_source_free_restore(self):
        fallback = unrelated_fallback(self)
        for species, signature in (("leprechaun", "filch_gold"), ("nymph", "filch_charm"), ("orc", "measured_cut")):
            # subTest records and suppresses failures; do not launch another
            # fixture/server after any failure, especially an exhausted budget.
            if self._failed():
                return
            self.check_budgets()
            self.assertNotIn(species, self.arm_budgets, "Finite arms cannot reset their budgets")
            self.arm_budget = ProcessWorkBudget(requests=500, actor_ticks=20000,
                                                transcript_bytes=16 * 1024 * 1024)
            self.arm_budgets[species] = self.arm_budget
            with self.subTest(species=species):
                self.save = self.directory / (species + ".db")
                package = theft_package(self, species)
                server = self.server("--checkpoint-interval", 1, scenario=package, seed=42)
                player, current = self.client()
                self.assertFalse(current["state"]["wizard_game"])
                weapon = next(row for row in observation(current)["inventory"]
                              if row["class"] == "weapon")
                weapon_id = weapon["id"]
                receipts = self.private_receipts()
                self.assertEqual(len(receipts), 1)
                self.assertEqual(receipts[0]["scope"], "dungeon/1")
                born = receipts[0]["decisions"][0]["result"]["attempts"][0]
                self.assertEqual((born["kind"], born["content"]), ("placed", "rogue." + species))
                actor = born["entity"]["id"]
                current = self.execute(player, {"type": "wait"})
                initial = checkpoint(self, player)
                creature = actor_creature(initial, actor)
                self.assertEqual(creature["injury"], 0)
                self.assertFalse(creature["dead"])
                self.assert_newborn_ownership(creature, species)
                self.assertIsNone(initial["actors"][str(actor)]["behavior"]["engagement"])
                hp = combat(current)["hp"]
                birth = quantity(initial, actor, GOLD)
                if species == "leprechaun":
                    stock = born["entity"]["carried_loot"]
                    self.assertEqual((stock["kind"], stock["content"], stock["quantity"]), ("carried", GOLD, birth))
                    self.assertLessEqual(2, birth)
                    self.assertLessEqual(birth, 61)
                else:
                    self.assertEqual(birth, 0)
                if species == "orc":
                    current = self.orc_collects_public_gold(player, current, actor)
                current, target = self.approach(player, current, species)
                self.assertEqual(combat(current)["hp"], hp, "Passive actors cannot attack an unprovoking observer")
                stolen_name = (next(row["name"] for row in observation(current)["inventory"]
                                    if row["class"] == "potion") if species == "nymph" else "gold coin")
                before = current
                start = len(player.transcript)
                current = self.execute(player, {"type": "use_ability", "id": "basic_melee",
                    "invocation": {"aim": {"kind": "actor_target", "target": target["id"]}, "weapon": {"kind": "default"}}})
                for _ in range(12):
                    if self.ability_seen(player, start, target["id"], signature) and (
                            species == "orc" or not inventory(current, stolen_name)):
                        break
                    current = self.execute(player, {"type": "wait"})
                else:
                    self.fail(f"Fixed seed42 {species} did not execute its signature/transfer within twelve waits")
                transferred = checkpoint(self, player)
                self.assertEqual(actor_creature(transferred, actor)["injury"], creature["injury"],
                                 "Empty natural provocation must stay harmless despite the fixture flat bonus")
                self.assertFalse(actor_creature(transferred, actor)["dead"])
                self.assertEqual(actor_creature(transferred, actor)["build"], creature["build"],
                                 "Theft preserves original body/class ownership and Health stream")
                self.assertEqual(transferred["actors"][str(actor)]["location"]["region"], 1)
                if species == "orc":
                    identity, expected = GOLD, 7
                    self.assertEqual(quantity(transferred, 1, GOLD), 2)
                elif species == "leprechaun":
                    identity, expected = GOLD, birth + 2
                    self.assertEqual(quantity(transferred, 1, GOLD), 0)
                else:
                    identity, expected = POTION, 1
                    self.assertEqual(combat(current)["hp"], hp, "Nymph contact and theft have zero damage")
                    self.assertNotIn(POTION, transferred["actors"][str(actor)]["knowledge"])
                    self.assertNotIn(POTION, transferred["actors"]["1"]["knowledge"])
                    self.assertTrue(inventory(current, "ring mail"), "Mundane unequipped armor is ineligible")
                    self.assertTrue(any(row["id"] == weapon_id
                                        for row in observation(current)["inventory"]),
                                    "The exact mundane weapon must remain with its owner")
                self.assertEqual(quantity(transferred, actor, identity), expected)
                if species != "orc":
                    self.assert_vanished_at_transfer(player, start, before, species, stolen_name)
                retained = carried(transferred, actor, identity)
                before_restart = self.request(player, {"type": "snapshot"})
                player.stop(); server.stop()
                shutil.rmtree(package)
                server = self.server("--checkpoint-interval", 1, scenario=fallback, seed=None)
                player, current = self.client()
                self.assertDisclosedStateEqual(current["state"], before_restart["state"])
                self.assertEqual(current["history"], before_restart["history"])
                self.assertEqual(checkpoint(self, player), transferred)
                self.assertEqual(self.private_receipts(), receipts, "Restore must not regenerate finite stock")
                self.assertTrue(any(row["id"] == weapon_id
                                    for row in observation(current)["inventory"]))
                current = self.execute(player, {"type": "equip", "item": weapon_id, "slot": 0})
                for _ in range(12):
                    if self.target(current, species) is None:
                        break
                    current, target = self.approach(player, current, species)
                    current = self.execute(player, {"type": "use_ability", "id": "basic_melee",
                        "invocation": {"aim": {"kind": "actor_target", "target": target["id"]}, "weapon": {"kind": "default"}}})
                else:
                    self.fail("Ordinary equipped recovery did not defeat the thief within twelve attacks")
                killed = checkpoint(self, player)
                self.assertTrue(actor_creature(killed, actor)["dead"])
                self.assertEqual(actor_creature(killed, actor)["build"], creature["build"],
                                 "Ordinary death does not reconstruct or duplicate ownership")
                for key, item in retained.items():
                    self.assertIn("Ground", killed["items"][key]["location"])
                    self.assertEqual(killed["items"][key]["quantity"], item["quantity"])
                    self.assertEqual(killed["items"][key]["spec"], item["spec"])
                current = self.take_dropped(player, current, stolen_name)
                recovered = checkpoint(self, player)
                total = expected + (2 if species == "orc" else 0)
                self.assertEqual(quantity(recovered, 1, identity), total)
                self.assertEqual(self.private_receipts(), receipts, "Death and pickup cannot reroll birth gold")
                if species == "nymph":
                    potion = inventory(current, stolen_name)
                    self.assertEqual(len(potion), 1)
                    self.assertFalse(potion[0]["identified"])
                    text, welcome = self.text_client(SPECTATOR_TOKEN, observe=True)
                    self.assertIn("read-only", welcome)
                    presented = text.command("inventory")
                    self.assertIn(stolen_name, presented)
                    self.assertNotIn("potion of restore strength", presented)
                    text.stop()
                player.stop(); server.stop()


if __name__ == "__main__":
    unittest.main()
