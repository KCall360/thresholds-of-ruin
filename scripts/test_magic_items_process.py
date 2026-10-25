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

from process_harness import ProcessTestCase, ROOT, SPECTATOR_TOKEN, non_graphical
from status_condition_package import checkpoint
from self_device_package import prepare as prepare_self_device
from nothing_catalog_package import prepare as prepare_imported_nothing
from light_catalog_package import prepare as prepare_imported_light


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


if __name__ == "__main__":
    unittest.main()
