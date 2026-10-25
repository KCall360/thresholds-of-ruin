"""Ordinary Text/native magic-item choices, exact charges and source-free work.

Backend contracts own outcome permutations. These witnesses own actual input,
paid-choice UI recovery, spectator presentation and one retained device plan.
"""
import json
import shutil
import subprocess
import unittest
import tomllib

from arena_matrix import toml_source

from process_harness import ProcessTestCase, ROOT, SPECTATOR_TOKEN, WIZARD_TOKEN, non_graphical, read_capture
from status_condition_package import checkpoint, actor_creature
from self_device_package import prepare as prepare_self_device
from nothing_catalog_package import prepare as prepare_imported_nothing
from light_catalog_package import prepare as prepare_imported_light
from drain_catalog_package import prepare as prepare_imported_drain
from first_contact_catalog_package import prepare as prepare_imported_first_contact
from torch_catalog_package import prepare as prepare_imported_torch


def observation(frame):
    return frame["state"]["observation"]


def interactions(frame):
    return observation(frame)["interactions"]


def items(frame, category):
    return [row for row in observation(frame)["inventory"] if row["class"] == category]


def charges(frame, item):
    return next(row["charges"] for row in interactions(frame)["inventory"] if row["item"] == item)


def settled(frame):
    view = observation(frame)
    return (view["ready"] and not frame["intentions"]
            and view["combat"]["preparation_remaining"] is None
            and int(view["combat"]["recovery_remaining"]) == 0)


