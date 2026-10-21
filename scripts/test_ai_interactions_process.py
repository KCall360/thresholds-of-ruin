"""Knowledge-limited item decisions through actual autonomous server execution."""
from scalar_wire import u64
import shutil
import subprocess
import tomllib
import unittest
from pathlib import Path
from tempfile import TemporaryDirectory
from unittest.mock import Mock, patch

from arena_matrix import toml_source
from process_harness import ProcessTestCase, ROOT, WIZARD_TOKEN
from process_harness import creature_package, headless_fight_completion
from rogue_roster_package import roster_package


class AiInteractionProcesses(ProcessTestCase):
    def idle_item_package(self, name):
        package = self.directory / name
        shutil.copytree(ROOT / "scenarios/tests/ai-interactions", package)
        path = package / "regions/1.toml"
        region = tomllib.loads(path.read_text(encoding="utf-8"))
        # Repair the idle precondition without changing hostility, memory, or
        # item policy. The mean guard has never seen the hostile player. Both
        # sides remain in one active region, and NPC/item cells stay unchanged.
        region["size"] = [9, 3, 2]
        region["anchors"]["start"] = [7, 1, 0]
        region["walls"] = [[5, y, z] for y in range(3) for z in range(2)]
        path.write_text(toml_source(region), encoding="utf-8", newline="\n")
        return package

    def validate_item_package(self, package):
        validated = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                   capture_output=True, text=True, encoding="utf-8", timeout=15)
        self.assertEqual(validated.returncode, 0, validated.stderr)

    def idle_item_observation(self, frame):
        view = frame["state"]["observation"]
        others = [row for row in view["visible_actors"] if row["id"] != view["self_target"]]
        self.assertEqual(others, [],
                         "idle item fixture must keep the hostile player completely unseen")
        return view

    def completed_idle_wait(self, player):
        """One ordinary Wait, through the player's recovery and permission update."""
        def completed(frame):
            self.assertIsNone(frame.get("error"))
            self.assertNotEqual((frame.get("message") or {}).get("type"), "error")
            status = ((frame.get("message") or {}).get("update") or {}).get("body", {})
            phase = (status.get("status") or {}).get("phase")
            self.assertNotIn(phase, ("failed", "cancelled", "paused", "suspended"))
            if frame.get("state_omitted", False):
                return False
            self.assertFalse(frame["state"]["observation"]["combat"]["dead"])
            return headless_fight_completion(frame)

        current = self.act(player, {"type": "wait"})
        if not completed(current):
            current = self.frame(player, completed)
        return current

    def test_authored_hobgoblin_acquires_secondary_bow_and_bounded_reserve(self):
        package = roster_package(self, "hobgoblin", autonomous=True)
        path = package / "regions/1.toml"
        region = tomllib.loads(path.read_text(encoding="utf-8"))
        # Only encounter geometry and disclosed ground loot change. The actual
        # production recipe, expertise, abilities and priorities stay intact.
        region["size"] = [10, 5, 2]
        region["anchors"]["start"] = [8, 2, 0]
        region["walls"] = [[5, y, z] for y in range(5) for z in range(2)]
        region["items"] = [
            {"id": 1001, "at": [2, 2, 0], "archetype": "rogue.shortbow"},
            {"id": 1002, "at": [2, 2, 0], "archetype": "rogue.arrow", "quantity": 20},
        ]
        path.write_text(toml_source(region), encoding="utf-8", newline="\n")
        self.validate_item_package(package)
        server = self.server("--checkpoint-interval", 1, scenario=package, wizard=True, seed=42)
        observer, initial = self.client(WIZARD_TOKEN, observe=True, actor=3)
        self.idle_item_observation(initial)
        player, _ = self.client()

        def stock(view):
            bows = [item for item in view["inventory"] if item["name"] == "shortbow"]
            arrows = sum(u64(item["quantity"]) for item in view["inventory"]
                         if item["name"] == "arrow")
            slots = {item["item"]: item["equipped_slot"]
                     for item in view["interactions"]["inventory"]}
            return len(bows), slots.get(bows[0]["id"]) if bows else None, arrows

        for _ in range(24):
            self.completed_idle_wait(player)
            view = self.idle_item_observation(self.request(observer, {"type": "snapshot"}))
            if stock(view) == (1, 10, 6):
                break
        # Six is the current bounded-reserve tuning, not a universal ammunition
        # rule. Secondary placement preserves the authored natural melee attack.
        self.assertEqual(stock(view), (1, 10, 6))
        for _ in range(4):
            self.completed_idle_wait(player)
        view = self.idle_item_observation(self.request(observer, {"type": "snapshot"}))
        self.assertEqual(stock(view), (1, 10, 6), "no collection or equip oscillation")
        self.assertEqual([row["item"]["quantity"] for row in view["ground_items"]
                          if row["item"]["name"] == "arrow"], ["14"])
        self.assertIsNone(self.request(player, {"type": "save"})["error"])
        observer.stop()
        player.stop()
        server.stop()
        self.server(scenario=package, wizard=True, seed=None)
        restored, reopened = self.client(WIZARD_TOKEN, observe=True, actor=3)
        self.assertEqual(stock(self.idle_item_observation(reopened)), (1, 10, 6))
        self.assertEqual(stock(self.idle_item_observation(
            self.request(restored, {"type": "snapshot"}))), (1, 10, 6))

    def test_creature_ai_chooses_a_weapon_using_a_purchased_rank(self):
        package = creature_package(self, "weapon-training", abilities=("fixture_stamina_strike",))
        manifest = package / "scenario.toml"
        source = manifest.read_text(encoding="utf-8")
        weapons = []
        for name, skill, bonus in [("light_blade", "light_weaponry", 1),
                                   ("heavy_blade", "heavy_weaponry", 0)]:
            weapons.append(name + ' = { class = "weapon", name = "' + name + '", '
                           'equipment = { slot = "weapon", attack = { skill = "' + skill + '", '
                           'bonus = ' + str(bonus) + ', wind_up = 60, recovery = 40, damage = { '
                           'primary = { category = "impact", sides = 6 }, components = [{ '
                           'category = "impact", amount = { type = "rolled", count = 2, '
                           'sides = 6, bonus = 0 } }] } } } }')
        source = source.replace('archetypes = { ', 'archetypes = { ' + ', '.join(weapons) + ', ', 1)
        manifest.write_text(source, encoding="utf-8", newline="\n")
        region = package / "regions/1.toml"
        source = region.read_text(encoding="utf-8").replace(
            'hit_dice = [{ source = "racial" }]',
            'hit_dice = [{ source = "warrior", purchases = [{ pool = "advancement", selection = { kind = "skill_rank", skill = "heavy_weaponry" } }] }]')
        source = ('items = [{ id = 100, at = [2,1,0], archetype = "light_blade", carried_by = 2 }, '
                  '{ id = 101, at = [2,1,0], archetype = "heavy_blade", carried_by = 2 }]\n' + source)
        region.write_text(source, encoding="utf-8", newline="\n")
        validated = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                   capture_output=True, text=True, encoding="utf-8", timeout=15)
        self.assertEqual(validated.returncode, 0, validated.stderr)
        self.server(scenario=package, wizard=True)
        player, _ = self.client()
        observer, _ = self.client(WIZARD_TOKEN, observe=True, actor=2)
        chosen = None
        for _ in range(12):
            self.completed_idle_wait(player)
            view = self.request(observer, {"type": "snapshot"})["state"]["observation"]
            names = {item["id"]: item["name"] for item in view["inventory"]}
            equipped = [item for item in view["interactions"]["inventory"]
                        if item["equipped_slot"] is not None]
            if equipped:
                self.assertEqual(len(equipped), 1)
                chosen = equipped[0]
                self.assertEqual(names[chosen["item"]], "heavy_blade")
                break
        self.assertIsNotNone(chosen, "AI must equip the actual upgrade")
        self.assertIsNone(chosen["known_equipment"], "base-gear utility estimates must not disclose unobserved instance numbers")

    def test_ai_takes_one_ground_healing_unit_and_keeps_the_remaining_stack(self):
        package = self.idle_item_package("ground-healing")
        path = package / "regions/1.toml"
        region = tomllib.loads(path.read_text(encoding="utf-8"))
        healing = next(item for item in region["items"] if item["id"] == 20)
        del healing["carried_by"]
        healing["quantity"] = 3
        path.write_text(toml_source(region), encoding="utf-8", newline="\n")
        self.validate_item_package(package)
        self.server(scenario=package, wizard=True)
        observer, initial = self.client(WIZARD_TOKEN, observe=True, actor=2)
        self.idle_item_observation(initial)
        player, _ = self.client()
        for _ in range(12):
            self.completed_idle_wait(player)
            view = self.idle_item_observation(self.request(observer, {"type": "snapshot"}))
            carried = [item for item in view["inventory"] if item["class"] == "potion"]
            if carried:
                break
        self.assertEqual([item["quantity"] for item in carried], ["1"])
        self.assertEqual([item["item"]["quantity"] for item in view["ground_items"] if item["item"]["class"] == "potion"], ["2"])

    def test_ai_walks_to_safe_known_ground_armor_then_replaces_old_gear(self):
        package = self.idle_item_package("ground-armor")
        path = package / "regions/1.toml"
        region = tomllib.loads(path.read_text(encoding="utf-8"))
        mail = next(item for item in region["items"] if item["id"] == 11)
        del mail["carried_by"]
        mail["at"] = [3, 1, 0]
        next(item for item in region["items"] if item["id"] == 14)["equipped_slot"] = 2
        path.write_text(toml_source(region), encoding="utf-8", newline="\n")
        self.validate_item_package(package)
        self.server(scenario=package, wizard=True)
        observer, initial = self.client(WIZARD_TOKEN, observe=True, actor=2)
        self.idle_item_observation(initial)
        player, _ = self.client()
        equipped = None
        for _ in range(18):
            self.completed_idle_wait(player)
            view = self.idle_item_observation(self.request(observer, {"type": "snapshot"}))
            mail = next((item for item in view["inventory"] if item["name"] == "mail"), None)
            if mail:
                equipped = next(item["equipped_slot"] for item in view["interactions"]["inventory"] if item["item"] == mail["id"])
                if equipped == 0:
                    break
        self.assertEqual(equipped, 0, "AI must travel, pick up known armor and complete replacement")

    def test_ai_replaces_armor_fills_duplicate_ring_and_restores_equipment(self):
        package = self.idle_item_package("idle-equipment")
        self.validate_item_package(package)
        server = self.server(scenario=package, wizard=True)
        observer, initial = self.client(WIZARD_TOKEN, observe=True, actor=2)
        initial_view = self.idle_item_observation(initial)
        player, _ = self.client()
        items = {item["name"]: item["id"] for item in initial_view["inventory"]}

        def equipment(view):
            return {item["item"]: item["equipped_slot"] for item in view["interactions"]["inventory"]}

        expected = {items["old mail"]: None, items["mail"]: 0,
                    items["old ring"]: 1, items["ring"]: 2, items["silver ring"]: None}
        finished = None
        for _ in range(14):
            self.completed_idle_wait(player)
            view = self.idle_item_observation(self.request(observer, {"type": "snapshot"}))
            actual = equipment(view)
            if all(actual[item] == slot for item, slot in expected.items()):
                finished = view
                break
        self.assertIsNotNone(finished, "AI must complete armor replacement and fill its second ring socket")
        hidden = next(item for item in finished["interactions"]["inventory"] if item["item"] == items["silver ring"])
        self.assertIsNone(hidden["known_equipment"])
        self.assertIsNone(self.request(player, {"type": "save"})["error"])
        observer.stop()
        player.stop()
        server.stop()
        self.server(scenario=package, wizard=True, seed=None)
        restored, reopened = self.client(WIZARD_TOKEN, observe=True, actor=2)
        self.idle_item_observation(reopened)
        view = self.idle_item_observation(self.request(restored, {"type": "snapshot"}))
        self.assertEqual({item: equipment(view)[item] for item in expected}, expected)

    def test_human_knowledge_does_not_make_an_unknown_ai_potion_usable(self):
        package = self.save.parent / "unknown-ai"
        shutil.copytree(ROOT / "scenarios/tests/ai-interactions", package)
        region = package / "regions/1.toml"
        region.write_text(region.read_text().replace('known_identities = ["healing"]', 'known_identities = []'))
        manifest = package / "scenario.toml"
        manifest.write_text(manifest.read_text().replace('characters = [{ id = 1,', 'characters = [{ id = 1, known_identities = ["healing"],'))
        subprocess.run([self.bin / "tor-scenario", "validate", package], check=True, capture_output=True, text=True)
        self.server(scenario=package, wizard=True)
        observer, _ = self.client(WIZARD_TOKEN, observe=True, actor=2)
        player, initial = self.client()
        target = next(actor["id"] for actor in initial["state"]["observation"]["visible_actors"] if actor["name"] == "ruin guard")
        self.act(player, {"type": "use_ability", "id": "basic_melee", "invocation": {"aim": {"kind": "actor_target", "target": target}, "weapon": {"kind": "default"}}})
        for _ in range(4):
            self.act(player, {"type": "wait"})
        state = self.request(observer, {"type": "snapshot"})["state"]["observation"]
        self.assertEqual(u64(state["combat"]["hp"]), 15)
        potion = next(item for item in state["inventory"] if item["class"] == "potion")
        self.assertEqual(potion["quantity"], "2")
        self.assertEqual(potion["name"], "red potion")
        self.assertFalse(potion["identified"])

    def test_wounded_ai_uses_its_known_healing_and_restores_consumed_stack(self):
        server = self.server(scenario="ai-interactions", wizard=True)
        observer, _ = self.client(WIZARD_TOKEN, observe=True, actor=2)
        player, initial = self.client()
        target = next(actor["id"] for actor in initial["state"]["observation"]["visible_actors"] if actor["name"] == "ruin guard")
        self.act(player, {"type": "use_ability", "id": "basic_melee", "invocation": {"aim": {"kind": "actor_target", "target": target}, "weapon": {"kind": "default"}}})
        healed = None
        for _ in range(5):
            state = self.request(observer, {"type": "snapshot"})
            potions = [item for item in state["state"]["observation"]["inventory"] if item["class"] == "potion"]
            if potions and potions[0]["quantity"] == "1":
                healed = state
                break
            self.act(player, {"type": "wait"})
        self.assertIsNotNone(healed, "autonomous AI must complete known healing")
        self.assertEqual(u64(healed["state"]["observation"]["combat"]["hp"]), 25)
        self.assertIsNone(self.request(player, {"type": "save"})["error"])
        observer.stop()
        player.stop()
        server.stop()
        self.server(scenario="ai-interactions", wizard=True, seed=None)
        restored, _ = self.client(WIZARD_TOKEN, observe=True, actor=2)
        state = self.request(restored, {"type": "snapshot"})
        self.assertEqual(u64(state["state"]["observation"]["combat"]["hp"]), 25)
        self.assertEqual([item["quantity"] for item in state["state"]["observation"]["inventory"] if item["class"] == "potion"], ["1"])


