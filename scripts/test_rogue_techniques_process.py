"""Purchased starter techniques and real injury care in ordinary Rogue play.

The production package, its seed and disclosed cells are the only setup. These
bounded examples establish usable content, not numerical balance or pacing.
"""
from scalar_wire import u64
import unittest

from process_harness import (
    ProcessTestCase, content_ids, disclosed_exploration_route, resource, settle_rogue_action, underfoot,
)
from rogue_magic_receipts import (
    EQUIPMENT_ITEM_STAGE_IDS, MAGIC_ITEM_STAGE_IDS,
    assert_equipment_item_receipts, assert_magic_item_receipts,
)

from rogue_test_support import (
    assert_room_gold_receipt, care_combat_action, disclosed_actor_candidates,
    disclosed_melee_candidate, disclosed_sight_candidate,
    first_floor_foes, disclosed_frames, resolved_events,
)


def observation(frame):
    return frame["state"]["observation"]


def combat(frame):
    return observation(frame)["combat"]


def ability(frame, identity):
    return next(row for row in combat(frame)["own_stats"]["abilities"]
                if row["id"] == identity)


def bandages(frame):
    return sum(int(row["quantity"]) for row in observation(frame)["inventory"]
               if (row.get("known_supply") or {}).get("id") == "bandage")


def visible_foes(frame):
    hostile = {row["actor"] for row in combat(frame)["actors"] if row["hostile"]}
    return disclosed_actor_candidates(row for row in observation(frame)["visible_actors"]
                                      if row["id"] in hostile)


