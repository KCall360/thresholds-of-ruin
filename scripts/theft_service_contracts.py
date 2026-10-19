"""One bounded production Service composition for theft, concealment and recovery.

The generated room is ordinary source. Wizard operations only position the
controlled encounter; all theft, death, transfer and pickup use paid actions.
Transport, native rendering and source-free recovery belong to the process owner.
"""
import copy
import json
import os
from pathlib import Path
import re
import tomllib
import unittest

from arena_matrix import toml_source
from service_contract_host import ServiceHost

ROOT = Path(__file__).resolve().parent.parent
POTION = "potion of restore strength"


def fixture():
    source = ROOT / "scenarios/rogue-exploration"
    manifest = tomllib.loads((source / "scenario.toml").read_text(encoding="utf-8"))
    species = tomllib.loads((source / "catalog/creatures.toml").read_text(encoding="utf-8"))["creatures"]["species"]
    actors = tomllib.loads((source / "catalog/actors.toml").read_text(encoding="utf-8"))["actors"]
    items = tomllib.loads((source / "catalog/items.toml").read_text(encoding="utf-8"))
    rules = tomllib.loads((ROOT / "crates/simulation/data/default-ruleset.toml").read_text(encoding="utf-8"))
    # Complete owned N/L rows and source damage survive. Both real checks roll
    # against an explicitly vulnerable fixture target, without seed searching.
    names = ("rogue.human", "rogue.nymph", "rogue.leprechaun")
    manifest["creatures"] = {"species": {name: copy.deepcopy(species[name]) for name in names}}
    manifest["creatures"]["species"]["rogue.human"].setdefault("modifiers", []).extend([
        {"scope": "always", "op": {"kind": "set", "stat": {"kind": "defense", "defense": "physical"}, "value": 0}},
        {"scope": "always", "op": {"kind": "set", "stat": {"kind": "defense", "defense": "cognitive"}, "value": 0}},
        {"scope": "always", "op": {"kind": "add", "stat": {"kind": "health"}, "value": 200}},
        {"scope": "always", "op": {"kind": "grant_ability", "ability": "fixture_finish"}},
    ])
    rules["abilities"].append({"id": "fixture_finish", "name": "Fixture Finish", "tags": [],
        "basic_attack": False, "invocation": "actor_target", "interaction": "hostile_to_target",
        "timing": {"source": {"kind": "fixed", "prepare": 1, "recover": 1}},
        "reach": {"kind": "sight", "range": 16},
        "strikes": {"kind": "authored", "strikes": [{"offset": 0,
            "on_success": [{"kind": "damage", "damage": {"kind": "components", "components": [
                {"category": "impact", "amount": {"kind": "fixed", "amount": 1000}}]}}],
            "on_failure": [], "descriptors": [], "riders": []}]},
        "ai": {"priority": 0, "hostile": True, "minimum_resource_percent": 0, "range_preference": "any"}})
    # Service create accepts a resolved Manifest, not module/preset authoring.
    manifest.pop("modules")
    manifest.update(id="theft-service", objective=None, unique_retrieval=None,
        archetypes={**{name: copy.deepcopy(actors[name]) for name in names[1:]},
            **{name: copy.deepcopy(items["items"][name]) for name in ("rogue.gold", "rogue.restore_strength")}},
        appearance_pools=items["appearance_pools"])
    manifest["characters"] = [copy.deepcopy(manifest["characters"][0])]
    manifest["characters"][0].update(known_identities=[], loadout=[
        {"archetype": "rogue.restore_strength", "quantity": 2},
        {"archetype": "rogue.gold", "quantity": 100}])
    manifest["characters"][0]["creature"]["nutrition"] = {"kind": "exempt"}
    manifest["characters"][0]["creature"]["future_growth"] = {"kind": "disabled"}
    manifest["generation_recipes"] = {"room": {"version": 1, "stages": [
        {"id": "stone", "version": 1, "operation": {"kind": "stone_fill"}},
        {"id": "slots", "version": 1, "operation": {"kind": "grid_partition"}},
        {"id": "rooms", "version": 1, "operation": {"kind": "rooms", "width": [7, 7], "height": [5, 5]}},
        {"id": "connections", "version": 1, "operation": {"kind": "connected_graph", "extra": [0, 0]}},
        {"id": "corridors", "version": 1, "operation": {"kind": "corridors"}},
        {"id": "stairs", "version": 1, "operation": {"kind": "stairs"}},
    ]}}
    manifest["procedural_worlds"] = {"dungeon": {"kind": "grid_chain", "version": 1,
        "base_region": 1, "grid": [1, 1], "size": [9, 7, 2], "recipe": "room",
        "domain": {"kind": "open", "max_depth": 2}}}
    manifest["floor_activation"] = {"version": 1, "protagonist": "selected_character",
        "families": {"dungeon": "empty"}, "profiles": {"empty": {"version": 1, "stages": []}}}
    return {"manifest": manifest, "regions": [], "rules": toml_source(rules), "seed": 42,
        "wizard": True, "frame_bytes": 65536, "client_bytes": 262144, "total_bytes": 524288}