class IdleItemWaitContracts(unittest.TestCase):
    """Exercise the actual stock driver against ordered player publications."""

    @staticmethod
    def player_frame(*, ready=True, admission=True):
        return {"state": {"observation": {"ready": ready,
                    "combat": {"dead": False, "preparation_active": False}}},
                "readiness": {"admission": admission, "resume": None, "cancel": None},
                "intentions": [], "error": None}

    OWNERS = {
        "hobgoblin": ("test_authored_hobgoblin_acquires_secondary_bow_and_bounded_reserve", 24),
        "weapon": ("test_creature_ai_chooses_a_weapon_using_a_purchased_rank", 12),
        "healing": ("test_ai_takes_one_ground_healing_unit_and_keeps_the_remaining_stack", 12),
        "armor": ("test_ai_walks_to_safe_known_ground_armor_then_replaces_old_gear", 18),
        "equipment": ("test_ai_replaces_armor_fills_duplicate_ring_and_restores_equipment", 14),
    }

    def run_stock_driver(self, *, owner="hobgoblin", delayed=None, progress=True, rejected=False):
        method, bound = self.OWNERS[owner]
        case = AiInteractionProcesses(method)
        case.bin, case.suffix = Path("unused-scripted-binaries"), ""
        observer, player, restored, server = (Mock() for _ in range(4))
        completed = 0
        submitted = 0
        pending = []

        def item_frame():
            acquired = rejected or (progress and completed >= 2)
            view = {"self_target": "3", "visible_actors": [], "inventory": [],
                    "interactions": {"inventory": []}, "ground_items": []}
            if owner == "hobgoblin":
                if acquired:
                    view["inventory"] = [
                        {"id": "11", "name": "shortbow", "quantity": "1"},
                        {"id": "12", "name": "arrow", "quantity": "6"}]
                    view["interactions"]["inventory"] = [{"item": "11", "equipped_slot": 10}]
                view["ground_items"] = [{"item": {"name": "arrow", "quantity": "14"}}]
            elif owner == "weapon":
                view["inventory"] = [{"id": "11", "name": "heavy_blade"}]
                view["interactions"]["inventory"] = [{"item": "11",
                    "equipped_slot": 0 if acquired else None, "known_equipment": None}]
            elif owner == "healing":
                if acquired:
                    view["inventory"] = [{"class": "potion", "quantity": "1"}]
                view["ground_items"] = [{"item": {"class": "potion", "quantity": "2"}}]
            elif owner == "armor":
                view["inventory"] = [{"id": "11", "name": "mail"}]
                view["interactions"]["inventory"] = [{"item": "11",
                    "equipped_slot": 0 if acquired else None}]
            else:
                names = ("old mail", "mail", "old ring", "ring", "silver ring")
                slots = (None, 0, 1, 2, None) if acquired else (0, None, 1, None, None)
                view["inventory"] = [{"id": str(i), "name": name} for i, name in enumerate(names)]
                view["interactions"]["inventory"] = [{"item": str(i),
                    "equipped_slot": slot, "known_equipment": None} for i, slot in enumerate(slots)]
            return {"state": {"observation": view}}

        def act(client, action):
            nonlocal submitted, completed
            self.assertIs(client, player)
            self.assertEqual(action, {"type": "wait"})
            submitted += 1
            if rejected or pending:
                return {"error": "StaleRevision: Refresh the observation before acting"}
            if delayed is None:
                completed += 1
                return self.player_frame()
            pending.extend([self.player_frame(**delayed), self.player_frame()])
            return self.player_frame(**delayed)

        def frame(client, predicate, *args, **kwargs):
            nonlocal completed
            self.assertIs(client, player, "only the player's stream establishes completion")
            while pending:
                value = pending.pop(0)
                last = not pending
                if last:
                    completed += 1
                if predicate(value):
                    self.assertTrue(last, "an intermediate admission/readiness is not completion")
                    return value
            self.fail("driver waited past the ordered completion publication")

        def request(client, value):
            if client is player:
                self.assertEqual(value, {"type": "save"})
                if not rejected:
                    self.assertEqual(completed, 6 if owner == "hobgoblin" else 2)
                return {"error": None}
            self.assertIn(client, (observer, restored))
            self.assertEqual(value, {"type": "snapshot"})
            return item_frame()

        case.act = Mock(side_effect=act)
        case.frame = Mock(side_effect=frame)
        case.request = Mock(side_effect=request)
        case.server = Mock(return_value=server)
        observers = iter((observer, restored))
        def client(*args, observe=False, **kwargs):
            current = next(observers) if observe else player
            return current, item_frame() if observe else self.player_frame()
        case.client = Mock(side_effect=client)
        case.validate_item_package = Mock()
        with TemporaryDirectory() as directory:
            package = Path(directory)
            (package / "regions").mkdir()
            (package / "scenario.toml").write_text('archetypes = { }\n', encoding="utf-8")
            (package / "regions/1.toml").write_text(
                'size = [10,5,2]\nitems = [{id=11, carried_by=2}, {id=14}, '
                '{id=20, carried_by=2}]\n[anchors]\nstart = [8,2,0]\n', encoding="utf-8")
            case.idle_item_package = Mock(return_value=package)
            with patch(__name__ + ".roster_package", return_value=package), \
                 patch(__name__ + ".creature_package", return_value=package), \
                 patch(__name__ + ".subprocess.run", return_value=Mock(returncode=0, stderr="")):
                if rejected:
                    with self.assertRaisesRegex(AssertionError, "StaleRevision"):
                        getattr(case, method)()
                elif progress:
                    getattr(case, method)()
                else:
                    with self.assertRaises(AssertionError):
                        getattr(case, method)()
        if rejected:
            self.assertEqual((submitted, completed), (1, 0))
            case.request.assert_not_called()
            case.frame.assert_not_called()
            return
        expected = (6 if owner == "hobgoblin" else 2) if progress else bound
        self.assertEqual((submitted, completed), (expected, expected),
                         "no dropped, replaced or retried Waits; original owner bound stays strict")
        self.assertEqual(pending, [])
        self.assertEqual(case.frame.call_count, 0 if delayed is None else expected)
        if owner != "weapon":
            self.assertEqual(case.validate_item_package.call_count, 1)
        if progress and owner in ("hobgoblin", "equipment"):
            server.stop.assert_called_once()

    def test_adjacent_idle_callers_complete_each_wait_before_observing_items(self):
        for owner in ("weapon", "healing", "armor", "equipment"):
            for delayed in (None, {"ready": False, "admission": True},
                            {"ready": True, "admission": False}):
                with self.subTest(owner=owner, delayed=delayed):
                    self.run_stock_driver(owner=owner, delayed=delayed)

    def test_idle_callers_fail_at_their_original_bounds_without_progress(self):
        for owner in self.OWNERS:
            with self.subTest(owner=owner):
                self.run_stock_driver(owner=owner, progress=False,
                                      delayed={"ready": False, "admission": True})

    def test_idle_callers_do_not_hide_rejected_wait_behind_successful_item_state(self):
        for owner in self.OWNERS:
            with self.subTest(owner=owner):
                self.run_stock_driver(owner=owner, rejected=True)

    def test_wait_errors_and_terminal_work_fail_without_retry(self):
        case = AiInteractionProcesses(
            "test_authored_hobgoblin_acquires_secondary_bow_and_bounded_reserve")
        dead = self.player_frame()
        dead["state"]["observation"]["combat"]["dead"] = True
        failed = self.player_frame()
        failed["message"] = {"update": {"body": {
            "type": "intention", "status": {"phase": "failed"}}}}
        wire_error = self.player_frame()
        wire_error["message"] = {"type": "error", "code": "stale_revision"}
        for returned in ({"error": "StaleRevision"}, dead, failed, wire_error):
            with self.subTest(returned=returned):
                case.act = Mock(return_value=returned)
                case.frame = Mock()
                with self.assertRaises(AssertionError):
                    case.completed_idle_wait("player")
                case.act.assert_called_once_with("player", {"type": "wait"})
                case.frame.assert_not_called()

    def test_already_completed_waits_keep_the_original_stock_and_reopen_checks(self):
        self.run_stock_driver()

    def test_admission_before_recovery_does_not_send_the_next_wait(self):
        self.run_stock_driver(delayed={"ready": False, "admission": True})

    def test_ready_before_admission_does_not_send_the_next_wait(self):
        self.run_stock_driver(delayed={"ready": True, "admission": False})


if __name__ == "__main__":
    unittest.main()