class RogueTechniquesProcesses(ProcessTestCase):
    def assert_first_floor_population(self, character):
        foes = first_floor_foes()
        receipts = self.private_receipts(actor=character)
        self.assertEqual(len(receipts), 1)
        receipt = receipts[0]
        self.assertEqual((receipt["scope"], receipt["entrant"], receipt["cause"]),
                         ("dungeon/1", character, "initial"))
        self.assertEqual((receipt["profile"], receipt["profile_version"]), ("rogue.low_depth", 1))
        expected = (
            ("rogue.eight_species_foes", set(foes), "actor", 4),
            ("rogue.bandages", {"rogue.clean_bandage"}, "item", 1),
            ("rogue.rations", {"rogue.ration"}, "item", 1),
            ("rogue.restore_strength", {"rogue.restore_strength"}, "item", 1),
        )
        self.assertEqual([row["id"] for row in receipt["decisions"]],
                         [row[0] for row in expected]
                         + ["rogue.room_gold", *MAGIC_ITEM_STAGE_IDS, *EQUIPMENT_ITEM_STAGE_IDS])
        identities = assert_room_gold_receipt(self, receipt["decisions"][4], 1)
        identities.extend(assert_magic_item_receipts(self, receipt["decisions"][5:8], require_placed=True))
        identities.extend(assert_equipment_item_receipts(self, receipt["decisions"][8:], require_placed=True))
        for stage, (_, eligible, kind, count) in zip(receipt["decisions"][:4], expected, strict=True):
            self.assertEqual(stage["version"], 1)
            result = stage["result"]
            self.assertEqual(result["kind"], "evaluated")
            self.assertEqual(result["requested"], count)
            self.assertEqual(len(result["attempts"]), count)
            placed = []
            for attempt in result["attempts"]:
                self.assertIn(attempt["content"], eligible)
                self.assertIn(attempt["kind"], ("placed", "skipped"))
                if attempt["kind"] == "skipped":
                    self.assertEqual(attempt["reason"], "no_eligible_cell")
                    continue
                self.assertEqual(attempt["entity"]["kind"], kind)
                if kind == "actor":
                    self.assertIsNone(attempt["entity"]["carried_loot"])
                placed.append(attempt)
                identities.append((kind, attempt["entity"]["id"]))
            self.assertTrue(placed, "The fixture must place real content for each stage")
        self.assertEqual(len(identities), len(set(identities)))
        return set(foes.values())

    def settled(self, player, current):
        return settle_rogue_action(self, player, current)

    def act_settled(self, player, action):
        return self.settled(player, self.act(player, action))

    def buy_first(self, player, selection, identity):
        before = self.request(player, {"type": "snapshot"})
        self.assertNotIn(identity, content_ids(combat(before)["own_stats"]["abilities"]))
        # Ordinary advancement previews require an owner page's current basis;
        # a world snapshot alone does not open that private editing context.
        opened = self.command(player, {"type": "advancement", "command": "overview"})
        self.assertIsNone(opened["error"])
        preview = self.command(player, {"type": "advancement", "command":
                                       "purchase 1 advancement " + selection})
        self.assertIsNone(preview["error"])
        self.assertNotIn(identity, content_ids(combat(preview)["own_stats"]["abilities"]))
        confirmed = self.command(player, {"type": "advancement", "command": "confirm"})
        self.assertIsNone(confirmed["error"])
        if any(row["phase"] == "queued" for row in confirmed["intentions"]):
            confirmed = self.frame(player, lambda frame: not frame.get("state_omitted", False) and not any(
                row["phase"] == "queued" for row in frame["intentions"]))
        self.settled(player, confirmed)
        page = self.command(player, {"type": "advancement", "command": "overview"})
        self.assertIsNone(page["error"])
        self.assertEqual(page["advancement_page"]["entries"][0]["value"]["points"],
                         {"earned": "1", "spent": "1", "unspent": "0"})
        current = self.request(player, {"type": "snapshot"})
        self.assertIn(identity, content_ids(combat(current)["own_stats"]["abilities"]))
        return current

    def explore(self, player, current, at, floor, visited):
        visited.add(at)
        for cell in observation(current)["visible_cells"]:
            position = cell["position"]
            if position["z"] == 0 and not cell["wall"]:
                floor.add((at[0] + position["x"], at[1] + position["y"]))
        targets = {(at[0] + foe["position"]["x"], at[1] + foe["position"]["y"])
                   for foe in visible_foes(current)}
        route = disclosed_exploration_route(at, floor, targets, visited)
        self.assertIsNotNone(route, "No disclosed route to an ordinary Rogue foe")
        target, direction = route[0]
        before = underfoot(current["state"])["key"]
        current = self.act_settled(player, {"type": "move", "direction": direction})
        self.assertNotEqual(underfoot(current["state"])["key"], before)
        return current, target

    def apply_first_technique(self, character, selection, identity):
        self.server(scenario="rogue-exploration", seed=42, character=character)
        # Current observations and transient events are asserted, not map memory.
        player, _ = self.client(actor=character, omit_memory=True, compact_background=True)
        foe_names = self.assert_first_floor_population(character)
        current = self.buy_first(player, selection, identity)
        at, floor, visited = (0, 0), set(), set()
        attempts = 0
        outcomes = []
        for _ in range(2500):
            known = ability(current, identity)
            # Use conservative visible reach. The server still validates the
            # actual body, sight and target at preparation and resolution.
            target = next((foe for foe in visible_foes(current)
                           if (disclosed_melee_candidate(observation(current), foe)
                               if known["reach"]["kind"] == "melee"
                               else disclosed_sight_candidate(foe, known["reach"]["range"]))), None)
            if target is None:
                current, at = self.explore(player, current, at, floor, visited)
                continue
            self.assertIn(target["name"], foe_names)
            cost = known["cost"]
            if cost and resource(current, cost["resource"]["id"])["available"] < cost["start"] + cost["resolution"]:
                current = self.act_settled(player, {"type": "wait"})
                continue
            self.assertLess(attempts, 16, f"{identity} did not apply: {outcomes}")
            attempts += 1
            before, start = current, len(player.transcript)
            current = self.act_settled(player, {"type": "use_ability", "id": identity,
                "invocation": {"kind": "actor_target", "target": target["id"]}})
            events = list(resolved_events(disclosed_frames(player, start), before,
                                          identity, target["id"]))
            self.assertTrue(events, f"No disclosed resolved {identity} against {target['name']}")
            outcomes.extend(event["outcome"] for event in events)
            if any(event["outcome"] == "applied" for event in events):
                self.assertEqual(bandages(current), 3)
                return current
        self.fail(f"{identity} did not apply within the disclosed exploration bound: {outcomes}")

    def test_warrior_first_rattling_blow_applies_to_an_ordinary_visible_foe(self):
        self.apply_first_technique(1, "talent rattling_blow", "rattling_blow")

    def test_mage_first_cold_lance_applies_to_an_ordinary_visible_foe(self):
        current = self.apply_first_technique(2,
            "talent evocation_initiate form spark_lance_cold", "spark_lance_cold")
        self.assertNotIn("spark_lance_fire", content_ids(combat(current)["own_stats"]["abilities"]))

    def test_bind_wounds_heals_combat_injury_spends_one_bandage_and_obeys_cooldown(self):
        self.server(scenario="rogue-exploration", seed=42)
        player, current = self.client(omit_memory=True, compact_background=True)
        self.assert_first_floor_population(1)
        self.assertFalse(current["state"]["wizard_game"])
        at, floor, visited, seen = (0, 0), set(), set(), set()
        contacted = set()
        injured = False
        start = len(player.transcript)
        for _ in range(2500):
            foes = visible_foes(current)
            seen.update(foe["id"] for foe in foes)
            contacted.intersection_update(foe["id"] for foe in foes)
            injured = injured or u64(combat(current)["hp"]) < u64(combat(current)["max_hp"])
            if injured and not foes and u64(combat(current)["hp"]) < u64(combat(current)["max_hp"]):
                break
            # Passive species need a resolved hostile contact first. Then let
            # real retaliation injure us before clearing the treatment area.
            action = care_combat_action(foes, injured=injured, contacted=contacted,
                                        observation=observation(current))
            if action is not None:
                before, contact_start = current, len(player.transcript)
                current = self.act_settled(player, action)
                if action["type"] == "use_ability":
                    target = action["invocation"]["target"]
                    events = list(resolved_events(disclosed_frames(player, contact_start),
                                                  before, "basic_melee", target))
                    self.assertTrue(events, "Provocation requires a disclosed resolved strike")
                    contacted.add(target)
            else:
                if not foes:
                    injured = False
                current, at = self.explore(player, current, at, floor, visited)
        else:
            self.fail("No surviving ordinary combat injury reached a clear treatment opportunity")
        target = observation(current)["self_target"]
        self.assertTrue(any(event["type"] == "ability" and event["caster"] in seen
            and event["ability"]["id"] == "basic_melee"
            and event["target"] == target and event["outcome"] == "applied"
            for frame in disclosed_frames(player, start) for event in combat(frame)["events"]),
            "The injury must come from disclosed ordinary hostile combat")

        before, start = current, len(player.transcript)
        self.assertEqual(bandages(before), 3)
        self.assertEqual(int(ability(before, "bind_wounds")["cooldown_remaining"]), 0)
        care = {"type": "use_ability", "id": "bind_wounds",
                "invocation": {"kind": "actor_target", "target": target},
                "supplies": {"kind": "carried"}}
        current = self.act_settled(player, care)
        frames = list(disclosed_frames(player, start))
        queued = [frame for frame in frames
                  if (frame.get("message") or {}).get("type") == "ack"
                  and frame["message"].get("receipt", {}).get("phase") == "queued"]
        self.assertTrue(queued, "Care must have an observed queued acknowledgement")
        for frame in queued:
            self.assertEqual(bandages(frame), 3, "Admission must not spend the bandage")
            self.assertEqual(u64(combat(frame)["hp"]), u64(combat(before)["hp"]))
            self.assertEqual(int(ability(frame, "bind_wounds")["cooldown_remaining"]), 0)
        # A single publication may include the entire preparation and resolution.
        # Assert held stock only if an actual unpaid preparation was disclosed;
        # supply lifecycle tests separately require that intermediate witness.
        for frame in frames:
            if (combat(frame)["preparation_remaining"] is not None
                    and int(combat(frame)["preparation_remaining"]) > 0
                    and int(ability(frame, "bind_wounds")["cooldown_remaining"]) > 0):
                self.assertEqual(bandages(frame), 3,
                                 "An observed pre-resolution preparation retains the bandage")
        events = list(resolved_events(frames, before, "bind_wounds", target))
        # Bind Wounds has only a Heal effect: applied is authoritative positive
        # treatment, whereas an already healthy target resolves as unaffected.
        self.assertTrue(any(event["outcome"] == "applied" for event in events), events)
        recovery = combat(before)["own_stats"]["health_recovery"]
        elapsed = int(observation(current)["tick"]) - int(observation(before)["tick"])
        self.assertLess(int(recovery["elapsed_ticks"]) + elapsed, int(recovery["period_ticks"]),
                        "This treatment sample must finish before passive Health recovery")
        self.assertEqual(u64(combat(current)["hp"]), min(u64(combat(before)["max_hp"]), u64(combat(before)["hp"]) + 2))
        self.assertEqual(bandages(current), 2)

        remaining = int(ability(current, "bind_wounds")["cooldown_remaining"])
        self.assertGreater(remaining, 0)
        # The first positive-cooldown frame may already contain the heal/spend.
        # Check the full authored interval against all elapsed actor-active time.
        self.assertEqual(remaining, 1200 - elapsed)
        rejected = self.act(player, care)
        self.assertIsNotNone(rejected["error"])
        for field in ("state", "history"):
            self.assertEqual(rejected[field], current[field], "Cooldown refusal must not spend or act")

        # Ordinary Wait advances actor-active cooldown; a snapshot or rejected
        # re-use did not. The bound comes from the currently disclosed remainder.
        for _ in range(1 + (remaining + 99) // 100):
            if remaining == 0:
                break
            before = current
            current = self.act_settled(player, {"type": "wait"})
            elapsed = int(observation(current)["tick"]) - int(observation(before)["tick"])
            self.assertEqual(int(ability(current, "bind_wounds")["cooldown_remaining"]),
                             max(0, remaining - elapsed))
            remaining = int(ability(current, "bind_wounds")["cooldown_remaining"])
            self.assertEqual(bandages(current), 2)
        self.assertEqual(remaining, 0)


if __name__ == "__main__":
    unittest.main()