def observation(snapshot):
    return snapshot["state"]["observation"]


def inventory_units(snapshot, item_class):
    return sum(int(item["quantity"]) for item in observation(snapshot)["inventory"] if item["class"] == item_class)


class TheftServiceContracts(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        executable = os.environ.get("TOR_SERVICE_CONTRACT_HOST")
        digest = os.environ.get("TOR_SERVICE_CONTRACT_SHA256")
        if not executable or not digest:
            raise RuntimeError("A qualified Service host and SHA-256 are required; no auto-build")
        cls.host = ServiceHost(executable, expected_sha256=digest)
        cls.addClassCleanup(cls.host.close)

    def setUp(self):
        self.case = self.host.call("create", **fixture())["case"]
        self.addCleanup(lambda: self.host.call("close", case=self.case))
        self.serial = 0
        self.peers = {}
        self.frames = []
        self.subject_frames = []
        self.connect(1)
        self.wizard = self.host.call("connect", case=self.case, role="wizard", user="setup", actors=[1])["connection"]
        self.drain(self.wizard)
        self.request(self.wizard, {"type": "attach", "actor": "1"})
        self.setup_command("teleport 1 1 4 3 0")

    def drain(self, peer):
        result = self.host.call("drain", case=self.case, connection=peer)
        self.assertTrue(result["live"])
        self.assertIsNone(result["closing"])
        rows = [json.loads(frame) for frame in result["frames"]]
        self.assertFalse([row for row in rows if row["type"] == "error"], rows)
        self.frames.extend(rows)
        if peer == self.peers.get(1):
            self.subject_frames.extend(rows)
        return rows

    def request(self, peer, request, *, response=None):
        self.serial += 1
        identity = f"theft-{self.serial}"
        self.host.request(case=self.case, connection=peer, request_id=identity, request=request)
        rows = self.drain(peer)
        if response is not None:
            # Publications queued by another peer's command remain in the full
            # disclosure transcript, but are not this request's response.
            return [row for row in rows if row["type"] == response and row.get("request_id") == identity]
        return rows

    def connect(self, actor):
        peer = self.host.call("connect", case=self.case, role="player", user=f"owner-{actor}", actors=[actor])["connection"]
        self.peers[actor] = peer
        self.drain(peer)
        self.request(peer, {"type": "attach", "actor": str(actor)})
        self.request(peer, {"type": "acquire_control"})
        return peer

    def snapshot(self, actor):
        rows = self.request(self.peers[actor], {"type": "snapshot"}, response="snapshot")
        values = [row["snapshot"] for row in rows if row["type"] == "snapshot"]
        self.assertEqual(len(values), 1)
        return values[0]

    @staticmethod
    def envelope(current, command):
        return {"type": "command", "context": {"stream": current["context"],
            "readiness_revision": current["readiness"]["revision"]}, "branch": current["branch"],
            "command": {"expected_revision": current["state"]["revision"], **command}}

    def setup_command(self, command):
        rows = self.request(self.wizard, {"type": "snapshot"}, response="snapshot")
        self.assertEqual(len(rows), 1)
        current = rows[0]["snapshot"]
        self.request(self.wizard, self.envelope(current, {"type": "wizard", "operation": command}))
        return self.host.call("audit", case=self.case, actor=1, user="setup")

    def spawn(self, name):
        # One exact fixture position; no search, retries, or dynamic RNG tuning.
        result = self.setup_command(f"creature spawn archetype:rogue.{name} 1 5 3 0")
        # Audit returns {state, history: HistoryPage}; entries are oldest first.
        entries = result["history"]["entries"]
        self.assertTrue(entries)
        self.assertEqual(entries[-1]["content"]["type"], "wizard")
        spawned = re.match(rf"Spawned actor (\d+) from archetype:rogue\.{re.escape(name)} ",
            entries[-1]["content"]["summary"])
        self.assertIsNotNone(spawned, "the newest wizard history must acknowledge this creature spawn")
        actor = int(spawned.group(1))
        self.connect(actor)
        return actor

    def act(self, actor, action):
        current = self.snapshot(actor)
        start = len(self.frames)
        rows = self.request(self.peers[actor], self.envelope(current, {"type": "act", "action": action}), response="ack")
        receipts = [row["receipt"] for row in rows if row["type"] == "ack"]
        self.assertEqual(len(receipts), 1)
        self.assertEqual(receipts[0]["type"], "admitted")
        intention = receipts[0]["intention"]
        for _ in range(100):
            for other in self.peers:
                if other == actor:
                    continue
                ready = self.snapshot(other)
                if ready["readiness"]["admission"] and observation(ready)["ready"]:
                    self.request(self.peers[other], self.envelope(ready, {"type": "act", "action": {"type": "wait"}}))
            result = self.host.call("step", case=self.case)
            for peer in [*self.peers.values(), self.wizard]:
                self.drain(peer)
            statuses = [row["update"]["body"]["status"] for row in self.frames[start:]
                if row["type"] == "update" and row["update"]["body"]["type"] == "intention"
                and row["update"]["body"]["status"]["intention"] == intention
                and row["update"]["body"]["status"]["actor"] == str(actor)
                and row["update"]["body"]["status"]["branch"] == current["branch"]]
            self.assertFalse([row for row in statuses if row["phase"] in ("failed", "cancelled", "paused")], statuses)
            if any(row["phase"] == "resolved" for row in statuses):
                return self.snapshot(actor)
            self.assertEqual(result, {"status": "progress"}, "paid action must reach a terminal resolution")
        self.fail("the controlled theft action exceeded its explicit scheduler budget")

    def use(self, actor, ability, name):
        target = next(row["id"] for row in observation(self.snapshot(actor))["visible_actors"] if row["name"] == name)
        return self.act(actor, {"type": "use_ability", "id": ability,
            "invocation": {"aim": {"kind": "actor_target", "target": target}, "weapon": {"kind": "default"}}, "supplies": {"kind": "carried"}})

    def recover(self, item_class):
        for _ in range(16):
            current = self.snapshot(1)
            ground = next(row for row in observation(current)["ground_items"] if row["item"]["class"] == item_class)
            if ground["reachable"]:
                return self.act(1, {"type": "take", "item": ground["item"]["id"], "quantity": None})
            origin, target = observation(current)["position"], ground["position"]
            dx, dy = target["x"] - origin["x"], target["y"] - origin["y"]
            direction = ("east" if dx > 0 else "west") if dx else ("south" if dy > 0 else "north")
            self.act(1, {"type": "move", "direction": direction})
        self.fail("ordinary recovery did not reach the visible death drop")

    def test_zero_damage_nymph_preserves_concealment_relocates_and_drops_while_leprechaun_conserves_gold(self):
        initial = self.snapshot(1)
        self.assertEqual(inventory_units(initial, "potion"), 2)
        hp = observation(initial)["combat"]["hp"]
        nymph = self.spawn("nymph")
        before = next(row["position"] for row in observation(self.snapshot(1))["visible_actors"] if row["name"] == "nymph")
        stolen = self.use(nymph, "filch_charm", "Warrior")
        after = self.snapshot(1)
        self.assertEqual(observation(after)["combat"]["hp"], hp, "N's checked contact deals zero damage")
        transfer = {"victim_potions": inventory_units(after, "potion"),
            "thief_potions": inventory_units(stolen, "potion"),
            "events": observation(stolen)["combat"]["events"]}
        self.assertEqual(transfer["victim_potions"], 1, transfer)
        self.assertEqual(transfer["thief_potions"], 1, transfer)
        potion = next(item for item in observation(stolen)["inventory"] if item["class"] == "potion")
        self.assertFalse(potion["identified"])
        self.assertNotEqual(potion["name"], POTION)
        position = next(row["position"] for row in observation(after)["visible_actors"] if row["name"] == "nymph")
        self.assertNotEqual(position, before, "TR3 moves only after the real transferred unit")
        self.use(1, "fixture_finish", "nymph")
        recovered = self.recover("potion")
        self.assertEqual(inventory_units(recovered, "potion"), 2)
        self.assertTrue(all(not item["identified"] for item in observation(recovered)["inventory"] if item["class"] == "potion"))
        self.assertNotIn(POTION, json.dumps(self.subject_frames), "the whole ordinary stream retains concealed identity")
        self.setup_command("teleport 1 1 4 3 0")
        leprechaun = self.spawn("leprechaun")
        stolen = self.use(leprechaun, "filch_gold", "Warrior")
        amount = inventory_units(stolen, "coin")
        self.assertGreaterEqual(amount, 2)
        self.assertLessEqual(amount, 61)
        self.assertEqual(inventory_units(self.snapshot(1), "coin") + amount, 100)
        self.use(1, "fixture_finish", "leprechaun")
        self.assertEqual(inventory_units(self.recover("coin"), "coin"), 100)