class MagicItemProcesses(ProcessTestCase):
    graphical = True

    def snapshot(self, client):
        frame = self.request(client, {"type": "snapshot"})
        self.assertIsNone(frame["error"])
        return frame

    def package(self, *, peer=False):
        package = self.directory / "magic-items"
        shutil.copytree(ROOT / "scenarios/tests/magic-items", package)
        if peer:
            # A real externally controlled peer holds a finite preparation boundary.
            with (package / "regions/1.toml").open("a", encoding="utf-8") as output:
                output.write('''
actors = [{ id = 2, at = [1, 3, 0], turn_ticks = 100, controller = "external", creature = { species = "figure", name = "partner", faction = "neutral", binding = "intellect", starting_skills = ["deception", "persuasion"], restoration = "choose_again", nutrition = { kind = "exempt" }, hit_dice = [{ source = "racial", purchases = [] }] } }]
''')
        validated = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                   capture_output=True, text=True, encoding="utf-8", timeout=30)
        self.assertEqual(validated.returncode, 0, validated.stdout + validated.stderr)
        return package

    def advance(self, subject, peers, predicate=settled):
        for _ in range(24):
            current = self.snapshot(subject)
            if predicate(current):
                return current
            for peer in peers:
                frame = self.snapshot(peer)
                if frame["readiness"]["admission"] and observation(frame)["ready"]:
                    self.assertIsNone(self.act(peer, {"type": "wait"})["error"])
        self.fail("Magic item fixture did not reach its bounded ordinary decision: " + repr(current))

    def collect(self, subject, initial, peers=()):
        for ground in observation(initial)["ground_items"]:
            taken = self.act(subject, {"type": "take", "item": ground["item"]["id"], "quantity": None})
            self.assertIsNone(taken["error"])
            self.advance(subject, peers)
        return self.snapshot(subject)

    @non_graphical
    def test_text_ordinary_pickup_drink_read_identify_and_zap(self):
        self.server(scenario="magic-items")
        probe, initial = self.client(observe=True)
        text, _ = self.text_client()
        for ground in observation(initial)["ground_items"]:
            self.assertIn("Taken", text.command("take #" + ground["item"]["id"]))
        carried = self.snapshot(probe)
        potion, = items(carried, "potion")
        scroll, = items(carried, "scroll")
        first, second = items(carried, "wand")
        self.assertFalse(first["identified"])
        self.assertIsNone(charges(carried, first["id"]))
        self.assertIsNone(charges(carried, second["id"]))
        text.command("drink #" + potion["id"])
        drank = self.snapshot(probe)
        self.assertEqual(items(drank, "potion")[0]["quantity"], "1")
        self.assertTrue(items(drank, "potion")[0]["identified"])
        text.command("read #" + scroll["id"])
        pending = self.snapshot(probe)
        self.assertEqual(interactions(pending)["identification"]["classes"], ["wand"])
        text.command("identify #" + first["id"])
        learned = self.snapshot(probe)
        self.assertIsNone(interactions(learned)["identification"])
        self.assertTrue(all(item["identified"] for item in items(learned, "wand")))
        self.assertEqual(charges(learned, first["id"]), 3)
        self.assertIsNone(charges(learned, second["id"]), "type knowledge must not invent another instance's charges")
        self.assertIn("3 charges", text.command("inventory"))
        text.command("zap #" + first["id"] + " east")
        zapped = self.snapshot(probe)
        self.assertEqual(charges(zapped, first["id"]), 2)
        self.assertEqual(len(items(zapped, "wand")), 2)
        self.assertFalse(zapped["state"]["wizard_game"])

    @non_graphical
    def test_text_cursed_armor_enchantment_and_source_free_reopen(self):
        package = self.package()
        path = package / "scenario.toml"
        manifest = tomllib.loads(path.read_text(encoding="utf-8"))
        manifest["archetypes"].update({
            "mail": {"name": "ring mail", "identity": "fixture.mail", "class": "armor",
                     "equipment": {"slot": "body_armor", "armor": {"physical_reduction": 2, "material": "metal", "rustproof": False}, "modifiers": []},
                     "equipment_generation": {"kind": "armor", "curse_percent": 100, "enchant_percent": 0, "minimum": 1, "maximum": 1}},
            "armor_identify": {"name": "scroll of identify armor", "identity": "fixture.identify_armor", "class": "scroll", "stackable": True,
                               "appearance_pool": "armor_pages", "readable": {"identify": ["armor"], "discovery": "on_use"}},
            "armor_enchant": {"name": "scroll of enchant armor", "identity": "fixture.enchant_armor", "class": "scroll", "stackable": True,
                              "appearance_pool": "enchant_pages", "readable": {"discovery": "never", "effects": [
                                  {"kind": "alter_equipment", "selection": {"kind": "slot", "slot": "body_armor", "class": "armor", "last": False}, "alteration": "enchant_armor"}]}},
        })
        manifest["appearance_pools"].update({"armor_pages": {"appearances": ["indigo page"]}, "enchant_pages": {"appearances": ["silver page"]}})
        manifest["characters"][0].update({"known_identities": ["fixture.mail"], "loadout": [
            {"archetype": "mail", "quantity": 1, "equipped_slot": 1},
            {"archetype": "mail", "quantity": 1},
            {"archetype": "armor_identify", "quantity": 1},
            {"archetype": "armor_enchant", "quantity": 2},
        ]})
        path.write_text(toml_source(manifest), encoding="utf-8")
        region_path = package / "regions/1.toml"
        region = tomllib.loads(region_path.read_text(encoding="utf-8"))
        region["items"] = []
        region_path.write_text(toml_source(region), encoding="utf-8")
        validated = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                   capture_output=True, text=True, encoding="utf-8", timeout=30)
        self.assertEqual(validated.returncode, 0, validated.stdout + validated.stderr)
        server = self.server("--checkpoint-interval", 1, scenario=package)
        probe, initial = self.client(observe=True)
        text, _ = self.adventure()
        worn = next(row["item"] for row in interactions(initial)["inventory"] if row["equipped_slot"] == 1)
        spare = next(row["id"] for row in items(initial, "armor") if row["id"] != worn)
        self.assertTrue(any(row["name"] == "indigo page" for row in items(initial, "scroll")))
        enchant = next(row["id"] for row in items(initial, "scroll") if row["name"] == "silver page")
        def said(command, expected):
            # Paid identification can refresh its prompt independently. Wait for
            # this command's narration before using another client's snapshot.
            self.send(text, command)
            output = text.until(lambda line: expected in line.lower())
            output += text.until(lambda line: line == "> ")
            return output
        def row(frame, item):
            return next(r for r in interactions(frame)["inventory"] if r["item"] == item)
        self.assertIsNone(row(initial, worn)["known_cursed"])
        failed_text = said("remove first ring mail", "it is cursed")
        self.assertIn("curs", failed_text.lower())
        failed = self.snapshot(probe)
        self.assertEqual(row(failed, worn)["equipped_slot"], 1)
        self.assertTrue(row(failed, worn)["known_cursed"])
        self.assertIsNone(row(failed, worn)["known_equipment"])
        said("read indigo page", "choose an item to identify")
        said("identify #" + worn, "you identify the ring mail")
        identified = self.snapshot(probe)
        self.assertIsNotNone(row(identified, worn)["known_equipment"])
        self.assertIsNone(row(identified, spare)["known_equipment"])
        said("read silver page", "finish reading the silver page")
        enchanted = self.snapshot(probe)
        self.assertFalse(row(enchanted, worn)["known_cursed"])
        self.assertNotEqual(row(identified, worn)["known_equipment"], row(enchanted, worn)["known_equipment"])
        remaining = next(i for i in items(enchanted, "scroll") if i["id"] == enchant)
        self.assertEqual(remaining["quantity"], "1")
        self.assertFalse(remaining["identified"])
        said("remove first ring mail", "finish removing the ring mail")
        removed = self.snapshot(probe)
        self.assertIsNone(row(removed, worn)["equipped_slot"])
        self.assertIsNone(self.request(probe, {"type": "save"})["error"])
        text.stop()
        probe.stop()
        server.stop(save=False)
        shutil.rmtree(package)
        self.server("--checkpoint-interval", 1, scenario=ROOT / "scenarios/two-room", seed=None)
        probe, restored = self.client(observe=True)
        self.assertEqual(interactions(restored)["inventory"], interactions(removed)["inventory"])
        spectator, hidden = self.client(SPECTATOR_TOKEN, observe=True)
        for item in interactions(hidden)["inventory"]:
            self.assertIsNone(item["known_cursed"])
            self.assertIsNone(item["known_protected"])
            self.assertIsNone(item["known_equipment"])
        self.assertFalse(restored["state"]["wizard_game"])

    @non_graphical
    def test_ring_wear_paid_union_choice_curse_and_source_free_restart(self):
        package = self.package()
        path = package / "scenario.toml"
        manifest = tomllib.loads(path.read_text(encoding="utf-8"))
        # A bounded birth policy forces the negative branch without wizard edits.
        # Exhaustive -1/+1/+2 distribution belongs to equipment_generation tests.
        manifest["archetypes"]["ring"] = {
            "name": "ring of increase damage", "identity": "magic.damage_ring",
            "class": "ring", "magical": True, "stackable": False,
            "appearance_pool": "rings",
            "equipment_generation": {"kind": "signed_potency", "negative": 1, "positive": 0},
            "equipment": {"slot": "ring", "modifiers": [], "potency_modifiers": [
                {"scope": "always", "op": {"kind": "add",
                 "stat": {"kind": "attack_damage_roll_bonus"}, "value": 1}}]},
        }
        manifest["appearance_pools"]["rings"] = {"appearances": ["agate ring"]}
        manifest["archetypes"]["identify"]["readable"]["identify"] = ["ring", "wand"]
        manifest["characters"][0]["loadout"] = [
            {"archetype": "ring", "quantity": 1},
            {"archetype": "ring", "quantity": 1},
            {"archetype": "ember", "quantity": 1},
            {"archetype": "identify", "quantity": 2},
        ]
        path.write_text(toml_source(manifest), encoding="utf-8")
        region_path = package / "regions/1.toml"
        region = tomllib.loads(region_path.read_text(encoding="utf-8"))
        region["items"] = []
        region_path.write_text(toml_source(region), encoding="utf-8")
        validated = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                   capture_output=True, text=True, encoding="utf-8", timeout=30)
        self.assertEqual(validated.returncode, 0, validated.stdout + validated.stderr)
        server = self.server("--checkpoint-interval", 1, scenario=package)
        owner, initial = self.client()
        ring, spare = [item["id"] for item in items(initial, "ring")]
        wand = items(initial, "wand")[0]["id"]
        scroll = items(initial, "scroll")[0]["id"]

        def row(frame, item):
            return next(value for value in interactions(frame)["inventory"] if value["item"] == item)

        def execute(action):
            result = self.act(owner, action)
            self.assertIsNone(result["error"])
            return self.advance(owner, [])

        worn = execute({"type": "equip", "item": ring, "slot": 2})
        self.assertEqual(row(worn, ring)["equipped_slot"], 2)
        self.assertTrue(all(not item["identified"] for item in items(worn, "ring")))
        self.assertTrue(all(item["name"] == "agate ring" for item in items(worn, "ring")))
        self.assertIsNone(row(worn, ring)["known_cursed"])
        self.assertIsNone(row(worn, ring)["known_equipment"])
        pending = self.act(owner, {"type": "read", "item": scroll})
        self.assertIsNone(pending["error"])
        choice = interactions(pending)["identification"]
        self.assertEqual(choice["classes"], ["ring", "wand"])
        self.assertEqual(set(choice["items"]), {ring, spare, wand})
        self.assertEqual(items(pending, "scroll")[0]["quantity"], "1")
        self.assertIsNone(self.request(owner, {"type": "save"})["error"])
        owner.stop()
        server.stop(save=False)
        shutil.rmtree(package)
        self.server("--checkpoint-interval", 1, scenario=ROOT / "scenarios/two-room", seed=None)
        owner, restored = self.client()
        self.assertEqual(interactions(restored)["identification"], choice)
        self.assertEqual(interactions(restored)["inventory"], interactions(pending)["inventory"])
        spectator, hidden = self.client(SPECTATOR_TOKEN, observe=True)
        self.assertIsNone(interactions(hidden)["identification"])
        self.assertTrue(all(value["known_equipment"] is None and value["known_cursed"] is None
                            for value in interactions(hidden)["inventory"]))
        learned = self.act(owner, {"type": "identify", "choice": choice["choice"], "item": ring})
        self.assertIsNone(learned["error"])
        self.assertEqual(observation(learned)["tick"], observation(restored)["tick"])
        self.assertIsNone(interactions(learned)["identification"])
        self.assertTrue(all(item["identified"] for item in items(learned, "ring")))
        self.assertIsNotNone(row(learned, ring)["known_equipment"])
        modifiers = row(learned, ring)["known_equipment"]["modifiers"]
        self.assertEqual(len(modifiers), 1)
        fields = {field["id"]: field["value"] for field in modifiers[0]["fields"]}
        self.assertEqual(fields["stat"], {"type": "text", "value": "attack_damage_roll_bonus"})
        self.assertEqual(fields["value"], {"type": "signed", "value": "-1"})
        self.assertTrue(row(learned, ring)["known_cursed"])
        self.assertIsNone(row(learned, spare)["known_equipment"])
        self.assertIsNone(row(learned, spare)["known_cursed"])
        self.assertIsNone(charges(learned, wand))
        # Failure to remove an unknown second ring reveals only that instance's
        # curse, never its numeric modifier; type knowledge is already shared.
        second = execute({"type": "equip", "item": spare, "slot": 3})
        self.assertIsNone(row(second, spare)["known_cursed"])
        failed = self.act(owner, {"type": "unequip", "item": spare})
        self.assertEqual(row(failed, spare)["equipped_slot"], 3)
        self.assertTrue(row(failed, spare)["known_cursed"])
        self.assertIsNone(row(failed, spare)["known_equipment"])
        hidden = self.snapshot(spectator)
        for value in interactions(hidden)["inventory"]:
            self.assertIsNone(value["known_cursed"])
            self.assertIsNone(value["known_equipment"])
        self.assertFalse(failed["state"]["wizard_game"])

    def test_native_paid_choice_reopens_and_direction_cancel_keeps_exact_charges(self):
        package = self.package()
        server = self.server("--checkpoint-interval", 1, scenario=package)
        probe, initial = self.client()
        carried = self.collect(probe, initial)
        self.assertEqual(len(items(carried, "wand")), 2)
        self.assertIsNone(self.request(probe, {"type": "release_control"})["error"])
        window = self.launch("tor-client-ascii", ["--connect", self.address, "--report-frames"])
        shown = self.ascii_frame(window, lambda frame: frame["has_control"] and not frame["busy"])
        native = self.native_keys(window)
        if shown["messages"]["more"]:
            self.native_press(window, native, "Escape", "escape")
        opened = self.native_press(window, native, "F6", "read")
        self.assertEqual(len(opened["item_rows"]), 1)
        self.assertEqual(opened["state"], carried["state"], "opening Read spends no turn")
        choice = self.native_press(window, native, "Return", "enter", wait_for_ready=True)
        self.assertEqual(choice["identification"]["classes"], ["wand"])
        self.assertEqual(len(choice["identification_rows"]), 2)
        self.assertEqual(items(choice, "scroll")[0]["quantity"], "2")
        aborted = self.native_press(window, native, "Escape", "escape", wait_for_ready=True)
        self.assertIsNone(aborted["identification"])
        self.assertEqual(items(aborted, "scroll")[0]["quantity"], "2", "paid cancellation cannot refund a scroll")
        self.assertEqual(observation(aborted)["tick"], observation(choice)["tick"], "answering a paid choice costs no second turn")
        self.native_press(window, native, "F6", "read")
        choice = self.native_press(window, native, "Return", "enter", wait_for_ready=True)
        choice_token = choice["identification"]["choice"]
        self.assertEqual(items(choice, "scroll")[0]["quantity"], "1")
        self.assertIsNone(self.request(probe, {"type": "save"})["error"])
        window.stop()
        probe.stop()
        server.stop(save=False)
        shutil.rmtree(package)
        self.assertFalse(package.exists())
        self.server("--checkpoint-interval", 1, scenario=ROOT / "scenarios/two-room", seed=None)

        # The actual spectator window cannot present or answer the owner's paid choice.
        spectator, viewed = self.window(token=SPECTATOR_TOKEN)
        self.assertIsNone(viewed["identification"])
        self.assertEqual(viewed["identification_rows"], [])
        self.assertTrue(all(row["charges"] is None for row in interactions(viewed)["inventory"]))
        self.assertIn("read-only", self.key(spectator, "read")["status"])
        spectator.stop()

        window = self.launch("tor-client-ascii", ["--connect", self.address, "--report-frames"])
        restored = self.ascii_frame(window, lambda frame: frame["has_control"] and not frame["busy"])
        self.assertEqual(restored["identification"], choice["identification"])
        self.assertEqual(restored["state"], choice["state"])
        native = self.native_keys(window)
        self.native_press(window, native, "Down", "down")
        identified = self.native_press(window, native, "Return", "enter", wait_for_ready=True)
        first, second = items(identified, "wand")
        self.assertIsNone(identified["identification"])
        self.assertEqual(charges(identified, second["id"]), 3)
        self.assertIsNone(charges(identified, first["id"]))
        self.assertTrue(first["identified"] and second["identified"])
        self.assertEqual(restored["identification"]["choice"], choice_token)
        menu = self.native_press(window, native, "F12", "zap")
        self.assertEqual(len(menu["item_rows"]), 2)
        self.assertNotIn("charges", menu["item_rows"][0])
        self.assertIn("charges: 3", menu["item_rows"][1])
        self.native_press(window, native, "Down", "down")
        aimed = self.native_press(window, native, "Return", "enter")
        self.assertEqual(aimed["zap_item"], second["id"])
        cancelled = self.native_press(window, native, "Escape", "escape")
        self.assertEqual(cancelled["state"], identified["state"], "aim cancellation is free")
        self.assertIsNone(cancelled["zap_item"])
        self.native_press(window, native, "F12", "zap")
        self.native_press(window, native, "Down", "down")
        self.native_press(window, native, "Return", "enter")
        fired = self.native_press(window, native, "Right", "right", wait_for_ready=True)
        self.assertEqual(charges(fired, second["id"]), 2)
        self.assertIsNone(charges(fired, first["id"]))
        self.assertEqual(len(items(fired, "wand")), 2)
        self.assertIsNone(fired["zap_item"])
        self.assertTrue(fired["window_open"])
        self.assertFalse(fired["state"]["wizard_game"])

    @non_graphical
    def test_paid_device_preparation_reopens_without_sources_or_second_charge(self):
        package = self.package(peer=True)
        server = self.server("--checkpoint-interval", 1, scenario=package)
        peer, _ = self.client(actor=2)
        subject, initial = self.client()
        carried = self.collect(subject, initial, [peer])
        wand = items(carried, "wand")[0]
        started = self.act(subject, {"type": "zap", "item": wand["id"], "aim": {"kind": "direction", "direction": "east"}})
        self.assertIsNone(started["error"])
        self.assertIsNotNone(observation(started)["interactions"]["preparation"])
        self.assertIsNone(self.request(subject, {"type": "release_control"})["error"])
        paused = self.snapshot(subject)
        if not any(row["phase"] == "paused" for row in paused["intentions"]):
            paused = self.frame(subject, lambda frame: any(row["phase"] == "paused" for row in frame["intentions"]))
        saved = checkpoint(self, subject)
        pending = saved["actors"]["1"]["pending"]
        self.assertFalse(pending["active"])
        self.assertEqual(pending["plan"]["kind"], "zap")
        self.assertEqual(pending["plan"]["charges"], 3)
        self.assertGreater(pending["remaining"], 0)
        subject.stop()
        peer.stop()
        server.stop(save=False)
        shutil.rmtree(package)
        self.server("--checkpoint-interval", 1, scenario=ROOT / "scenarios/two-room", seed=None)
        peer, _ = self.client(actor=2)
        subject, restored = self.client()
        self.assertEqual(restored["intentions"], paused["intentions"])
        self.assertEqual(checkpoint(self, subject), saved, "source-free reopen must preserve the exact paid ray plan and RNG")
        self.assertIsNone(self.command(subject, {"type": "resume_intention"})["error"])
        completed = self.advance(subject, [peer])
        final = checkpoint(self, subject)
        self.assertIsNone(final["actors"]["1"]["pending"])
        device = final["items"][str(pending["plan"]["item"])]
        self.assertEqual(device["charges"], 2)
        self.assertEqual(len(items(completed, "wand")), 2)
        self.assertFalse(completed["state"]["wizard_game"])


    @non_graphical
    def test_text_nothing_unknown_direction_and_identified_self_skip(self):
        package = prepare_self_device(self)
        self.server("--checkpoint-interval", 1, scenario=package)
        probe, initial = self.client(observe=True)
        text, _ = self.text_client()
        for ground in observation(initial)["ground_items"]:
            self.assertIn("Taken", text.command("take #" + ground["item"]["id"]))
        carried = self.snapshot(probe)
        first, second = items(carried, "wand")
        scroll, = items(carried, "scroll")
        self.assertTrue(all(not row["identified"] for row in items(carried, "wand")))
        modes = {row["item"]: row["device_activation"] for row in interactions(carried)["inventory"]}
        self.assertEqual(modes[first["id"]], "direction")
        self.assertEqual(modes[second["id"]], "direction")
        before = checkpoint(self, probe)
        self.assertEqual(before["actors"]["1"]["turn_ticks"], 100)
        self.assertEqual(sorted(before["items"][str(i)]["charges"] for i in (30, 31)), [3, 3])
        self.assertIn("direction", text.command("zap #" + first["id"]).lower())
        self.assertEqual(checkpoint(self, probe), before, "unsubmitted missing aim costs nothing")
        text.command("zap #" + first["id"] + " east")
        spent = self.snapshot(probe)
        after = checkpoint(self, probe)
        self.assertEqual(int(observation(spent)["tick"]), int(observation(carried)["tick"]) + 100)
        self.assertEqual(sorted(after["items"][str(i)]["charges"] for i in (30, 31)), [2, 3])
        self.assertEqual(after["rules_rng"], before["rules_rng"])
        self.assertEqual(observation(spent)["combat"]["own_stats"]["conditions"],
                         observation(carried)["combat"]["own_stats"]["conditions"])
        self.assertTrue(all(not row["identified"] for row in items(spent, "wand")))
        self.assertIsNone(charges(spent, first["id"]))
        text.command("read #" + scroll["id"])
        self.assertIsNotNone(interactions(self.snapshot(probe))["identification"])
        text.command("identify #" + first["id"])
        learned = self.snapshot(probe)
        self.assertEqual(charges(learned, first["id"]), 2)
        self.assertIsNone(charges(learned, second["id"]))
        self.assertTrue(all(row["identified"] for row in items(learned, "wand")))
        modes = {row["item"]: row["device_activation"] for row in interactions(learned)["inventory"]}
        self.assertEqual(modes[first["id"]], "self_target")
        self.assertEqual(modes[second["id"]], "self_target")
        spectator, followed = self.client(SPECTATOR_TOKEN, observe=True)
        followed_modes = {row["item"]: row["device_activation"] for row in interactions(followed)["inventory"]}
        self.assertEqual(followed_modes, modes)
        known_before = checkpoint(self, probe)
        self.assertEqual(known_before["actors"]["1"]["turn_ticks"], 100)
        text.command("zap #" + first["id"])
        final = self.snapshot(probe)
        self.assertEqual(int(observation(final)["tick"]), int(observation(learned)["tick"]) + 100)
        self.assertEqual(charges(final, first["id"]), 1)
        self.assertIsNone(charges(final, second["id"]))
        self.assertFalse(final["state"]["wizard_game"])
        messages = [json.loads(line).get("message") for line in probe.transcript if line.startswith("{")]
        self.self_device_sample = next(message for message in reversed(messages)
                                      if message and message["type"] == "snapshot")

    @non_graphical
    def test_self_condition_reaches_user_not_direction_recipient(self):
        package = prepare_self_device(self, peer=True, condition=True)
        self.server("--checkpoint-interval", 1, scenario=package)
        peer, _ = self.client(actor=2)
        subject, initial = self.client()
        carried = self.collect(subject, initial, [peer])
        wand = items(carried, "wand")[0]
        def selected(frame):
            return [row for row in observation(frame)["combat"]["own_stats"]["conditions"]
                    if row["condition"]["id"] == "fixture.self_device"]
        self.assertEqual(selected(carried), [])
        self.assertEqual(selected(self.snapshot(peer)), [])
        before = checkpoint(self, subject)
        started = self.act(subject, {"type": "zap", "item": wand["id"],
                                    "aim": {"kind": "direction", "direction": "east"}})
        self.assertIsNone(started["error"])
        completed = self.advance(subject, [peer])
        self.assertEqual(len(selected(completed)), 1)
        self.assertEqual(selected(self.snapshot(peer)), [], "supplied direction is not self-effect target authority")
        after = checkpoint(self, subject)
        self.assertEqual(sorted(after["items"][str(i)]["charges"] for i in (30, 31)), [2, 3])
        self.assertEqual(after["rules_rng"], before["rules_rng"])
        self.assertTrue(all(not row["identified"] for row in items(completed, "wand")))
        self.assertFalse(completed["state"]["wizard_game"])


    @non_graphical
    def test_imported_nothing_unknown_direction_paid_identity_and_self_skip(self):
        package = prepare_imported_nothing(self)
        self.server("--checkpoint-interval", 1, scenario=package)
        probe, initial = self.client(observe=True)
        text, _ = self.text_client()
        for ground in observation(initial)["ground_items"]:
            self.assertIn("Taken", text.command("take #" + ground["item"]["id"]))
        carried = self.snapshot(probe)
        self.assertEqual(observation(carried)["combat"]["own_stats"]["attributes"]["speed"], 0,
                         "effective compiled speed must preserve exact base-turn timing")
        first, second = items(carried, "wand")
        scroll, = items(carried, "scroll")
        self.assertTrue(all(not row["identified"] for row in items(carried, "wand")))
        modes = {row["item"]: row["device_activation"] for row in interactions(carried)["inventory"]}
        self.assertEqual(modes[first["id"]], "direction")
        self.assertEqual(modes[second["id"]], "direction")
        before = checkpoint(self, probe)
        self.assertEqual(before["actors"]["1"]["turn_ticks"], 100)
        initial_counts = {str(i): before["items"][str(i)]["charges"] for i in (30, 31)}
        self.assertTrue(all(3 <= count <= 7 for count in initial_counts.values()))
        self.assertIn("direction", text.command("zap #" + first["id"]).lower())
        self.assertEqual(checkpoint(self, probe), before, "unsubmitted missing aim costs nothing")
        text.command("zap #" + first["id"] + " east")
        spent = self.snapshot(probe)
        after = checkpoint(self, probe)
        self.assertEqual(int(observation(spent)["tick"]), int(observation(carried)["tick"]) + 100)
        spent_ids = [key for key in initial_counts if after["items"][key]["charges"] == initial_counts[key] - 1]
        self.assertEqual(len(spent_ids), 1)
        selected_physical = spent_ids[0]
        for key in initial_counts:
            self.assertEqual(after["items"][key]["charges"], initial_counts[key] - (key == selected_physical))
        remaining = initial_counts[selected_physical] - 1
        self.assertEqual(after["rules_rng"], before["rules_rng"])
        self.assertEqual(observation(spent)["combat"]["own_stats"]["conditions"],
                         observation(carried)["combat"]["own_stats"]["conditions"])
        self.assertTrue(all(not row["identified"] for row in items(spent, "wand")))
        self.assertIsNone(charges(spent, first["id"]))
        text.command("read #" + scroll["id"])
        self.assertIsNotNone(interactions(self.snapshot(probe))["identification"])
        text.command("identify #" + first["id"])
        learned = self.snapshot(probe)
        self.assertEqual(charges(learned, first["id"]), remaining)
        self.assertIsNone(charges(learned, second["id"]))
        self.assertTrue(all(row["identified"] for row in items(learned, "wand")))
        modes = {row["item"]: row["device_activation"] for row in interactions(learned)["inventory"]}
        self.assertEqual(modes[first["id"]], "self_target")
        self.assertEqual(modes[second["id"]], "self_target")
        spectator, followed = self.client(SPECTATOR_TOKEN, observe=True)
        followed_modes = {row["item"]: row["device_activation"] for row in interactions(followed)["inventory"]}
        self.assertEqual(followed_modes, modes)
        known_before = checkpoint(self, probe)
        self.assertEqual(known_before["actors"]["1"]["turn_ticks"], 100)
        text.command("zap #" + first["id"])
        final = self.snapshot(probe)
        self.assertEqual(int(observation(final)["tick"]), int(observation(learned)["tick"]) + 100)
        self.assertEqual(charges(final, first["id"]), remaining - 1)
        self.assertIsNone(charges(final, second["id"]))
        self.assertFalse(final["state"]["wizard_game"])



    def exercise_imported_light(self, *, passage):
        package = prepare_imported_light(self, passage=passage)
        server = self.server("--checkpoint-interval", 1, scenario=package)
        probe, initial = self.client(observe=True)
        text, _ = self.text_client()
        self.assertEqual(len(observation(initial)["ground_items"]), 1)
        wand = observation(initial)["ground_items"][0]["item"]["id"]
        self.assertIn("Taken", text.command("take #" + wand))
        carried = self.snapshot(probe)
        self.assertEqual(observation(carried)["combat"]["own_stats"]["attributes"]["speed"], 0)
        self.assertFalse(items(carried, "wand")[0]["identified"])
        before = checkpoint(self, probe)
        count = before["items"]["30"]["charges"]
        self.assertTrue(10 <= count <= 19)
        self.assertEqual(before["actors"]["1"]["turn_ticks"], 100)
        initial_cells = {cell["key"]: cell for cell in observation(carried)["visible_cells"]}
        self.assertTrue(initial_cells)
        self.assertEqual(observation(carried)["position"], {"x": 0, "y": 0, "z": 0})
        # Wire cell positions are observer-frame offsets, not source coordinates
        # (server adapt::offset(cell.offset)). This stationary, unrotated chamber
        # starts at the asserted source anchor [1, 2, 0].
        def is_room_surface(cell):
            offset = cell["position"]
            x, y, z = offset["x"] + 1, offset["y"] + 2, offset["z"]
            interior = 0 <= x <= 6 and 0 <= y <= 4 and 0 <= z <= 1
            x_face = x in (-1, 7) and 0 <= y <= 4 and 0 <= z <= 1
            y_face = y in (-1, 5) and 0 <= x <= 6 and 0 <= z <= 1
            z_face = z in (-1, 2) and 0 <= x <= 6 and 0 <= y <= 4
            return interior or x_face or y_face or z_face
        self.assertTrue(any(is_room_surface(cell) for cell in initial_cells.values()))
        self.assertTrue(all(not cell["lit"] for cell in initial_cells.values()))
        text.command("zap #" + wand + " east")
        first = self.snapshot(probe)
        saved = checkpoint(self, probe)
        self.assertEqual(saved["items"]["30"]["charges"], count - 1)
        self.assertEqual(saved["rules_rng"], before["rules_rng"])
        self.assertEqual(int(observation(first)["tick"]), int(observation(carried)["tick"]) + 100)
        self.assertTrue(items(first, "wand")[0]["identified"], "OnUse identifies even when no room changes")
        offer = next(row for row in interactions(first)["inventory"] if row["item"] == wand)
        self.assertEqual(offer["device_activation"], "self_target")
        self.assertIsNone(offer["charges"], "OnUse identity is not paid exact-instance identification")
        first_cells = {cell["key"]: cell for cell in observation(first)["visible_cells"]}
        self.assertTrue(initial_cells.keys() <= first_cells.keys())
        # Interior and six face shells only: an observed diagonal shell corner
        # is explicitly excluded, rather than broadening canonical illumination.
        for cell in first_cells.values():
            expected_lit = not passage and is_room_surface(cell)
            self.assertEqual(cell["lit"], expected_lit, cell["position"])
        for key, cell in initial_cells.items():
            if not is_room_surface(cell):
                self.assertFalse(first_cells[key]["lit"], "diagonal shell must stay dark")
        text.command("zap #" + wand)
        second = self.snapshot(probe)
        second_saved = checkpoint(self, probe)
        self.assertEqual(second_saved["items"]["30"]["charges"], count - 2)
        self.assertEqual(int(observation(second)["tick"]), int(observation(first)["tick"]) + 100)
        self.assertEqual(second_saved["rules_rng"], saved["rules_rng"])
        self.assertEqual(observation(second)["visible_cells"], observation(first)["visible_cells"],
                         "already-lit room and passage are paid illumination no-ops")
        self.assertIsNone(charges(second, wand))
        self.assertFalse(second["state"]["wizard_game"])
        text.stop(save=False)
        probe.stop(save=False)
        server.stop(save=False)
        shutil.rmtree(package)
        self.server("--checkpoint-interval", 1, scenario=ROOT / "scenarios/two-room", seed=None)
        reopened, restored = self.client(observe=True)
        self.assertEqual(checkpoint(self, reopened), second_saved)
        self.assertEqual(observation(restored)["visible_cells"], observation(second)["visible_cells"])
        self.assertTrue(items(restored, "wand")[0]["identified"])

    @non_graphical
    def test_imported_room_light_identifies_illuminates_and_persists_without_sources(self):
        self.exercise_imported_light(passage=False)

    @non_graphical
    def test_imported_passage_light_identifies_and_spends_without_illumination(self):
        self.exercise_imported_light(passage=True)


    @non_graphical
    def test_imported_drain_empty_unknown_identified_self_and_source_free_reopen(self):
        package = prepare_imported_drain(self)
        server = self.server("--checkpoint-interval", 1, scenario=package)
        probe, initial = self.client(observe=True)
        text, _ = self.text_client()
        for ground in observation(initial)["ground_items"]:
            self.assertIn("Taken", text.command("take #" + ground["item"]["id"]))
        carried = self.snapshot(probe)
        self.assertEqual(observation(carried)["combat"]["own_stats"]["attributes"]["speed"], 0,
                         "effective compiled speed must preserve exact base-turn timing")
        self.assertEqual(int(observation(carried)["combat"]["hp"]), 21)
        self.assertEqual(int(observation(carried)["combat"]["max_hp"]), 21)
        first, second = items(carried, "wand")
        scroll, = items(carried, "scroll")
        self.assertTrue(all(not row["identified"] for row in items(carried, "wand")))
        modes = {row["item"]: row["device_activation"] for row in interactions(carried)["inventory"]}
        self.assertEqual(modes[first["id"]], "direction")
        self.assertEqual(modes[second["id"]], "direction")
        before = checkpoint(self, probe)
        self.assertEqual(before["actors"]["1"]["turn_ticks"], 100)
        initial_counts = {str(i): before["items"][str(i)]["charges"] for i in (30, 31)}
        self.assertTrue(all(3 <= count <= 7 for count in initial_counts.values()))
        self.assertIn("direction", text.command("zap #" + first["id"]).lower())
        self.assertEqual(checkpoint(self, probe), before, "unsubmitted missing aim costs nothing")
        text.command("zap #" + first["id"] + " east")
        spent = self.snapshot(probe)
        after = checkpoint(self, probe)
        self.assertEqual(int(observation(spent)["tick"]), int(observation(carried)["tick"]) + 100)
        spent_ids = [key for key in initial_counts if after["items"][key]["charges"] == initial_counts[key] - 1]
        self.assertEqual(len(spent_ids), 1)
        selected_physical = spent_ids[0]
        for key in initial_counts:
            self.assertEqual(after["items"][key]["charges"], initial_counts[key] - (key == selected_physical))
        self.assertEqual(int(observation(spent)["combat"]["hp"]), 21, "empty scope charges without caster loss")
        remaining = initial_counts[selected_physical] - 1
        self.assertEqual(after["rules_rng"], before["rules_rng"])
        self.assertEqual(observation(spent)["combat"]["own_stats"]["conditions"],
                         observation(carried)["combat"]["own_stats"]["conditions"])
        self.assertTrue(all(not row["identified"] for row in items(spent, "wand")))
        self.assertIsNone(charges(spent, first["id"]))
        text.command("read #" + scroll["id"])
        self.assertIsNotNone(interactions(self.snapshot(probe))["identification"])
        text.command("identify #" + first["id"])
        learned = self.snapshot(probe)
        self.assertEqual(charges(learned, first["id"]), remaining)
        self.assertIsNone(charges(learned, second["id"]))
        self.assertTrue(all(row["identified"] for row in items(learned, "wand")))
        modes = {row["item"]: row["device_activation"] for row in interactions(learned)["inventory"]}
        self.assertEqual(modes[first["id"]], "self_target")
        self.assertEqual(modes[second["id"]], "self_target")
        spectator, followed = self.client(SPECTATOR_TOKEN, observe=True)
        followed_modes = {row["item"]: row["device_activation"] for row in interactions(followed)["inventory"]}
        self.assertEqual(followed_modes, modes)
        known_before = checkpoint(self, probe)
        self.assertEqual(known_before["actors"]["1"]["turn_ticks"], 100)
        text.command("zap #" + first["id"])
        final = self.snapshot(probe)
        self.assertEqual(int(observation(final)["tick"]), int(observation(learned)["tick"]) + 100)
        self.assertEqual(charges(final, first["id"]), remaining - 1)
        self.assertIsNone(charges(final, second["id"]))
        self.assertFalse(final["state"]["wizard_game"])
        self.assertEqual(int(observation(final)["combat"]["hp"]), 21)
        final_saved = checkpoint(self, probe)
        self.assertEqual(final_saved["rules_rng"], known_before["rules_rng"])
        text.stop(save=False)
        probe.stop(save=False)
        spectator.stop(save=False)
        server.stop(save=False)
        shutil.rmtree(package)
        self.server("--checkpoint-interval", 1, scenario=ROOT / "scenarios/two-room", seed=None)
        reopened, restored = self.client(observe=True)
        self.assertEqual(checkpoint(self, reopened), final_saved)
        self.assertEqual(int(observation(restored)["combat"]["hp"]), 21)
        self.assertEqual(charges(restored, first["id"]), remaining - 1)

    @non_graphical
    def test_imported_drain_weak_text_attempt_spends_time_not_charge_or_identity(self):
        package = prepare_imported_drain(self, weak=True)
        self.server("--checkpoint-interval", 1, scenario=package)
        probe, initial = self.client(observe=True)
        # Human narration belongs to adventure mode; --script is diagnostic output.
        text, _ = self.adventure()
        self.say(text, "take all")
        before = self.snapshot(probe)
        self.assertEqual(len(items(before, "wand")), 2)
        self.assertEqual(len(items(before, "scroll")), 1)
        self.assertEqual(observation(before)["ground_items"], [])
        wand = items(before, "wand")[0]
        self.assertEqual(int(observation(before)["combat"]["hp"]), 1)
        self.assertEqual(int(observation(before)["combat"]["max_hp"]), 1)
        self.assertEqual(observation(before)["combat"]["own_stats"]["attributes"]["speed"], 0)
        saved = checkpoint(self, probe)
        self.assertEqual(saved["actors"]["1"]["turn_ticks"], 100)
        self.assertTrue(all(3 <= saved["items"][key]["charges"] <= 7 for key in ("30", "31")))
        for attempt in range(2):
            reply = self.say(text, "zap #" + wand["id"] + " east")
            self.assertIn("too weak", reply.lower())
            self.assertNotIn("You finish zapping", reply)
            after = self.snapshot(probe)
            after_saved = checkpoint(self, probe)
            self.assertEqual(int(observation(after)["tick"]), int(observation(before)["tick"]) + 100 * (attempt + 1))
            self.assertEqual(int(observation(after)["combat"]["hp"]), 1)
            self.assertEqual(after_saved["rules_rng"], saved["rules_rng"])
            for key in ("30", "31"):
                self.assertEqual(after_saved["items"][key]["charges"], saved["items"][key]["charges"])
            self.assertTrue(all(not item["identified"] for item in items(after, "wand")))
            self.assertTrue(all(row["device_activation"] == "direction" for row in interactions(after)["inventory"] if row["zappable"]))
            self.assertIsNone(charges(after, wand["id"]))
            self.assertFalse(after["state"]["wizard_game"])
        messages = [json.loads(line).get("message") for line in probe.transcript if line.startswith("{")]
        self.drain_refusal_sample = next(message for message in reversed(messages)
            if message and message["type"] == "snapshot"
            and message["snapshot"]["state"]["observation"]["interactions"]["refused"])


    @non_graphical
    def test_imported_drain_nonempty_odd_health_pays_once_and_reopens_without_sources(self):
        package = prepare_imported_drain(self, target=True)
        server = self.server("--checkpoint-interval", 1, scenario=package)
        peer, _ = self.client(actor=2)
        subject, initial = self.client()
        carried = self.collect(subject, initial, [peer])
        self.assertTrue(settled(carried), "measured Zap begins at a settled ordinary decision")
        target_before = self.snapshot(peer)
        for frame in (carried, target_before):
            self.assertEqual(int(observation(frame)["combat"]["hp"]), 21)
            self.assertEqual(int(observation(frame)["combat"]["max_hp"]), 21)
            self.assertEqual(observation(frame)["combat"]["own_stats"]["attributes"]["speed"], 0)
            self.assertGreater(int(observation(frame)["combat"]["own_stats"]["health_recovery"]["period_ticks"]), 100)
        wand = items(carried, "wand")[0]
        before = checkpoint(self, subject)
        self.assertEqual(set(before["actors"]), {"1", "2"})
        for actor in (1, 2):
            self.assertEqual(before["actors"][str(actor)]["turn_ticks"], 100)
            self.assertEqual(actor_creature(before, actor)["injury"], 0)
        counts = {key: before["items"][key]["charges"] for key in ("30", "31")}
        self.assertTrue(all(3 <= count <= 7 for count in counts.values()))
        accepted = self.act(subject, {"type": "zap", "item": wand["id"],
                                     "aim": {"kind": "direction", "direction": "west"}})
        self.assertIsNone(accepted["error"])
        completed = self.advance(subject, [peer])
        after = checkpoint(self, subject)
        target_after = self.snapshot(peer)
        self.assertEqual(int(observation(completed)["tick"]), int(observation(carried)["tick"]) + 100)
        self.assertEqual(int(observation(completed)["combat"]["hp"]), 10)
        self.assertEqual(int(observation(target_after)["combat"]["hp"]), 11)
        self.assertEqual(actor_creature(after, 1)["injury"], 11)
        self.assertEqual(actor_creature(after, 2)["injury"], 10)
        changed = [key for key in counts if after["items"][key]["charges"] != counts[key]]
        self.assertEqual(len(changed), 1)
        self.assertEqual(after["items"][changed[0]]["charges"], counts[changed[0]] - 1)
        self.assertEqual(after["rules_rng"], before["rules_rng"])
        self.assertTrue(all(not item["identified"] for item in items(completed, "wand")))
        self.assertIsNone(charges(completed, wand["id"]))
        self.assertFalse(completed["state"]["wizard_game"])
        subject.stop(save=False)
        peer.stop(save=False)
        server.stop(save=False)
        shutil.rmtree(package)
        self.server("--checkpoint-interval", 1, scenario=ROOT / "scenarios/two-room", seed=None)
        reopened, restored = self.client(observe=True)
        self.assertEqual(checkpoint(self, reopened), after)
        self.assertEqual(int(observation(restored)["combat"]["hp"]), 10)
        self.assertTrue(all(not item["identified"] for item in items(restored, "wand")))


    def exercise_imported_first_contact(self, *, missile):
        package = prepare_imported_first_contact(self, missile=missile)
        server = self.server("--checkpoint-interval", 1, scenario=package)
        peer, _ = self.client(actor=2)
        collector, initial = self.client()
        self.collect(collector, initial, [peer])
        collector.stop(save=False)
        probe, _ = self.client(observe=True)
        text, _ = self.text_client()
        carried = self.advance(probe, [peer])
        self.assertTrue(settled(carried))
        target_before = self.snapshot(peer)
        for frame in (carried, target_before):
            self.assertEqual(int(observation(frame)["combat"]["hp"]), 50)
            self.assertEqual(int(observation(frame)["combat"]["max_hp"]), 50)
            self.assertEqual(observation(frame)["combat"]["own_stats"]["attributes"]["speed"], 0)
            self.assertGreater(int(observation(frame)["combat"]["own_stats"]["health_recovery"]["period_ticks"]), 100)
        owner_stats = observation(carried)["combat"]["own_stats"]
        target_stats = observation(target_before)["combat"]["own_stats"]
        before_names = [row["name"] for row in observation(carried)["visible_actors"]]
        if missile:
            self.assertIn("contact recipient", before_names)
        else:
            self.assertNotIn("contact recipient", before_names,
                             "the actual physical recipient must begin visually invisible")
        def rank(stats, skill):
            return next(row["rank"] for row in stats["skills"] if row["skill"]["id"] == skill)
        if missile:
            # Every retained 3d6 Lore result beats every target Agility save.
            # These are actual compiled public stats, not an ungranted CheckBonus.
            self.assertGreater(owner_stats["attributes"]["intellect"] + rank(owner_stats, "lore") + 3,
                               target_stats["attributes"]["speed"] + rank(target_stats, "agility") + 18)
        wand, = items(carried, "wand")
        scroll, = items(carried, "scroll")
        self.assertFalse(wand["identified"])
        self.assertNotIn("cancellation", wand["name"].lower())
        self.assertNotIn("magic missile", wand["name"].lower())
        offer = next(row for row in interactions(carried)["inventory"] if row["item"] == wand["id"])
        self.assertEqual(offer["device_activation"], "direction")
        self.assertIsNone(offer["charges"])
        before = checkpoint(self, probe)
        self.assertFalse(actor_creature(before, 2)["cancelled"])
        self.assertEqual(actor_creature(before, 2)["injury"], 0)
        self.assertEqual(before["actors"]["1"]["turn_ticks"], 100)
        count = before["items"]["30"]["charges"]
        self.assertTrue(3 <= count <= 7)
        self.assertIn("direction", text.command("zap #" + wand["id"]).lower())
        self.assertEqual(checkpoint(self, probe), before)
        text.command("zap #" + wand["id"] + " east")
        completed = self.advance(probe, [peer])
        after = checkpoint(self, probe)
        self.assertEqual(int(observation(completed)["tick"]), int(observation(carried)["tick"]) + 100)
        self.assertEqual(after["items"]["30"]["charges"], count - 1)
        self.assertEqual(int(observation(completed)["combat"]["hp"]), 50)
        self.assertFalse(completed["state"]["wizard_game"])
        self.assertIsNone(charges(completed, wand["id"]), "OnUse never grants paid instance charge knowledge")
        self.assertEqual(items(completed, "wand")[0]["identified"], missile)
        if missile:
            loss = actor_creature(after, 2)["injury"]
            self.assertIn(loss, (2, 3, 4, 5))
            self.assertEqual(int(observation(self.snapshot(peer))["combat"]["hp"]), 50 - loss)
            self.assertFalse(actor_creature(after, 2)["cancelled"])
        else:
            self.assertTrue(actor_creature(after, 2)["cancelled"])
            self.assertIn("contact recipient", [row["name"] for row in observation(completed)["visible_actors"]],
                          "cancellation removes the same body's authored invisibility")
            self.assertEqual(actor_creature(after, 2)["injury"], 0)
            self.assertEqual(after["rules_rng"], before["rules_rng"])
        # Paid Identify, unlike OnUse, discloses exact remaining instance charges.
        text.command("read #" + scroll["id"])
        self.advance(probe, [peer])
        self.assertIsNotNone(interactions(self.snapshot(probe))["identification"])
        text.command("identify #" + wand["id"])
        learned = self.advance(probe, [peer])
        self.assertTrue(items(learned, "wand")[0]["identified"])
        self.assertEqual(charges(learned, wand["id"]), count - 1)
        self.assertEqual(next(row for row in interactions(learned)["inventory"]
                              if row["item"] == wand["id"])["device_activation"], "direction")
        known = checkpoint(self, probe)
        self.assertIn("direction", text.command("zap #" + wand["id"]).lower())
        self.assertEqual(checkpoint(self, probe), known, "identified contact devices still require direction")
        text.stop(save=False)
        probe.stop(save=False)
        peer.stop(save=False)
        server.stop(save=False)
        shutil.rmtree(package)
        self.server("--checkpoint-interval", 1, scenario=ROOT / "scenarios/two-room", seed=None)
        reopened, restored = self.client(observe=True)
        self.assertEqual(checkpoint(self, reopened), known)
        self.assertTrue(items(restored, "wand")[0]["identified"])
        self.assertEqual(charges(restored, wand["id"]), count - 1)

    @non_graphical
    def test_imported_cancellation_text_hits_invisible_body_and_reopens_without_sources(self):
        self.exercise_imported_first_contact(missile=False)

    @non_graphical
    def test_imported_magic_missile_text_fails_one_save_and_reopens_without_sources(self):
        self.exercise_imported_first_contact(missile=True)

    @non_graphical
    def test_imported_cancellation_privileged_capture_records_actual_reach_and_effect(self):
        package = prepare_imported_first_contact(self, missile=False, grant=True)
        self.server("--checkpoint-interval", 1, scenario=package, wizard=True)
        peer, _ = self.client(actor=2)
        owner, initial = self.client()
        carried = self.collect(owner, initial, [peer])
        self.assertTrue(settled(carried))
        ability = next(row for row in observation(carried)["combat"]["own_stats"]["abilities"]
                       if row["id"] == "rogue.wand_cancellation")
        self.assertEqual(ability["reach"], {"kind": "until_obstruction"})
        self.assertEqual(ability["target_kind"], "directional_ray")
        self.assertFalse(ability["basic_attack"])
        self.assertFalse(ability["weapon_selection"])
        self.snapshot(owner)
        def message(client, kind):
            messages = [json.loads(line).get("message") for line in client.transcript if line.startswith("{")]
            return next(row for row in reversed(messages) if row and row["type"] == kind)
        reach_sample = message(owner, "snapshot")
        wizard, _ = self.client(WIZARD_TOKEN, observe=True)
        enabled = self.command(wizard, {"type": "wizard", "command": "combat capture on"})
        self.assertFalse(enabled.get("error"), enabled.get("error"))
        self.assertTrue(message(wizard, "combat_diagnostics")["report"]["enabled"])
        wand, = items(carried, "wand")
        before = checkpoint(self, owner)
        self.assertFalse(actor_creature(before, 2)["cancelled"])
        self.assertNotIn("contact recipient", [row["name"] for row in observation(carried)["visible_actors"]])
        accepted = self.act(owner, {"type": "zap", "item": wand["id"],
                                    "aim": {"kind": "direction", "direction": "east"}})
        self.assertIsNone(accepted["error"])
        completed = self.advance(owner, [peer])
        after = checkpoint(self, owner)
        self.assertEqual(after["items"]["30"]["charges"], before["items"]["30"]["charges"] - 1)
        self.assertTrue(actor_creature(after, 2)["cancelled"])
        self.assertEqual(after["rules_rng"], before["rules_rng"])
        self.assertFalse(items(completed, "wand")[0]["identified"])
        queried = self.command(wizard, {"type": "wizard", "command": "combat inspect"})
        self.assertFalse(queried.get("error"), queried.get("error"))
        effect_sample = message(wizard, "combat_diagnostics")
        effects = [effect for record in effect_sample["report"]["records"]
                   for effect in record["outcome"]["effects"] if effect["type"] == "cancel_specials"]
        self.assertEqual(len(effects), 1)
        self.assertTrue(effects[0]["changed"])
        self.assertEqual(str(effects[0]["target"]), "2")
        self.assertNotIn('"type":"combat_diagnostics"', "".join(owner.transcript).replace(" ", ""))
        self.first_contact_samples = {"reach": reach_sample, "effect": effect_sample}

    @non_graphical
    def test_imported_torch_text_explicit_control_fuel_drop_and_source_free_reopen(self):
        package = prepare_imported_torch(self)
        server = self.server("--checkpoint-interval", 1, scenario=package)
        probe, initial = self.client(observe=True)
        text, _ = self.adventure()
        self.say(text, "take all")
        carried = self.snapshot(probe)
        torch, = observation(carried)["inventory"]
        target = torch["id"]
        self.assertEqual(torch["class"], "tool")
        self.assertEqual(torch["name"], "torch")
        # Adventure binds disclosed nouns; probe handles are only for state lookup.
        self.assertEqual(observation(carried)["combat"]["own_stats"]["attributes"]["speed"], 0)
        def light(frame):
            return next(row for row in interactions(frame)["inventory"] if row["item"] == target)["portable_light"]
        def ticks(frame):
            return int(observation(frame)["tick"])
        def lit_cell(frame):
            return any(cell["position"] == {"x": 3, "y": 0, "z": 0} and cell["lit"]
                       for cell in observation(frame)["visible_cells"])
        self.assertEqual(light(carried), {"remaining": "100000", "lit": False,
                                         "ignitable": False, "extinguishable": False})
        self.assertFalse(lit_cell(carried))
        self.assertEqual(checkpoint(self, probe)["actors"]["1"]["turn_ticks"], 100)
        refused = self.say(text, "ignite torch")
        self.assertIn("can't ignite", refused.lower())
        self.assertEqual(ticks(self.snapshot(probe)), ticks(carried))
        self.say(text, "wield torch")
        held = self.snapshot(probe)
        self.assertFalse(light(held)["lit"], "equipping never auto-ignites")
        self.assertTrue(light(held)["ignitable"])
        self.assertFalse(lit_cell(held))
        self.assertIn("finish igniting", self.say(text, "ignite torch").lower())
        burning = self.snapshot(probe)
        self.assertEqual(ticks(burning), ticks(held) + 100)
        self.assertTrue(light(burning)["lit"])
        self.assertEqual(int(light(burning)["remaining"]), 100000)
        self.assertTrue(lit_cell(burning))
        def snapshot_message():
            messages = [json.loads(line).get("message") for line in probe.transcript if line.startswith("{")]
            return next(message for message in reversed(messages) if message and message["type"] == "snapshot")
        lit_sample = snapshot_message()
        self.assertIn("fuel:", self.say(text, "inventory").lower())
        self.say(text, "wait")
        elapsed = self.snapshot(probe)
        self.assertEqual(ticks(elapsed), ticks(burning) + 100)
        self.assertEqual(int(light(elapsed)["remaining"]), 99900)
        self.assertIn("finish extinguishing", self.say(text, "extinguish torch").lower())
        off = self.snapshot(probe)
        self.assertEqual(ticks(off), ticks(elapsed) + 100)
        self.assertFalse(light(off)["lit"])
        self.assertEqual(int(light(off)["remaining"]), 99800)
        self.assertFalse(lit_cell(off))
        self.torch_samples = {"lit": lit_sample, "off": snapshot_message()}
        self.say(text, "ignite torch")
        relit = self.snapshot(probe)
        self.say(text, "remove torch")
        stowed = self.snapshot(probe)
        self.assertFalse(light(stowed)["lit"])
        self.assertFalse(light(stowed)["ignitable"])
        self.assertEqual(int(light(stowed)["remaining"]),
                         int(light(relit)["remaining"]) - (ticks(stowed) - ticks(relit)))
        self.say(text, "wield torch")
        self.say(text, "ignite torch")
        before_drop = self.snapshot(probe)
        self.say(text, "drop torch")
        dropped = self.snapshot(probe)
        self.assertEqual(observation(dropped)["inventory"], [])
        self.assertTrue(lit_cell(dropped), "dropped flame remains a ground light source")
        saved = checkpoint(self, probe)
        self.assertTrue(saved["items"]["30"]["fuel"]["lit"])
        self.assertEqual(int(saved["items"]["30"]["fuel"]["remaining"]),
                         int(light(before_drop)["remaining"]) - (ticks(dropped) - ticks(before_drop)))
        self.assertFalse(dropped["state"]["wizard_game"])
        text.stop(save=False)
        probe.stop(save=False)
        server.stop(save=False)
        shutil.rmtree(package)
        self.server("--checkpoint-interval", 1, scenario=ROOT / "scenarios/two-room", seed=None)
        reopened, restored = self.client(observe=True)
        self.assertEqual(checkpoint(self, reopened), saved)
        self.assertTrue(lit_cell(restored))


    def test_native_ascii_torch_choices_explicit_ignite_extinguish_and_cancel(self):
        package = prepare_imported_torch(self)
        self.server(scenario=package)
        capture = self.directory / "torch.ppm"
        window = self.launch("tor-client-ascii", ["--connect", self.address, "--report-frames", "--capture", capture])
        initial = self.ascii_frame(window, lambda frame: frame.get("state") and not frame["busy"]
                                   and frame["can_admit_intention"])
        self.assertTrue(initial["window_open"])
        native = self.native_keys(window)
        def press(name, action, **completion):
            return self.native_press(window, native, name, action, **completion)
        press("g", "pickup")
        carried = press("Return", "enter", wait_for_ready=True, wait_for_admission=True)
        torch, = observation(carried)["inventory"]
        def light(frame):
            return next(row for row in interactions(frame)["inventory"]
                        if row["item"] == torch["id"])["portable_light"]
        self.assertFalse(light(carried)["lit"])
        self.assertFalse(light(carried)["ignitable"])
        press("w", "equip")
        held = press("Return", "enter", wait_for_ready=True, wait_for_admission=True)
        self.assertFalse(light(held)["lit"])
        self.assertTrue(light(held)["ignitable"])
        asking = press("x", "ignite")
        self.assertEqual(observation(asking), observation(held), "opening chooser is free")
        cancelled = press("Escape", "escape")
        self.assertEqual(observation(cancelled), observation(held))
        press("x", "ignite")
        lit = press("Return", "enter", wait_for_ready=True, wait_for_admission=True)
        self.assertTrue(light(lit)["lit"])
        self.assertTrue(light(lit)["extinguishable"])
        self.assertEqual(int(observation(lit)["tick"]), int(observation(held)["tick"]) + 100)
        self.assertTrue(any(cell["position"] == {"x": 3, "y": 0, "z": 0} and cell["lit"]
                            for cell in observation(lit)["visible_cells"]))
        self.assertIn("You finish igniting the torch.", lit["narration"])
        press("v", "extinguish")
        off = press("Return", "enter", wait_for_ready=True, wait_for_admission=True)
        self.assertFalse(light(off)["lit"])
        self.assertEqual(int(light(off)["remaining"]), 99900)
        self.assertIn("You finish extinguishing the torch.", off["narration"])
        pixels = read_capture(capture).split(b"\n", 3)[3]
        self.assertEqual(len(pixels), 1200 * 800 * 3)
        self.assertGreater(len(set(pixels)), 8)



if __name__ == "__main__":
    unittest.main()
