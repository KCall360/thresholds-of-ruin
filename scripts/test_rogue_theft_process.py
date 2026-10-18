"""One bounded ordinary L/N/O mechanism sequence through the actual server.

The generated floor, finite stock, synthetic durable victim and neutral factions
are authored fixture context. Production L/N/O species, purchases, AI and shared
Filch rules are copied unchanged. This is not starter survival or balance data.
The victim's harmless natural contact provokes normally; equipping the production
mace later permits ordinary death/drop/recovery. No runtime edits or seed search.

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


def theft_package(test, species):
    """Finite controlled context, with genuine generated scope and production foes."""
    source = encounter_sources()
    manifest = {key: copy.deepcopy(source["manifest"][key]) for key in
                ("format", "version", "ruleset", "modules", "themes", "assets")}
    manifest.update(id="rogue-theft-" + species, default_character=1,
                    factions={"delvers": [], "dungeon_foes": []},
                    ai_profiles={"ordinary": {}}, capabilities={"npc_stair_traversal": False})
    victim = copy.deepcopy(source["species"]["rogue.human"])
    victim["attributes"].update(strength=20, speed=10)
    victim["melee"]["bonus"] = 100
    victim["melee"]["strikes"][0]["on_success"] = []
    victim["modifiers"] = [
        {"scope": "always", "op": {"kind": "add", "stat": {"kind": "health"}, "value": 500}},
        *[{"scope": "always", "op": {"kind": "add", "stat": {
            "kind": "defense", "defense": defense}, "value": -100}}
          for defense in ("physical", "cognitive")]]
    manifest["creatures"] = {"species": {"fixture.theft_witness": victim}}
    character = copy.deepcopy(source["manifest"]["characters"][0])
    character.update(anchor="dungeon/1/start", known_identities=[GOLD, "rogue.mace", "rogue.ring_mail"],
                     loadout=[{"archetype": "rogue.mace", "quantity": 1},
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
        self.work_budget = ProcessWorkBudget(requests=500, actor_ticks=20000,
                                             transcript_bytes=16 * 1024 * 1024)

    def tearDown(self):
        report = self.work_budget.report()
        (self.directory / "work-budget.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
        print("Theft process budget: " + json.dumps(report, sort_keys=True), flush=True)
        if not self._failed():
            self.work_budget.check()
        super().tearDown()

    def client(self, *args, **kwargs):
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

    def test_fixed_generated_l_n_o_keep_finite_loot_recoverable_after_source_free_restore(self):
        fallback = unrelated_fallback(self)
        for species, signature in (("leprechaun", "filch_gold"), ("nymph", "filch_charm"), ("orc", "measured_cut")):
            with self.subTest(species=species):
                self.save = self.directory / (species + ".db")
                package = theft_package(self, species)
                server = self.server("--checkpoint-interval", 1, scenario=package, seed=42)
                player, current = self.client()
                self.assertFalse(current["state"]["wizard_game"])
                receipts = self.private_receipts()
                self.assertEqual(len(receipts), 1)
                self.assertEqual(receipts[0]["scope"], "dungeon/1")
                born = receipts[0]["decisions"][0]["result"]["attempts"][0]
                self.assertEqual((born["kind"], born["content"]), ("placed", "rogue." + species))
                actor = born["entity"]["id"]
                current = self.execute(player, {"type": "wait"})
                initial = checkpoint(self, player)
                recipe = encounter_sources()["actors"]["rogue." + species]["creature"]
                creature = actor_creature(initial, actor)
                self.assertEqual(creature["build"]["species"], "rogue." + species)
                self.assertEqual(len(creature["build"]["hd"]), len(recipe["hit_dice"]))
                self.assertEqual(creature["build"]["choices"], [
                    {"purchases": row["purchases"], "attribute": row.get("attribute")} for row in recipe["hit_dice"]])
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
                    "invocation": {"kind": "actor_target", "target": target["id"]}})
                for _ in range(12):
                    if self.ability_seen(player, start, target["id"], signature) and (
                            species == "orc" or not inventory(current, stolen_name)):
                        break
                    current = self.execute(player, {"type": "wait"})
                else:
                    self.fail(f"Fixed seed42 {species} did not execute its signature/transfer within twelve waits")
                transferred = checkpoint(self, player)
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
                    self.assertTrue(inventory(current, "mace"))
                self.assertEqual(quantity(transferred, actor, identity), expected)
                if species != "orc":
                    self.assert_vanished_at_transfer(player, start, before, species, stolen_name)
                retained = carried(transferred, actor, identity)
                before_restart = self.request(player, {"type": "snapshot"})
                player.stop(); server.stop()
                shutil.rmtree(package)
                server = self.server("--checkpoint-interval", 1, scenario=fallback, seed=None)
                player, current = self.client()
                self.assertEqual(current["state"], before_restart["state"])
                self.assertEqual(current["history"], before_restart["history"])
                self.assertEqual(checkpoint(self, player), transferred)
                self.assertEqual(self.private_receipts(), receipts, "Restore must not regenerate finite stock")
                weapon = inventory(current, "mace")[0]
                current = self.execute(player, {"type": "equip", "item": weapon["id"], "slot": 0})
                for _ in range(12):
                    if self.target(current, species) is None:
                        break
                    current, target = self.approach(player, current, species)
                    current = self.execute(player, {"type": "use_ability", "id": "basic_melee",
                        "invocation": {"kind": "actor_target", "target": target["id"]}})
                else:
                    self.fail("Ordinary equipped recovery did not defeat the thief within twelve attacks")
                killed = checkpoint(self, player)
                self.assertTrue(actor_creature(killed, actor)["dead"])
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
