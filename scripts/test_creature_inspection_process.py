"""Private creature inspection through real Text, headless and native clients."""
from scalar_wire import u64
from collections import Counter
import shutil
import subprocess
from process_harness import ProcessTestCase, ROOT, WIZARD_TOKEN
from process_harness import content_ids, creature_package, resource

def natural_attack_source(components, *, skill="heavy_weaponry", bonus=0, wind_up=60, recovery=40):
    """Author one checked natural strike using the current independent source schema."""
    from arena_matrix import toml_value
    return "melee = " + toml_value({"skill": skill, "bonus": bonus, "wind_up": wind_up,
        "recovery": recovery, "strikes": [{"offset": 0,
            "check": {"actor": "user", "skill": {"kind": "selected_weapon"},
                      "against": {"kind": "defense", "defense": "physical"}},
            "on_success": [{"kind": "damage", "damage": {"kind": "components", "components": components}}],
            "on_failure": [], "descriptors": [], "riders": []}]})


def strike_page_package(test):
    """Keep the shared 256-reference source limit; exercise many detail pages."""
    import copy
    import tomllib
    from arena_matrix import toml_source
    from strike_sequence_package import sequence_package
    package = sequence_package(test, autonomous=False)
    rules_path = package / "rules.toml"
    rules = tomllib.loads(rules_path.read_text(encoding="utf-8"))
    ids = [f"descriptor_{index:03}_" + "x" * 45 for index in range(128)]
    rules["descriptors"].extend({"id": identity, "name": "\\" * 256} for identity in ids)
    ability = next(row for row in rules["abilities"] if row["id"] == "triple_strike")
    strikes = ability["strikes"]["strikes"]
    for strike in strikes[:2]:
        strike["descriptors"] = list(ids)
    rules_path.write_text(toml_source(rules), encoding="utf-8", newline="\n")
    path = package / "scenario.toml"
    manifest = tomllib.loads(path.read_text(encoding="utf-8"))
    manifest["creatures"]["species"]["sequence_fighter"]["melee"]["strikes"] = copy.deepcopy(strikes)
    path.write_text(toml_source(manifest), encoding="utf-8", newline="\n")
    compiled = subprocess.run([test.bin / ("tor-scenario" + test.suffix), "validate", package],
                              capture_output=True, text=True, encoding="utf-8", timeout=30)
    test.assertEqual(compiled.returncode, 0, compiled.stderr)
    return package


def mixed_trigger_page_package(test):
    """Checked primary with both authored trigger branches across detail pages."""
    import copy
    import tomllib
    from arena_matrix import toml_source
    package = strike_page_package(test)
    rules_path = package / "rules.toml"
    rules = tomllib.loads(rules_path.read_text(encoding="utf-8"))
    ability = next(row for row in rules["abilities"] if row["id"] == "triple_strike")
    first = ability["strikes"]["strikes"][0]
    first["check"] = {"actor": "user", "skill": {"kind": "named", "skill": "heavy_weaponry"},
                      "against": {"kind": "difficulty", "formula": {"base": 10}}}
    first["on_success"] = [{"kind": "heal", "amount": {"kind": "fixed", "amount": amount}}
                           for amount in range(1, 16)]
    first["on_failure"] = [{"kind": "damage", "damage": {"kind": "components", "components": [
        {"category": "impact", "amount": {"kind": "fixed", "amount": 33}}]}}]
    first["riders"] = [
        {"trigger": "success", "effects": [{"kind": "heal", "amount": {"kind": "fixed", "amount": amount}}
                                             for amount in range(1, 15)], "descriptors": []},
        {"trigger": "failure", "effects": [{"kind": "damage", "damage": {"kind": "components", "components": [
            {"category": "impact", "amount": {"kind": "fixed", "amount": 77}}]}}], "descriptors": []},
    ]
    rules_path.write_text(toml_source(rules), encoding="utf-8", newline="\n")
    path = package / "scenario.toml"
    manifest = tomllib.loads(path.read_text(encoding="utf-8"))
    manifest["creatures"]["species"]["sequence_fighter"]["melee"]["strikes"] = copy.deepcopy(ability["strikes"]["strikes"])
    path.write_text(toml_source(manifest), encoding="utf-8", newline="\n")
    validated = subprocess.run([test.bin / ("tor-scenario" + test.suffix), "validate", package],
                               capture_output=True, text=True, encoding="utf-8", timeout=30)
    test.assertEqual(validated.returncode, 0, validated.stderr)
    return package


def natural_melee_page_package(test):
    """Normally validated inherited rows with explicit ability-added riders."""
    import tomllib
    from arena_matrix import toml_source
    package = mixed_trigger_page_package(test)
    path = package / "rules.toml"
    rules = tomllib.loads(path.read_text(encoding="utf-8"))
    ability = next(row for row in rules["abilities"] if row["id"] == "triple_strike")
    ability["strikes"] = {"kind": "natural_melee", "additional_riders": [{
        "trigger": "success", "condition": {"kind": "sequence_successful_hits", "minimum": 2}, "effects": [{"kind": "heal", "amount": {"kind": "fixed", "amount": 22}}],
        "descriptors": []}]}
    path.write_text(toml_source(rules), encoding="utf-8", newline="\n")
    validated = subprocess.run([test.bin / ("tor-scenario" + test.suffix), "validate", package],
                               capture_output=True, text=True, encoding="utf-8", timeout=30)
    test.assertEqual(validated.returncode, 0, validated.stderr)
    return package


def outgoing_edge_package(test, policy):
    """Normally compiled content; wizard access is used only for diagnostics."""
    import tomllib
    from arena_matrix import toml_source
    from strike_sequence_package import sequence_package
    package = sequence_package(test, autonomous=False)
    package = package.rename(test.directory / ("outgoing-edge-" + policy))
    rules_path = package / "rules.toml"
    rules = tomllib.loads(rules_path.read_text(encoding="utf-8"))
    ability = next(row for row in rules["abilities"] if row["id"] == "triple_strike")
    ability["outgoing_edge_policy"] = {"kind": policy}
    for strike in ability["strikes"]["strikes"]:
        strike["check"] = {"actor": "user", "skill": {"kind": "named", "skill": "heavy_weaponry"},
                           "against": {"kind": "difficulty", "formula": {"base": -1000}}}
        strike["on_success"][0]["damage"]["components"][0]["amount"] = {
            "kind": "rolled", "dice": {"count": 1, "sides": 6, "bonus": 0}}
    rules_path.write_text(toml_source(rules), encoding="utf-8", newline="\n")
    manifest_path = package / "scenario.toml"
    manifest = tomllib.loads(manifest_path.read_text(encoding="utf-8"))
    manifest["id"] = package.name
    modifiers = manifest["creatures"]["species"]["sequence_fighter"]["modifiers"]
    for stat, amount in (("edge", 5), ("edge", -2), ("check_edge", -6), ("incoming_edge", -1)):
        modifiers.append({"scope": "always", "op": {"kind": "add", "stat": {"kind": stat}, "value": amount}})
    manifest_path.write_text(toml_source(manifest), encoding="utf-8", newline="\n")
    compiled = subprocess.run([test.bin / ("tor-scenario" + test.suffix), "validate", package],
                              capture_output=True, text=True, encoding="utf-8", timeout=30)
    test.assertEqual(compiled.returncode, 0, compiled.stderr)
    return package


def trace_fields(step):
    """Read the generic diagnostic field vocabulary without shape adapters."""
    result = {}
    for field in step["fields"]:
        value = field["value"]
        if value["type"] == "dice":
            result[field["id"]] = value["values"]
        elif value["type"] in ("signed", "unsigned"):
            result[field["id"]] = int(value["value"])
        else:
            result[field["id"]] = value["value"]
    return result


def damage_total(record):
    return sum(effect["amount"] for effect in record["outcome"]["effects"] if effect["type"] == "damage")


class CreatureInspectionProcesses(ProcessTestCase):
    def _outgoing_policy_peer_wait(self, peer, owner_frame):
        # The owner's preparation does not synchronize the peer's socket.
        # Its unpaid turn holds time while this read-only snapshot catches up.
        current = self.request(peer, {"type": "snapshot"})
        self.assertIsNone(current["error"])
        self.assertTrue(current["state"]["observation"]["ready"])
        self.assertEqual(current["state"]["observation"]["tick"],
                         owner_frame["state"]["observation"]["tick"])
        waited = self.act(peer, {"type": "wait"})
        self.assertIsNone(waited["error"])
        return waited

    def test_owner_outgoing_policy_preserves_kernel_and_pinned_sequence_on_restart(self):
        fallback = creature_package(self, "outgoing-policy-fallback")
        for policy, expected_check, expected_damage in (("ordinary", -4, 0), ("ignore_negative", 4, 2)):
            with self.subTest(policy=policy):
                package = outgoing_edge_package(self, policy)
                self.save = self.directory / (policy + ".db")
                server = self.server(scenario=package, wizard=True)
                wizard = self.wizard()
                player, initial = self.client()
                peer, _ = self.client(actor=2)
                target = next(actor["id"] for actor in initial["state"]["observation"]["visible_actors"]
                              if actor["name"] == "Sequence sentinel")
                self._private_report(wizard, "combat capture on")
                preparing = self.act(player, {"type": "use_ability", "id": "triple_strike", "invocation": {"aim": {"kind": "actor_target", "target": target}, "weapon": {"kind": "default"}}})
                self.assertIsNone(preparing["error"])
                self.assertTrue(preparing["state"]["observation"]["combat"]["preparation_active"])
                self._outgoing_policy_peer_wait(peer, preparing)
                prefix = self.frame(player, lambda frame: (frame["state"]["observation"]["combat"]["sequence"] or {}).get("completed") == 1)
                first = self._private_report(wizard, "combat inspect")["records"]
                first = [record for record in first if record["ability"]["id"] == "triple_strike"]
                self.assertEqual(len(first), 1)
                self.assertIsNone(self.request(player, {"type": "save"})["error"])
                server.stop(); player.stop(); peer.stop(); wizard.stop()
                (package / "rules.toml").unlink()
                server = self.server(scenario=fallback, seed=None, wizard=True)
                wizard = self.wizard()
                player, restored = self.client()
                peer, _ = self.client(actor=2)
                self.assertEqual(restored["state"]["observation"]["combat"]["sequence"],
                                 prefix["state"]["observation"]["combat"]["sequence"])
                self._private_report(wizard, "combat capture on")
                self.assertIsNone(self.command(player, {"type": "resume_intention"})["error"])
                preparing = self.frame(player, lambda frame: frame.get("state") is not None
                                       and frame["state"]["observation"]["combat"]["preparation_active"])
                self._outgoing_policy_peer_wait(peer, preparing)
                second = self.frame(player, lambda frame: any(event["type"] == "ability" and event["strike"] == 1
                    for event in frame["state"]["observation"]["combat"]["events"]))
                self._outgoing_policy_peer_wait(peer, second)
                self.frame(player, lambda frame: frame["state"]["observation"]["combat"]["sequence"] is None
                           and not frame["intentions"])
                resumed = self._private_report(wizard, "combat inspect")["records"]
                resumed = [record for record in resumed if record["ability"]["id"] == "triple_strike"]
                self.assertEqual(len(resumed), 2, "restart resumes only the unexecuted tail")
                for record in first + resumed:
                    check = next(trace_fields(step) for step in record["trace"]["steps"] if step["id"] == "check")
                    self.assertEqual(check["input_edge"], expected_check)
                    self.assertEqual(check["shared_damage_edge"], expected_damage)
                    self.assertEqual(len(check["rolled"]), 6)
                    expected_kept = sorted(check["rolled"])[-3:] if expected_check > 0 else sorted(check["rolled"])[:3]
                    self.assertEqual(check["kept"], sum(expected_kept))
                    damage = next(trace_fields(step) for step in record["trace"]["steps"] if step["id"] == "damage.component")
                    # One base die accepts at most one edge from the shared damage budget.
                    component_edge = min(expected_damage, 1)
                    self.assertEqual(damage["edge"], component_edge)
                    self.assertEqual(len(damage["rolled"]), 1 + component_edge)
                    self.assertEqual(damage["kept"], [max(damage["rolled"])])
                    finished = next(trace_fields(step) for step in record["trace"]["steps"] if step["id"] == "damage.finished")
                    self.assertEqual(finished["unused_edge"], expected_damage - component_edge)
                self.assertNotIn('"type":"combat_diagnostics"', "".join(player.transcript).replace(" ", ""))
                server.stop(); player.stop(); peer.stop(); wizard.stop()

    def _private_report(self, client, operation, kind="combat_diagnostics"):
        import json
        start = len(client.transcript)
        ready = self.command(client, {"type": "wizard", "command": operation})
        self.assertFalse(ready.get("error"), ready.get("error"))
        messages = [json.loads(line).get("message")
                    for line in client.transcript[start:] if line.startswith("{")]
        reports = [message["report"] for message in messages if message and message["type"] == kind]
        self.assertEqual(len(reports), 1, "the command must return a fresh private report")
        if kind == "creature_inspection":
            report = reports[0]
            skills = {row["skill"]["id"]: row for row in report["stats"]["skills"]}
            owned = Counter()
            grants = list(report["starting_skills"])
            grants.extend(purchase["selection"]["skill"]
                          for die in report["hit_dice"] for purchase in die["purchases"]
                          if purchase["selection"]["kind"] == "skill_rank")
            for skill in grants:
                self.assertEqual(skills[skill["id"]]["skill"], skill)
                owned[skill["id"]] += 1
            for skill_id, row in skills.items():
                self.assertEqual(row["rank"], owned[skill_id])
                self.assertLessEqual(row["rank"], 5)
        return reports[0]

    def _private_strikes(self, client, report, source):
        """Reconstruct authored lists only in this test from real typed pages."""
        import json
        current = self.command(client, {"type": "inspect"})
        cursor = None
        entries = []
        for _ in range(657):
            start = len(client.transcript)
            current = self.request(client, {"type": "command", "context": current["input_context"],
                "branch": current["branch"], "command": {"type": "inspect_strike_source",
                    "expected_revision": current["state"]["revision"], "actor": report["actor"],
                    "target_revision": report["revision"], "source": source, "cursor": cursor}})
            self.assertFalse(current.get("error"), current.get("error"))
            messages = [json.loads(line).get("message") for line in client.transcript[start:]
                        if line.startswith("{")]
            pages = [message["page"] for message in messages
                     if message and message["type"] == "creature_strike_page"]
            self.assertEqual(len(pages), 1)
            page = pages[0]
            self.assertEqual(page["source"], source)
            self.assertEqual(page["actor"], report["actor"])
            self.assertEqual(page["target_revision"], report["revision"])
            entries.extend(page["entries"])
            cursor = page["next"]
            if cursor is None:
                break
        else:
            self.fail("authored detail traversal must terminate")
        strikes = []
        for entry in entries:
            kind = entry["kind"]
            if kind == "strike_header":
                self.assertEqual(entry["strike"], len(strikes))
                strikes.append({"ordinal": entry["strike"], "offset": entry["offset"],
                    "unlock": entry["unlock"], "check": entry["check"], "on_success": [],
                    "on_failure": [], "descriptors": [], "riders": [], "counts": entry})
                continue
            strike = strikes[entry["strike"]]
            if kind == "rider_header":
                self.assertEqual(entry["rider"], len(strike["riders"]))
                strike["riders"].append({"ordinal": entry["rider"], "trigger": entry["trigger"],
                    "condition": entry["condition"], "check": entry["check"],
                    "effects": [], "descriptors": [], "counts": entry})
            else:
                owner = strike["riders"][entry["rider"]] if kind.startswith("rider_") else strike
                field = {"success_effect": "on_success", "failure_effect": "on_failure",
                         "strike_descriptor": "descriptors", "rider_effect": "effects",
                         "rider_descriptor": "descriptors"}[kind]
                self.assertEqual(entry["index"], len(owner[field]))
                owner[field].append(entry.get("row", entry.get("descriptor")))
        for strike in strikes:
            counts = strike.pop("counts")
            for field, count in (("on_success", "success_count"), ("on_failure", "failure_count"),
                                 ("descriptors", "descriptor_count"), ("riders", "rider_count")):
                self.assertEqual(len(strike[field]), counts[count])
            for rider in strike["riders"]:
                counts = rider.pop("counts")
                self.assertEqual(len(rider["effects"]), counts["effect_count"])
                self.assertEqual(len(rider["descriptors"]), counts["descriptor_count"])
        return strikes

    def test_natural_melee_pages_show_species_and_added_riders_in_real_clients(self):
        import json
        package = natural_melee_page_package(self)
        self.server(scenario=package, wizard=True)
        observer, initial = self.client(observe=True)
        wizard, _ = self.client(WIZARD_TOKEN, observe=True)
        report = self._private_report(wizard, "creature inspect 2", "creature_inspection")
        summary = next(row for row in report["ability_strikes"] if row["ability"] == "triple_strike")
        self.assertEqual(summary["source"], {"kind": "natural_melee", "strike_count": 3, "additional_riders": 1})
        start = len(wizard.transcript)
        strikes = self._private_strikes(wizard, report, {"kind": "ability", "id": "triple_strike"})
        self.assertEqual([len(strike["riders"]) for strike in strikes], [3, 1, 1])
        for strike in strikes:
            self.assertEqual(trace_fields(strike["riders"][-1]["effects"][0])["amount"], "22")
            self.assertEqual(strike["riders"][-1]["condition"],
                             {"kind": "sequence_successful_hits", "minimum": 2})
        self.assertEqual([rider["condition"] for rider in strikes[0]["riders"][:-1]], [None, None])
        messages = [json.loads(line).get("message") for line in wizard.transcript[start:] if line.startswith("{")]
        pages = [message["page"] for message in messages if message and message["type"] == "creature_strike_page"]
        for page in pages:
            self.assertEqual(page["resolved"]["kind"], "natural_melee")
            self.assertEqual(page["resolved"]["natural_riders"], [2, 0, 0])
            self.assertEqual(page["resolved"]["additional_riders"], 1)
        text, _ = self.adventure(WIZARD_TOKEN)
        summary = self.say(text, "wizard creature inspect 2")
        self.assertIn("3 natural strikes", summary)
        output = [self.say(text, "strikes ability triple_strike")]
        for _ in range(len(pages) - 1):
            output.append(self.say(text, "strikes next"))
        shown = "\n".join(output)
        self.assertIn("Natural source:", shown)
        self.assertIn("species-authored 0", shown)
        self.assertIn("ability-added 0", shown)
        self.assertIn("Condition: at least 2 qualifying primary hits on the same target in this sequence; attempt once on a qualifying hit.", shown)
        self.assertIn("22", shown)
        after = self.request(observer, {"type": "snapshot"})
        self.assertEqual(after["state"], initial["state"])
        self.assertNotIn('"creature_strike_page"', "".join(observer.transcript))

    def test_ordered_strikes_checked_primary_discloses_both_rider_triggers_across_real_pages(self):
        import json
        package = mixed_trigger_page_package(self)
        self.server(scenario=package, wizard=True)
        observer, initial = self.client(observe=True)
        wizard, _ = self.client(WIZARD_TOKEN, observe=True)
        report = self._private_report(wizard, "creature inspect 2", "creature_inspection")
        start = len(wizard.transcript)
        strikes = self._private_strikes(wizard, report, {"kind": "natural"})
        self.assertEqual(strikes[0]["check"]["id"], "primary_check")
        self.assertEqual([trace_fields(row)["amount"] for row in strikes[0]["on_success"]],
                         [str(amount) for amount in range(1, 16)])
        self.assertEqual(trace_fields(strikes[0]["on_failure"][0])["primary_component_0"], "Impact: 33")
        riders = strikes[0]["riders"]
        self.assertEqual([rider["trigger"] for rider in riders], ["success", "failure"])
        self.assertEqual([len(rider["effects"]) for rider in riders], [14, 1])
        self.assertEqual([trace_fields(row)["amount"] for row in riders[0]["effects"]],
                         [str(amount) for amount in range(1, 15)])
        self.assertEqual(trace_fields(riders[1]["effects"][0])["primary_component_0"], "Impact: 77")
        messages = [json.loads(line).get("message") for line in wizard.transcript[start:] if line.startswith("{")]
        pages = [message["page"] for message in messages if message and message["type"] == "creature_strike_page"]
        primary_pages = [(entry["kind"], index) for index, page in enumerate(pages)
                         for entry in page["entries"] if entry["strike"] == 0 and entry["kind"] in ("success_effect", "failure_effect")]
        self.assertEqual(primary_pages, [("success_effect", 0)] * 15 + [("failure_effect", 1)])
        trigger_pages = [(entry["trigger"], index) for index, page in enumerate(pages)
                         for entry in page["entries"] if entry["kind"] == "rider_header"]
        self.assertEqual([trigger for trigger, _ in trigger_pages], ["success", "failure"])
        self.assertNotEqual(trigger_pages[0][1], trigger_pages[1][1])
        text, _ = self.adventure(WIZARD_TOKEN)
        self.say(text, "wizard creature inspect 2")
        output = self.say(text, "strikes natural")
        presented = [output]
        for _ in range(len(pages) - 1):
            output = self.say(text, "strikes next")
            self.assertIn("Strike source:", output)
            presented.append(output)
        combined = "\n".join(presented)
        self.assertIn("primary success", combined)
        self.assertIn("primary failure", combined)
        self.assertIn("Impact: 77", combined)
        after = self.request(observer, {"type": "snapshot"})
        self.assertEqual(after["state"], initial["state"])
        self.assertNotIn('"creature_strike_page"', "".join(observer.transcript))

    def test_ordered_strikes_adventure_interface_pages_successfully_without_script_mode(self):
        package = strike_page_package(self)
        self.server(scenario=package, wizard=True)
        observer, initial = self.client(observe=True)
        text, _ = self.adventure(WIZARD_TOKEN)
        summary = self.say(text, "wizard creature inspect 2")
        self.assertIn("3 authored strikes", summary)
        first = self.say(text, "strikes natural")
        self.assertIn("Strike source:", first)
        self.assertIn("Strike 1 (authored 0)", first)
        second = self.say(text, "strikes next")
        self.assertIn("Strike source:", second)
        self.assertNotEqual(first, second)
        previous = self.say(text, "strikes previous")
        self.assertEqual(previous, first)
        marker = self.say(text, "strikes ability basic_melee")
        self.assertIn("Uses selected melee", marker)
        self.assertIn("strikes selected", marker)
        selected = self.say(text, "strikes selected")
        self.assertIn("Strike source:", selected)
        self.assertNotIn("Uses selected melee", selected)
        for output in (summary, first, second, previous, marker, selected):
            self.assertNotIn("Server error", output)
            self.assertNotIn("Reopen the creature inspection", output)
        after = self.request(observer, {"type": "snapshot"})
        self.assertEqual(after["state"], initial["state"])
        self.assertNotIn('"creature_strike_page"', "".join(observer.transcript))

    def test_ordered_strikes_pages_use_real_headless_text_queries_and_explicit_restart(self):
        import json
        package = strike_page_package(self)
        self.server(scenario=package, wizard=True)
        player, ordinary = self.client(observe=True)
        wizard, current = self.client(WIZARD_TOKEN, observe=True)
        report = self._private_report(wizard, "creature inspect 2", "creature_inspection")
        self.assertNotIn("strikes", report["natural_attack"])
        self.assertEqual(report["natural_attack"]["strike_count"], 3)
        self.assertEqual([row["ability"] for row in report["ability_strikes"]],
                         content_ids(report["stats"]["abilities"]))

        def query(client, frame, source, cursor):
            start = len(client.transcript)
            result = self.request(client, {"type": "command", "context": frame["input_context"],
                "branch": frame["branch"], "command": {"type": "inspect_strike_source",
                    "expected_revision": frame["state"]["revision"], "actor": "2",
                    "target_revision": report["revision"], "source": source, "cursor": cursor}})
            messages = [json.loads(line).get("message") for line in client.transcript[start:]
                        if line.startswith("{")]
            pages = [message for message in messages if message and message["type"] == "creature_strike_page"]
            for page in pages:
                self.assertLessEqual(len(json.dumps(page, ensure_ascii=False, separators=(",", ":")).encode()), 196608)
            return result, pages

        source = {"kind": "natural"}
        denied, pages = query(player, ordinary, source, None)
        self.assertIsNotNone(denied["error"])
        self.assertFalse(pages)
        entries, cursor, first_cursor, starts = [], None, None, set()
        for _ in range(657):
            current, pages = query(wizard, current, source, cursor)
            self.assertFalse(current.get("error"), current.get("error"))
            self.assertEqual(len(pages), 1)
            page = pages[0]["page"]
            self.assertTrue(1 <= len(page["entries"]) <= 16)
            start = json.dumps(page["start"], sort_keys=True)
            self.assertNotIn(start, starts)
            starts.add(start)
            entries.extend(page["entries"])
            cursor = page["next"]
            if first_cursor is None:
                first_cursor = cursor
            if cursor is None:
                break
        else:
            self.fail("the bounded authored source must terminate")
        self.assertEqual(len(entries), 262)  # three headers, three effects, 256 descriptors
        descriptors = [row for row in entries if row["kind"] == "strike_descriptor"]
        self.assertEqual(len(descriptors), 256)
        self.assertEqual([row["index"] for row in descriptors], list(range(128)) * 2)
        self.assertEqual(current["state"], ordinary["state"])
        self.assertNotIn('"creature_strike_page"', "".join(player.transcript))
        reset = self.request(wizard, {"type": "snapshot"})
        stale, pages = query(wizard, reset, source, first_cursor)
        self.assertIsNotNone(stale["error"])
        self.assertFalse(pages)
        fresh, pages = query(wizard, stale, source, None)
        self.assertFalse(fresh.get("error"), fresh.get("error"))
        self.assertEqual(pages[0]["page"]["start"], {"kind": "strike_header", "strike": 0})

        text, _ = self.text_client(WIZARD_TOKEN, observe=True)
        summary = text.command("wizard creature inspect 2")
        self.assertIn("3 authored strikes", summary)
        first = text.command("strikes natural")
        self.assertIn("Strike source", first)
        second = text.command("strikes next")
        self.assertIn("Strike source", second)
        self.assertNotEqual(first, second)
        previous = text.command("strikes previous")
        self.assertEqual(first, previous)
        marker = text.command("strikes ability basic_melee")
        self.assertIn("Uses selected melee", marker)
        self.assertIn("strikes selected", marker)
        selected = text.command("strikes selected")
        self.assertIn("Strike source", selected)
        self.assertNotIn("Uses selected melee", selected)

    def test_braced_potion_discloses_disadvantage_on_an_enemy_physical_check(self):
        package = creature_package(self, "braced-edge", hostile=True, abilities=())
        manifest = package / "scenario.toml"
        source = manifest.read_text(encoding="utf-8")
        source = source.replace('archetypes = { ', 'archetypes = { brace = { class = "potion", name = "brace draught", consumable = { effects = [{ kind = "apply_condition", condition = "braced", duration = 2000 }] } }, ')
        manifest.write_text(source, encoding="utf-8", newline="\n")
        region = package / "regions/1.toml"
        with region.open("a", encoding="utf-8", newline="\n") as output:
            output.write('\n[[items]]\nid = 20\nat = [1, 1, 0]\ncarried_by = 1\narchetype = "brace"\n')
        result = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                capture_output=True, text=True, encoding="utf-8", timeout=15)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.server(scenario=package, wizard=True)
        wizard, _ = self.client(WIZARD_TOKEN, observe=True)
        player, initial = self.client()
        self._private_report(wizard, "combat capture on")
        potion = next(item["id"] for item in initial["state"]["observation"]["inventory"]
                      if item["name"] == "brace draught")
        self.assertIsNone(self.act(player, {"type": "drink", "item": potion})["error"])
        for _ in range(40):
            report = self._private_report(wizard, "combat inspect")
            checks = [trace_fields(step) for record in report["records"]
                      if record["actor"] == "2" and record["target"] == "1"
                      for step in record["trace"]["steps"] if step["id"] == "check"]
            if any(check["input_edge"] == -1 for check in checks):
                break
            self.assertIsNone(self.act(player, {"type": "wait"})["error"])
        else:
            self.fail("an enemy attack prepared after Braced must disclose disadvantage")
        check = next(check for check in checks if check["input_edge"] == -1)
        self.assertEqual(check["edge"], -1)
        self.assertNotIn('"type":"combat_diagnostics"', "".join(player.transcript).replace(" ", ""))

    def test_next_check_potions_are_reserved_privately_across_restart_and_cannot_be_reused(self):
        for token, expected in (("readied", 1), ("distracted", -1)):
            with self.subTest(token=token):
                package = creature_package(self, "next-check-" + token, abilities=())
                self.save = self.directory / (token + ".db")
                manifest = package / "scenario.toml"
                source = manifest.read_text(encoding="utf-8")
                potion = ('draught = { class = "potion", name = "check draught", consumable = '
                          '{ effects = [{ kind = "apply_condition", condition = "' + token + '", duration = 2000 }] } }, ')
                source = source.replace('archetypes = { ', 'archetypes = { ' + potion)
                manifest.write_text(source, encoding="utf-8", newline="\n")
                region = package / "regions/1.toml"
                source = region.read_text(encoding="utf-8")
                declaration = 'controller = "ai"\nai = "practice"'
                self.assertIn(declaration, source)
                source = source.replace(declaration, 'controller = "external"')
                source += '\n[[items]]\nid = 20\nat = [1, 1, 0]\ncarried_by = 1\narchetype = "draught"\n'
                region.write_text(source, encoding="utf-8", newline="\n")
                validated = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                           capture_output=True, text=True, encoding="utf-8", timeout=15)
                self.assertEqual(validated.returncode, 0, validated.stderr)
                server = self.server(scenario=package, wizard=True)
                wizard, _ = self.client(WIZARD_TOKEN, observe=True)
                player, initial = self.client()
                peer, _ = self.client(WIZARD_TOKEN, actor=2)
                target = next(actor["id"] for actor in initial["state"]["observation"]["visible_actors"]
                              if actor["name"] == "practice target")
                self._private_report(wizard, "combat capture on")
                # Queue the peer's ordinary turn so drinking can finish, then its
                # next input boundary holds the player's attack in preparation.
                self.assertIsNone(self.command(peer, {"type": "act", "action": {"type": "wait"}})["error"])
                item = next(item["id"] for item in initial["state"]["observation"]["inventory"]
                            if item["name"] == "check draught")
                self.assertIsNone(self.act(player, {"type": "drink", "item": item})["error"])

                def completed(frame):
                    return (frame.get("state") is not None
                            and frame["state"]["observation"]["ready"]
                            and frame["state"]["observation"]["combat"]["preparation_remaining"] is None)

                self.frame(player, completed)
                report = self._private_report(wizard, "creature inspect 1", "creature_inspection")
                self.assertEqual([entry["condition"]["id"] for entry in report["conditions"]], [token])
                started = self.act(player, {"type": "use_ability", "id": "basic_melee", "invocation": {"aim": {"kind": "actor_target", "target": target}, "weapon": {"kind": "default"}}})
                self.assertIsNone(started["error"])
                self.assertTrue(started["state"]["observation"]["combat"]["preparation_active"])
                identity = started["intentions"][0]["intention"]
                remaining = started["state"]["observation"]["combat"]["preparation_remaining"]
                self.assertEqual(self._private_report(wizard, "creature inspect 1", "creature_inspection")["conditions"], [])
                self.assertIsNone(self.request(player, {"type": "save"})["error"])
                server.stop()
                player.stop()
                peer.stop()
                wizard.stop()
                server = self.server(scenario=package, wizard=True)
                wizard, _ = self.client(WIZARD_TOKEN, observe=True)
                player, restored = self.client()
                peer, _ = self.client(WIZARD_TOKEN, actor=2)
                self._private_report(wizard, "combat capture on")
                self.assertEqual(restored["intentions"][0]["intention"], identity)
                self.assertEqual(restored["intentions"][0]["phase"], "paused")
                self.assertEqual(restored["state"]["observation"]["combat"]["preparation_remaining"], remaining)
                self.assertEqual(self._private_report(wizard, "creature inspect 1", "creature_inspection")["conditions"], [])
                resumed = self.command(player, {"type": "resume_intention"})
                self.assertIsNone(resumed["error"])
                preparing = self.frame(player, lambda frame: frame.get("state") is not None
                                       and frame["state"]["observation"]["combat"]["preparation_active"])
                # The owner's socket cannot synchronize the peer's observation.
                # Refresh before acting, while its unpaid turn still holds time.
                peer_ready = self.request(peer, {"type": "snapshot"})
                self.assertIsNone(peer_ready["error"])
                self.assertTrue(peer_ready["state"]["observation"]["ready"])
                self.assertEqual(peer_ready["state"]["observation"]["tick"],
                                 preparing["state"]["observation"]["tick"])
                self.assertIsNone(self.act(peer, {"type": "wait"})["error"])
                self.frame(player, completed)
                preparing = self.act(player, {"type": "use_ability", "id": "basic_melee", "invocation": {"aim": {"kind": "actor_target", "target": target}, "weapon": {"kind": "default"}}})
                self.assertIsNone(preparing["error"])
                self.assertTrue(preparing["state"]["observation"]["combat"]["preparation_active"])
                peer_ready = self.request(peer, {"type": "snapshot"})
                self.assertIsNone(peer_ready["error"])
                self.assertTrue(peer_ready["state"]["observation"]["ready"])
                self.assertEqual(peer_ready["state"]["observation"]["tick"],
                                 preparing["state"]["observation"]["tick"])
                self.assertIsNone(self.act(peer, {"type": "wait"})["error"])
                self.frame(player, completed)
                records = self._private_report(wizard, "combat inspect")["records"]
                checks = [trace_fields(step) for record in records
                          if record["actor"] == "1" and record["ability"] and record["ability"]["id"] == "basic_melee"
                          for step in record["trace"]["steps"] if step["id"] == "check"]
                self.assertEqual([check["input_edge"] for check in checks], [expected, 0])
                self.assertEqual([check["edge"] for check in checks], [expected, 0])
                self.assertEqual([len(check["rolled"]) for check in checks], [4, 3])
                self.assertEqual([check["shared_damage_edge"] for check in checks], [0, 0])
                retained = sorted(checks[0]["rolled"], reverse=expected > 0)[:3]
                self.assertEqual(checks[0]["retained"], retained)
                self.assertEqual(checks[0]["kept"], sum(retained))
                self.assertEqual(self._private_report(wizard, "creature inspect 1", "creature_inspection")["conditions"], [])
                for client in (player, peer, wizard):
                    self.assertNotIn('"reserved_by"', "".join(client.transcript))
                self.assertNotIn('"type":"combat_diagnostics"', "".join(player.transcript).replace(" ", ""))
                player.stop()
                peer.stop()
                wizard.stop()
                server.stop()

    def test_fear_immunity_and_resistance_keep_rng_and_payment_contracts(self):
        for condition in ("fear", "mind_affecting", "resistance"):
            with self.subTest(condition=condition):
                package = creature_package(self, "fear-" + condition, abilities=("fixture_focus_fear",))
                self.save = self.directory / (condition + ".db")
                manifest = package / "scenario.toml"
                source = manifest.read_text(encoding="utf-8")
                if condition == "resistance":
                    grant = '{ scope = "always", op = { kind = "add", stat = { kind = "ability", ability = "fixture_focus_fear", stat = "check_bonus" }, value = -1000 } }'
                else:
                    grant = ('{ scope = "always", op = { kind = "immunity", selector = { kind = "descriptor", descriptor = "'
                             + condition + '" } } }')
                source = source.replace('modifiers = [', 'modifiers = [' + grant + ', ', 1)
                manifest.write_text(source, encoding="utf-8", newline="\n")
                result = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                        capture_output=True, text=True, encoding="utf-8", timeout=15)
                self.assertEqual(result.returncode, 0, result.stderr)
                server = self.server(scenario=package, wizard=True)
                wizard, _ = self.client(WIZARD_TOKEN, observe=True)
                player, initial = self.client()

                def query(operation):
                    return self._private_report(wizard, operation)

                query("combat capture on")
                target = next(actor["id"] for actor in initial["state"]["observation"]["visible_actors"]
                              if actor["name"] == "practice target")
                started = self.act(player, {"type": "use_ability", "id": "fixture_focus_fear", "invocation": {"aim": {"kind": "actor_target", "target": target}, "weapon": {"kind": "default"}}})
                self.assertIsNone(started["error"])
                self.assertEqual(resource(started, "focus")["balance"],
                                 resource(initial, "focus")["balance"] - 1)
                self.assertEqual(resource(started, "focus")["reserved"], 1)
                completed = self.frame(player, lambda frame: frame.get("state") is not None
                                       and frame["state"]["observation"]["ready"]
                                       and int(frame["state"]["observation"]["tick"]) > 0
                                       and frame["state"]["observation"]["combat"]["preparation_remaining"] is None)
                elapsed = int(completed["state"]["observation"]["tick"])
                self.assertEqual(resource(completed, "focus")["reserved"], 0)
                self.assertEqual(resource(completed, "focus")["balance"],
                                 resource(initial, "focus")["balance"] - 2 + elapsed // 300)
                records = query("combat inspect")["records"]
                record = next(record for record in records if record["ability"]["id"] == "fixture_focus_fear")
                self.assertFalse((record["outcome"]["result"] == "applied"))
                self.assertEqual(damage_total(record), 0)
                self.assertEqual(record["charge"], {"stamina": None, "mana": None,
                    "focus": {"resource": {"id": "focus", "name": "Focus"}, "start": 1, "resolution": 1}})
                self.assertEqual(record["target_before"]["conditions"], [])
                self.assertEqual(record["target_after"]["conditions"], [])
                self.assertEqual(u64(record["target_after"]["health"]), u64(record["target_before"]["health"]))
                steps = record["trace"]["steps"]
                checks = [trace_fields(step) for step in steps if step["id"] == "check"]
                if condition == "resistance":
                    self.assertEqual(len(checks), 1)
                    self.assertFalse(checks[0]["success"])
                    self.assertEqual(len(checks[0]["rolled"]), 3)
                    self.assertNotEqual(checks[0]["rng_before"], checks[0]["rng_after"])
                    self.assertNotIn("effect.immune", [step["id"] for step in steps])
                else:
                    self.assertEqual(checks, [], "immunity must not draw resistance dice")
                    self.assertEqual([step["id"] for step in steps], ["effect.immune"])
                self.assertEqual(record["outcome"], {"result": "miss" if condition == "resistance" else "unaffected", "effects": []})
                self.assertNotIn('"type":"combat_diagnostics"', "".join(player.transcript).replace(" ", ""))
                player.stop()
                wizard.stop()
                server.stop()

    def test_fear_disadvantage_survives_restart_and_immunity_clears_it(self):
        package = creature_package(self, "fear-edge", hostile=True, abilities=("fixture_focus_fear",))
        manifest = package / "scenario.toml"
        source = manifest.read_text(encoding="utf-8")
        source = source.replace('modifiers = [', 'modifiers = [{ scope = "always", op = { kind = "add", stat = { kind = "ability", ability = "fixture_focus_fear", stat = "check_bonus" }, value = 1000 } }, ', 1)
        source += ('\n[creatures.species.fighter]\nbehavior = { disposition = "mean", idle = "explore", memory_ticks = 1000 }\nkind = "humanoid"\nattributes = { strength = 0, speed = 0, intellect = 0, willpower = 0, awareness = 0, presence = 0 }\n' + natural_attack_source([{"category": "impact", "amount": {"kind": "fixed", "amount": 4}}], bonus=100, wind_up=150) + '\nmodifiers = [{ scope = "always", op = { kind = "add", stat = { kind = "health" }, value = 200 } }]\n\n[creatures.templates.fearless]\npriority = 0\nmodifiers = [{ scope = "always", op = { kind = "immunity", selector = { descriptor = "fear", kind = "descriptor" } } }]\n')
        manifest.write_text(source, encoding="utf-8", newline="\n")
        region = package / "regions/1.toml"
        source = region.read_text(encoding="utf-8")
        region.write_text(source.replace('species = "human"', 'species = "fighter"', 1),
                          encoding="utf-8", newline="\n")
        result = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                capture_output=True, text=True, encoding="utf-8", timeout=15)
        self.assertEqual(result.returncode, 0, result.stderr)
        server = self.server(scenario=package, wizard=True)
        wizard, _ = self.client(WIZARD_TOKEN, observe=True)
        player, initial = self.client()

        def query(operation, kind="combat_diagnostics"):
            return self._private_report(wizard, operation, kind)

        query("combat capture on")
        target = next(actor["id"] for actor in initial["state"]["observation"]["visible_actors"]
                      if actor["name"] == "practice target")
        started = self.act(player, {"type": "use_ability", "id": "fixture_focus_fear", "invocation": {"aim": {"kind": "actor_target", "target": target}, "weapon": {"kind": "default"}}})
        self.assertIsNone(started["error"])
        completed = self.frame(player, lambda frame: frame.get("state") is not None
                               and frame["state"]["observation"]["ready"]
                               and int(frame["state"]["observation"]["tick"]) > 0
                               and frame["state"]["observation"]["combat"]["preparation_remaining"] is None)
        records = query("combat inspect")["records"]
        fear = next(record for record in records if record["ability"]["id"] == "fixture_focus_fear")
        self.assertTrue((fear["outcome"]["result"] == "applied"))
        self.assertEqual(fear["target_after"]["conditions"][0]["source"], "1")
        attack = None
        for _ in range(40):
            candidates = [record for record in records if record["actor"] == "2" and record["target"] == "1"
                          and any(step["id"] == "check" and trace_fields(step)["input_edge"] == -1
                                  for step in record["trace"]["steps"])]
            if candidates:
                attack = candidates[0]
                break
            completed = self.act(player, {"type": "wait"})
            self.assertIsNone(completed["error"])
            records = query("combat inspect")["records"]
        self.assertIsNotNone(attack, "capture an attack planned while the condition is active")
        self.assertEqual(attack["actor_before"]["conditions"][0]["source"], "1")
        check = next(trace_fields(step) for step in attack["trace"]["steps"] if step["id"] == "check")
        self.assertEqual((check["input_edge"], check["edge"], check["unused_edge"]), (-1, -1, 0))
        self.assertEqual(len(check["rolled"]), 4)
        self.assertEqual(check["kept"], sum(sorted(check["rolled"])[:3]))
        self.assertEqual(damage_total(attack), 4)
        original = query("creature inspect 2", "creature_inspection")
        self.assertEqual(original["conditions"][0]["source"], "1")
        self.assertGreater(int(original["conditions"][0]["remaining_ticks"]), 0)
        self.assertEqual(original["tick"], completed["state"]["observation"]["tick"])
        self.assertIsNone(self.request(player, {"type": "save"})["error"])
        player.stop()
        wizard.stop()
        server.stop()
        self.server(scenario=package, wizard=True, seed=None)
        wizard, _ = self.client(WIZARD_TOKEN, observe=True)
        player, _ = self.client()
        self.assertEqual(query("creature inspect 2", "creature_inspection"), original)
        self.assertFalse(query("combat inspect")["enabled"])
        changed = self.command(wizard, {"type": "wizard", "command": "creature template 2 fearless on"})
        self.assertFalse(changed.get("error"), changed.get("error"))
        immune = query("creature inspect 2", "creature_inspection")
        self.assertEqual(immune["conditions"], [])
        for field in ("tick", "hp", "max_hp", "injury", "hit_dice"):
            self.assertEqual(immune[field], original[field])
        changed = self.command(wizard, {"type": "wizard", "command": "creature template 2 fearless off"})
        self.assertFalse(changed.get("error"), changed.get("error"))
        self.assertEqual(query("creature inspect 2", "creature_inspection")["conditions"], [])
        self.assertNotIn('"type":"combat_diagnostics"', "".join(player.transcript).replace(" ", ""))

    def test_fear_refresh_does_not_stack_and_expires_with_ordinary_waits(self):
        package = creature_package(self, "fear-refresh", abilities=("fixture_focus_fear",))
        manifest = package / "scenario.toml"
        source = manifest.read_text(encoding="utf-8")
        # This probe isolates refresh and expiry. A target with engagement
        # memory could legitimately retaliate and fear the caster in between.
        behavior = 'disposition = "mean", idle = "explore", memory_ticks = 1000'
        self.assertEqual(source.count(behavior), 1)
        source = source.replace(behavior, 'disposition = "passive", idle = "wait", memory_ticks = 0', 1)
        source = source.replace('modifiers = [', 'modifiers = [{ scope = "always", op = { kind = "add", stat = { kind = "ability", ability = "fixture_focus_fear", stat = "check_bonus" }, value = 1000 } }, ', 1)
        manifest.write_text(source, encoding="utf-8", newline="\n")
        result = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                capture_output=True, text=True, encoding="utf-8", timeout=15)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.server(scenario=package, wizard=True)
        wizard, _ = self.client(WIZARD_TOKEN, observe=True)
        player, initial = self.client()

        def query(operation, kind="combat_diagnostics"):
            return self._private_report(wizard, operation, kind)

        query("combat capture on")
        target = next(actor["id"] for actor in initial["state"]["observation"]["visible_actors"]
                      if actor["name"] == "practice target")
        for _ in range(2):
            started = self.act(player, {"type": "use_ability", "id": "fixture_focus_fear", "invocation": {"aim": {"kind": "actor_target", "target": target}, "weapon": {"kind": "default"}}})
            self.assertIsNone(started["error"])
            self.frame(player, lambda frame: frame.get("state") is not None
                       and frame["state"]["observation"]["ready"]
                       and int(frame["state"]["observation"]["tick"]) > 0
                       and frame["state"]["observation"]["combat"]["preparation_remaining"] is None)
        records = [record for record in query("combat inspect")["records"]
                   if record["actor"] == "1" and record["target"] == "2"
                   and record["ability"]["id"] == "fixture_focus_fear"]
        self.assertEqual(len(records), 2)
        first, second = records
        self.assertTrue((first["outcome"]["result"] == "applied"))
        self.assertTrue((second["outcome"]["result"] == "applied"))
        first_check = next(trace_fields(step) for step in first["trace"]["steps"] if step["id"] == "check")
        second_check = next(trace_fields(step) for step in second["trace"]["steps"] if step["id"] == "check")
        self.assertEqual(first_check["input_edge"], 0)
        self.assertEqual(len(first_check["rolled"]), 3)
        # Fear now checks the caster against Spiritual defense; the feared target
        # does not roll, so its source-scoped disadvantage cannot affect this check.
        self.assertEqual((second_check["input_edge"], second_check["edge"]), (0, 0))
        self.assertEqual(len(second_check["rolled"]), 3)
        self.assertEqual(second_check["kept"], sum(second_check["rolled"]))
        self.assertEqual(first["target_before"]["conditions"], [])
        self.assertEqual(len(second["target_before"]["conditions"]), 1)
        previous = int(second["target_before"]["conditions"][0]["remaining_ticks"])
        duration = int(first["target_after"]["conditions"][0]["remaining_ticks"])
        self.assertGreater(previous, 0)
        self.assertLess(previous, duration)
        self.assertEqual(second["target_after"]["conditions"], first["target_after"]["conditions"])
        affected = query("creature inspect 2", "creature_inspection")
        deadline = int(affected["tick"]) + int(affected["conditions"][0]["remaining_ticks"])
        for _ in range(8):
            if int(affected["tick"]) >= deadline:
                break
            waited = self.act(player, {"type": "wait"})
            self.assertIsNone(waited["error"])
            affected = query("creature inspect 2", "creature_inspection")
            if int(affected["tick"]) < deadline:
                self.assertEqual(affected["conditions"], [{"condition": {"id": "fear", "name": "Fear"}, "phase": "active", "source": "1", "remaining_ticks":
                                                   str(deadline - int(affected["tick"]))}])
        # The expiry settles at its deadline; an ordinary wait returns at its
        # own input boundary, which can be later than that internal event.
        self.assertGreaterEqual(int(affected["tick"]), deadline)
        self.assertLess(int(affected["tick"]), deadline + 100)
        self.assertEqual(affected["conditions"], [])
        self.assertEqual(u64(affected["hp"]), u64(affected["max_hp"]))

    def test_natural_three_and_eighteen_do_not_override_attack_totals(self):
        # These fixed seeds yield first combat 3d6 totals of 3 and 18.
        # Fixed damage and independent Health streams consume no combat draws before it.
        for seed, bonus, expected_die, hit in ((223, 100, 3, True), (921, -100, 18, False)):
            with self.subTest(seed=seed, expected_die=expected_die):
                package = creature_package(self, "natural-roll-" + str(seed), abilities=("fixture_stamina_strike",))
                self.save = self.directory / ("natural-roll-" + str(seed) + ".db")
                manifest = package / "scenario.toml"
                source = manifest.read_text(encoding="utf-8")
                attack = natural_attack_source([{"category": "impact", "amount": {"kind": "fixed", "amount": 1}}], bonus=bonus)
                source = "\n".join(attack if line.startswith("melee = ") else line
                                    for line in source.splitlines()) + "\n"
                manifest.write_text(source, encoding="utf-8", newline="\n")
                region = package / "regions/1.toml"
                source = region.read_text(encoding="utf-8").replace('name = "practice target",',
                         'name = "practice target", attributes = { strength = 5, speed = 4, '
                         'intellect = 3, willpower = 2, awareness = 1, presence = 0 },', 1)
                region.write_text(source, encoding="utf-8", newline="\n")
                result = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                        capture_output=True, text=True, encoding="utf-8", timeout=15)
                self.assertEqual(result.returncode, 0, result.stderr)
                server = self.server(scenario=package, wizard=True, seed=seed)
                wizard, _ = self.client(WIZARD_TOKEN, observe=True)
                player, initial = self.client()
                before = self._private_report(wizard, "creature inspect 2", "creature_inspection")
                self.assertEqual(before["stats"]["defenses"], {"physical": 19, "cognitive": 15, "spiritual": 11})
                self._private_report(wizard, "combat capture on")
                target = next(actor["id"] for actor in initial["state"]["observation"]["visible_actors"]
                              if actor["name"] == "practice target")
                self.assertIsNone(self.act(player, {"type": "use_ability", "id": "basic_melee", "invocation": {"aim": {"kind": "actor_target", "target": target}, "weapon": {"kind": "default"}}})["error"])
                self.frame(player, lambda frame: frame.get("state") is not None
                           and frame["state"]["observation"]["ready"]
                           and int(frame["state"]["observation"]["tick"]) > 0
                           and frame["state"]["observation"]["combat"]["preparation_remaining"] is None)
                records = self._private_report(wizard, "combat inspect")["records"]
                self.assertEqual(len(records), 1)
                record = records[0]
                checks = [trace_fields(step) for step in record["trace"]["steps"] if step["id"] == "check"]
                self.assertEqual(len(checks), 1)
                check = checks[0]
                self.assertEqual((check["rolled"], check["kept"]), ([expected_die // 3] * 3, expected_die))
                self.assertEqual((check["threshold"], check["bonus"]), (19, bonus + 3))
                self.assertEqual(int(check["total"]), expected_die + check["bonus"])
                self.assertEqual(check["success"], int(check["total"]) >= check["threshold"])
                self.assertEqual((check["success"], (record["outcome"]["result"] == "applied"), damage_total(record)), (hit, hit, int(hit)))
                self.assertEqual(u64(record["target_before"]["health"]) - u64(record["target_after"]["health"]), int(hit))
                components = [step for step in record["trace"]["steps"] if step["id"] == "damage.component"]
                self.assertEqual(len(components), int(hit), "misses skip the entire damage bundle")
                self.assertIsNone(record["charge"])
                player.stop()
                wizard.stop()
                server.stop()

    def test_resource_recovery_and_fear_pause_while_frozen_or_detached(self):
        from ruleset_sources import load_rules
        for destination in (3, 4):
            with self.subTest(destination=destination):
                package = creature_package(self, "timer-streaming-" + str(destination), abilities=("fixture_focus_fear",))
                recovery_periods = {definition["resource"]: definition["recovery_period"]
                                    for definition in load_rules(package / "rules.toml", self.bin / ("tor-scenario" + self.suffix))["resources"]}
                self.save = self.directory / ("timer-" + str(destination) + ".db")
                manifest = package / "scenario.toml"
                source = manifest.read_text(encoding="utf-8")
                # Keep the timer subject idle: a retaliatory paid preparation
                # would spend Focus and add unrelated active-time commitments.
                behavior = 'disposition = "mean", idle = "explore", memory_ticks = 1000'
                self.assertEqual(source.count(behavior), 1)
                source = source.replace(behavior, 'disposition = "passive", idle = "wait", memory_ticks = 0', 1)
                source = source.replace('modifiers = [', 'modifiers = [{ scope = "always", op = { kind = "add", stat = { kind = "ability", ability = "fixture_focus_fear", stat = "check_bonus" }, value = 1000 } }, ', 1)
                source += ('\n[creatures.templates.capacity]\npriority = 0\nmodifiers = [{ scope = "always", op = { kind = "add", stat = { kind = "resource_max", resource = "stamina" }, value = 2 } }, { scope = "always", op = { kind = "add", stat = { kind = "resource_max", resource = "focus" }, value = 2 } }, { scope = "always", op = { kind = "add", stat = { kind = "resource_max", resource = "mana" }, value = 2 } }]\n')
                manifest.write_text(source, encoding="utf-8", newline="\n")
                # Long authored rooms prevent sight/reach pins from keeping the subject active.
                # Default radii retain hall 1 frozen from hall 3, and detach it from hall 4.
                gallery = package / "regions/2.toml"
                text = gallery.read_text(encoding="utf-8").replace("size = [5, 3, 2]", "size = [32, 3, 2]")
                text = text.replace('anchors = { "landing" = [0, 1, 0] }',
                                    'anchors = { "landing" = [0, 1, 0], "exit" = [31, 1, 0] }')
                text = text.replace('portals = [', 'portals = [{ at = [31, 1, 0], direction = "east", '
                                    'to = "3/landing", height = 2 }, ', 1)
                gallery.write_text(text, encoding="utf-8", newline="\n")
                for region in (3, 4):
                    portals = ('{ at = [0, 1, 0], direction = "west", to = "'
                               + str(region - 1) + '/exit", height = 2 }')
                    if region == 3:
                        portals += ', { at = [31, 1, 0], direction = "east", to = "4/landing", height = 2 }'
                    text = (f'id = {region}\nname = "timer room {region}"\nsize = [32, 3, 2]\n'
                            'chamber = true\ngravity = [0, 0, -1]\n'
                            'anchors = { landing = [0, 1, 0], exit = [31, 1, 0] }\n'
                            'portals = [' + portals + ']\n')
                    (package / f"regions/{region}.toml").write_text(text, encoding="utf-8", newline="\n")
                result = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                        capture_output=True, text=True, encoding="utf-8", timeout=15)
                self.assertEqual(result.returncode, 0, result.stderr)
                server = self.server(scenario=package, wizard=True)
                wizard, _ = self.client(WIZARD_TOKEN, observe=True)
                player, _ = self.client()

                def operation(command):
                    ready = self.command(wizard, {"type": "wizard", "command": command})
                    self.assertFalse(ready.get("error"), ready.get("error"))
                    return ready

                def inspect():
                    return self._private_report(wizard, "creature inspect 2", "creature_inspection")

                snapshot = self.request(player, {"type": "snapshot"})
                target = next(actor["id"] for actor in snapshot["state"]["observation"]["visible_actors"]
                              if actor["name"] == "practice target")
                self.assertIsNone(self.act(player, {"type": "use_ability", "id": "fixture_focus_fear", "invocation": {"aim": {"kind": "actor_target", "target": target}, "weapon": {"kind": "default"}}})["error"])
                self.frame(player, lambda frame: frame.get("state") is not None
                           and frame["state"]["observation"]["ready"]
                           and int(frame["state"]["observation"]["tick"]) > 0
                           and frame["state"]["observation"]["combat"]["preparation_remaining"] is None)
                operation("creature template 2 capacity on")
                before = inspect()
                self.assertEqual(len(before["conditions"]), 1)
                pools = {pool["resource"]["id"]: pool for pool in before["stats"]["resources"]}
                self.assertEqual(set(pools), {"stamina", "focus", "mana"})
                self.assertTrue(all(pool["maximum"] - pool["balance"] == 2 for pool in pools.values()))
                self.assertTrue(all(pool["reserved"] == 0 for pool in pools.values()))
                operation(f"teleport 1 {destination} 16 1 0")
                for restart in (False, True):
                    for _ in range(20):
                        waited = self.act(player, {"type": "wait"})
                        self.assertIsNone(waited["error"])
                    self.assertGreaterEqual(int(waited["state"]["observation"]["tick"]) - int(before["tick"]), 2000)
                    if destination == 3:
                        frozen = inspect()
                        for field in ("conditions", "hp", "injury", "stats"):
                            self.assertEqual(frozen[field], before[field], field)
                    else:
                        unavailable = self.command(wizard, {"type": "wizard", "command": "creature inspect 2"})
                        self.assertIn("InvalidAction", unavailable["error"])
                        self.assertIn("unavailable", unavailable["error"])
                    if not restart:
                        self.assertIsNone(self.request(wizard, {"type": "save"})["error"])
                        player.stop()
                        wizard.stop()
                        server.stop()
                        server = self.server(scenario=package, wizard=True, seed=None)
                        wizard, _ = self.client(WIZARD_TOKEN, observe=True)
                        player, _ = self.client()
                operation("teleport 1 1 1 1 0")
                thawed = inspect()
                for field in ("conditions", "hp", "injury", "stats"):
                    self.assertEqual(thawed[field], before[field], field)
                thaw_tick = int(thawed["tick"])
                for _ in range(20):
                    self.assertIsNone(self.act(player, {"type": "wait"})["error"])
                    active = inspect()
                    elapsed = int(active["tick"]) - thaw_tick
                    for pool in active["stats"]["resources"]:
                        original = pools[pool["resource"]["id"]]
                        period = recovery_periods[pool["resource"]["id"]]
                        self.assertEqual(pool["balance"], min(original["maximum"], original["balance"] + elapsed // period),
                                         f'{pool["resource"]["id"]} recovery after {elapsed} active ticks')
                        self.assertEqual((pool["reserved"], pool["available"]), (0, pool["balance"]))
                    remaining = int(before["conditions"][0]["remaining_ticks"]) - elapsed
                    self.assertEqual(active["conditions"], [] if remaining <= 0 else
                                     [{"condition": {"id": "fear", "name": "Fear"}, "phase": "active", "source": "1", "remaining_ticks": str(remaining)}])
                self.assertGreaterEqual(elapsed, 2000)
                self.assertEqual(active["conditions"], [])
                self.assertTrue(all(pool["balance"] == pool["maximum"] for pool in active["stats"]["resources"]))
                player.stop()
                wizard.stop()
                server.stop()

    def test_mixed_damage_protection_composes_once_and_survives_restart(self):
        cases = [
            ("reduction", [], (18, 15, 8)),
            ("cold-immune", [('descriptor', 'cold')], (12, 9, 3)),
            ("energy-immune", [('category', 'energy')], (4, 4, 3)),
            ("all-immune", [('category', 'energy'), ('category', 'keen')], (0, 0, 0)),
        ]
        attack = natural_attack_source([
            {"category": "energy", "descriptor": "fire", "amount": {"kind": "fixed", "amount": 4}},
            {"category": "keen", "amount": {"kind": "fixed", "amount": 4}},
            {"category": "energy", "descriptor": "cold", "amount": {"kind": "fixed", "amount": 6}},
            {"category": "energy", "descriptor": "fire", "amount": {"kind": "fixed", "amount": 4}},
        ], bonus=100)
        for name, immunities, totals in cases:
            with self.subTest(protection=name):
                package = creature_package(self, "mixed-" + name, abilities=("fixture_stamina_strike",))
                self.save = self.directory / (name + ".db")
                manifest = package / "scenario.toml"
                source = manifest.read_text(encoding="utf-8")
                source = "\n".join(attack if line.startswith("melee = ") else line
                                    for line in source.splitlines()) + "\n"
                source = source.replace('archetypes = { ', 'archetypes = { "warded_mail" = { class = "armor", name = "warded mail", equipment = { slot = "body_armor", modifiers = [{ scope = "always", op = { kind = "add", stat = { kind = "defense", defense = "physical" }, value = 0 } }, { scope = "always", op = { kind = "reduction", selector = { kind = "category", category = "energy" }, amount = 2 } }] } }, ', 1)
                grants = [
                    '{ scope = "always", op = { kind = "add", stat = { kind = "health" }, value = 200 } }',
                    '{ scope = "always", op = { kind = "reduction", selector = { descriptor = "fire", kind = "descriptor" }, amount = 3 } }',
                    '{ scope = "always", op = { kind = "reduction", selector = { category = "energy", kind = "category" }, amount = 4 } }',
                    '{ scope = "always", op = { kind = "reduction", selector = { category = "keen", kind = "category" }, amount = 1 } }',
                ]
                grants += ('{ scope = "always", op = { kind = "immunity", selector = { kind = "' + kind + '", '
                           + kind + ' = "' + value + '" } } }' for kind, value in immunities)
                source += ('\n[creatures.species.defender]\nbehavior = { disposition = "mean", idle = "explore", memory_ticks = 1000 }\nkind = "humanoid"\nattributes = { strength = 0, speed = 0, intellect = 0, willpower = 0, awareness = 0, presence = 0 }\n' + natural_attack_source([{"category": "impact", "amount": {"kind": "fixed", "amount": 4}}]) + '\nmodifiers = [' + ", ".join(grants) + ']\n')
                manifest.write_text(source, encoding="utf-8", newline="\n")
                region = package / "regions/1.toml"
                source = region.read_text(encoding="utf-8").replace('species = "human"', 'species = "defender"', 1)
                source = source.replace('\n[[actors]]', '\nitems = [{ id = 30, at = [2,1,0], '
                                        'archetype = "warded_mail", carried_by = 2, equipped_slot = 2 }]\n\n[[actors]]', 1)
                region.write_text(source, encoding="utf-8", newline="\n")
                result = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                        capture_output=True, text=True, encoding="utf-8", timeout=15)
                self.assertEqual(result.returncode, 0, result.stderr)
                server = self.server(scenario=package, wizard=True)
                wizard, _ = self.client(WIZARD_TOKEN, observe=True)
                player, initial = self.client()
                summaries = []
                previous_health = None
                for continuation in range(2):
                    if continuation:
                        self.assertIsNone(self.request(player, {"type": "save"})["error"])
                        player.stop()
                        wizard.stop()
                        server.stop()
                        server = self.server(scenario=package, wizard=True, seed=None)
                        wizard, _ = self.client(WIZARD_TOKEN, observe=True)
                        player, initial = self.client()
                    self._private_report(wizard, "combat capture on")
                    target = next(actor["id"] for actor in initial["state"]["observation"]["visible_actors"]
                                  if actor["name"] == "practice target")
                    started = self.act(player, {"type": "use_ability", "id": "basic_melee", "invocation": {"aim": {"kind": "actor_target", "target": target}, "weapon": {"kind": "default"}}})
                    self.assertIsNone(started["error"])
                    self.frame(player, lambda frame: frame.get("state") is not None
                               and frame["state"]["observation"]["ready"]
                               and int(frame["state"]["observation"]["tick"]) > 0
                               and frame["state"]["observation"]["combat"]["preparation_remaining"] is None)
                    records = self._private_report(wizard, "combat inspect")["records"]
                    self.assertEqual(len(records), 1)
                    record = records[0]
                    self.assertEqual(record["outcome"]["result"], "applied" if totals[2] else "unaffected")
                    self.assertEqual(damage_total(record), totals[2])
                    self.assertIsNone(record["charge"])
                    if previous_health is not None:
                        self.assertEqual(u64(record["target_before"]["health"]), previous_health)
                    self.assertEqual(u64(record["target_before"]["health"]) - u64(record["target_after"]["health"]), totals[2])
                    previous_health = u64(record["target_after"]["health"])
                    steps = record["trace"]["steps"]
                    components = [trace_fields(step) for step in steps if step["id"] == "damage.component"]
                    self.assertEqual(len(components), 3, "split Fire damage must merge before protection")
                    fire = components[0]
                    self.assertEqual((fire["category"], fire["descriptor"], fire["raw"]), ("energy", "fire", 8))
                    for component in components:
                        category_immune = name == "all-immune" or (
                            name == "energy-immune" and component["category"] == "energy")
                        descriptor_immune = name == "cold-immune" and component["descriptor"] == "cold"
                        self.assertEqual(component["category_immune"], category_immune)
                        self.assertEqual(component["descriptor_immune"], descriptor_immune)
                        self.assertEqual(component["after_immunity"],
                                         0 if category_immune or descriptor_immune else component["raw"])
                        self.assertEqual(component["amount_kind"], "fixed")
                        self.assertEqual(component["rng_before"], component["rng_after"])
                        self.assertEqual(component["rolled"], [])
                        self.assertEqual(component["kept"], [])
                    reductions = [trace_fields(step) for step in steps if step["id"] == "damage.reduction"]
                    fire_reductions = [step for step in reductions if step["selector_kind"] == "descriptor" and step["selector"] == "fire"]
                    self.assertEqual(len(fire_reductions), 1)
                    self.assertEqual(fire_reductions[0]["capacity"], 3)
                    energy = [step for step in reductions if step["selector_kind"] == "category" and step["selector"] == "energy"]
                    self.assertEqual(len(energy), 1, "broad reduction applies once across Fire and Cold")
                    self.assertEqual(energy[0]["capacity"], 6, "source and equipped reduction must add")
                    self.assertEqual(energy[0]["before"], totals[1] - (0 if name == "all-immune" else 4))
                    self.assertEqual(energy[0]["after"], max(0, energy[0]["before"] - 6))
                    descriptor_positions = [i for i, step in enumerate(reductions) if step["selector_kind"] == "descriptor"]
                    category_positions = [i for i, step in enumerate(reductions) if step["selector_kind"] == "category"]
                    self.assertLess(max(descriptor_positions), min(category_positions))
                    summary = {"components": [{key: component[key] for key in
                                               ("category", "descriptor", "raw", "category_immune", "descriptor_immune", "after_immunity")}
                                              for component in components], "reductions": reductions, "finished": trace_fields(next(step for step in steps if step["id"] == "damage.finished"))}
                    self.assertEqual(summary["finished"], {"raw": 18,
                                                          "after_immunity": totals[0], "after_descriptors": totals[1],
                                                          "total": totals[2], "unused_edge": 0})
                    summaries.append(summary)
                self.assertEqual(*summaries)
                self.assertNotIn('"type":"combat_diagnostics"', "".join(player.transcript).replace(" ", ""))
                player.stop()
                wizard.stop()
                server.stop()

    def test_inspection_is_private_and_preserves_paid_preparation(self):
        package = self.directory / "inspection-arena"
        shutil.copytree(ROOT / "scenarios/mob-arena", package)
        manifest = package / "scenario.toml"
        source = manifest.read_text(encoding="utf-8")
        source = source.replace('control = "manual"', 'control = "all_ai", start_paused = true', 1)
        manifest.write_text(source, encoding="utf-8", newline="\n")
        result = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                capture_output=True, text=True, encoding="utf-8", timeout=15)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.server(scenario=package, wizard=True)
        wizard, _ = self.text_client(WIZARD_TOKEN, observe=True)
        observer, _ = self.client(WIZARD_TOKEN, observe=True)
        self.assertNotIn("Server error", wizard.command("wizard arena step"))
        self.frame(observer, lambda frame: frame.get("state") is not None
                   and resource(frame, "mana")["reserved"] == 1)
        before = self.request(observer, {"type":"snapshot"})
        self.assertTrue(before["state"]["observation"]["combat"]["preparation_active"])
        output = wizard.command("wizard creature inspect 1")
        self.assertNotIn("Server error", output)
        for detail in ("blue adept", "human", "arcane", "Hit die 1", "Spark Lance (Fire)"):
            self.assertIn(detail, output)
        after = self.request(observer, {"type":"snapshot"})
        self.assertEqual(after["state"], before["state"])
        target = wizard.command("wizard creature inspect 2")
        self.assertNotIn("Server error", target)
        self.assertIn("blue sentinel", target)
        self.assertIn("brace for impact", target.lower())
        player, _ = self.client(observe=True)
        for actor in (2, 999):
            denied = self.command(player, {"type":"wizard", "command":f"creature inspect {actor}"})
            self.assertIn("Wizard authority", denied["error"])

    def test_headless_exports_sources_and_restart_reconstructs_the_report(self):
        server = self.server(scenario="mob-arena", wizard=True)
        wizard, _ = self.client(WIZARD_TOKEN, observe=True)
        ready = self.command(wizard, {"type": "wizard", "command": "creature inspect 2"})
        self.assertFalse(ready.get("error"), ready.get("error"))
        import json
        messages = [json.loads(line).get("message") for line in wizard.transcript if line.startswith("{")]
        report = next(message["report"] for message in reversed(messages)
                      if message and message["type"] == "creature_inspection")
        self.assertEqual(report["actor"], "2")
        self.assertEqual(report["species"]["id"], "human")
        self.assertEqual(report["hit_dice"][1]["purchases"][0]["selection"]["talent"]["id"], "brace_for_impact")
        self.assertEqual(len(set(content_ids(report["starting_skills"]))), 2)
        self.assertEqual(report["restoration"], "authored")
        for entry in report["hit_dice"]:
            self.assertEqual(entry["point_pools"], [{"pool": {"id": "advancement", "name": "Advancement"},
                                                   "granted": 1, "spent": 1,
                                                   "allowed": ["skill_rank", "talent", "expertise"]}])
        self.assertEqual(report["dormant_talents"], [])
        self.assertIsInstance(report["hit_dice"][0]["health_seed"], str)
        player, _ = self.client(observe=True)
        denied = self.command(player, {"type": "wizard", "command": "creature inspect 2"})
        self.assertIn("Wizard authority", denied["error"])
        self.assertNotIn("health_seed", "".join(player.transcript))
        self.assertNotIn("grants", "".join(player.transcript))
        wizard.stop()
        player.stop()
        server.stop()
        self.server(scenario="mob-arena", wizard=True)
        wizard, _ = self.client(WIZARD_TOKEN, observe=True)
        ready = self.command(wizard, {"type": "wizard", "command": "creature inspect 2"})
        self.assertFalse(ready.get("error"), ready.get("error"))
        messages = [json.loads(line).get("message") for line in wizard.transcript if line.startswith("{")]
        restored = next(message["report"] for message in reversed(messages)
                        if message and message["type"] == "creature_inspection")
        self.assertEqual(restored, report)


    def test_ordered_strikes_private_inspection_keeps_dormant_authored_rows_and_riders(self):
        import copy
        import tomllib
        from arena_matrix import toml_source
        from strike_sequence_package import sequence_package
        package = sequence_package(self, autonomous=False)
        path = package / "scenario.toml"
        manifest = tomllib.loads(path.read_text(encoding="utf-8"))
        rules = tomllib.loads((package / "rules.toml").read_text(encoding="utf-8"))
        source = next(ability for ability in rules["abilities"] if ability["id"] == "triple_strike")
        strikes = copy.deepcopy(source["strikes"]["strikes"])
        strikes[1]["unlock"] = "rattling_blow"
        strikes[1]["riders"] = [{"trigger": "success", "check": {"skill": {"kind": "named", "skill": "spellcasting"},
            "defense": "cognitive"}, "effects": [{"kind": "apply_condition", "condition": "fear", "duration": 50}],
            "descriptors": ["mind_affecting"]}]
        manifest["creatures"]["species"]["sequence_fighter"]["melee"]["strikes"] = strikes
        path.write_text(toml_source(manifest), encoding="utf-8", newline="\n")
        validated = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                   capture_output=True, text=True, encoding="utf-8", timeout=30)
        self.assertEqual(validated.returncode, 0, validated.stderr)
        self.server(scenario=package, wizard=True)
        player, initial = self.client(observe=True)
        wizard, _ = self.client(WIZARD_TOKEN, observe=True)
        report = self._private_report(wizard, "creature inspect 2", "creature_inspection")
        natural = report["natural_attack"]
        self.assertEqual(natural["strike_count"], 3)
        natural_strikes = self._private_strikes(wizard, report, {"kind": "natural"})
        self.assertEqual(natural["source"]["id"], "sequence_fighter")
        self.assertEqual([row["ordinal"] for row in natural_strikes], [0, 1, 2])
        self.assertEqual([row["offset"] for row in natural_strikes], ["0", "100", "200"])
        middle = natural_strikes[1]
        self.assertEqual(middle["unlock"]["id"], "rattling_blow")
        self.assertIsNone(middle["check"])
        self.assertEqual(middle["on_failure"], [])
        self.assertEqual(middle["riders"][0]["ordinal"], 0)
        self.assertEqual(middle["riders"][0]["trigger"], "success")
        self.assertEqual(middle["riders"][0]["check"]["id"], "rider_check")
        self.assertEqual(middle["riders"][0]["effects"][0]["id"], "apply_condition")
        authored = next(row for row in report["ability_strikes"] if row["ability"] == "triple_strike")
        self.assertEqual(authored["source"], {"kind": "authored", "strike_count": 3})
        self.assertEqual(len(self._private_strikes(wizard, report, {"kind": "ability", "id": "triple_strike"})), 3)
        text, _ = self.text_client(WIZARD_TOKEN, observe=True)
        summary = text.command("wizard creature inspect 2")
        self.assertIn("Triple Strike", summary)
        output = text.command("strikes natural")
        for expected in ("Strike 2 (authored 1); offset 100; unlock Rattling Blow (rattling_blow)",
                         "Rider 1 (authored 0)", "Rider check:", "rider 1 effect 1",
                         "Primary success:", "Primary failure: 0 effects"):
            self.assertIn(expected, output)
        denied = self.command(player, {"type": "wizard", "command": "creature inspect 2"})
        self.assertIn("Wizard authority", denied["error"])
        self.assertEqual(denied["state"], initial["state"])
        self.assertNotIn("ability_strikes", "".join(player.transcript))
        self.assertNotIn("rider_check", "".join(player.transcript))

    def test_full_natural_attack_executes_and_survives_restart(self):
        import json
        import tomllib
        from arena_matrix import toml_source
        # Isolate the natural source from production talent/loadout changes.
        # Author the complete table rather than depending on TOML line layout.
        package = creature_package(self, "mixed-natural-arena", hostile=True, abilities=())
        manifest_path = package / "scenario.toml"
        manifest = tomllib.loads(manifest_path.read_text(encoding="utf-8"))
        manifest["arena"] = {"participants": [1, 2], "control": "all_ai", "start_paused": True,
                             "ticks": 100000, "actions": 10000}
        manifest["factions"] = {"neutral": ["foe"], "foe": ["neutral"]}
        character = manifest["characters"][0]
        character.update(unselected="ai", ai="practice")
        character["creature"]["starting_skills"] = ["light_weaponry", "spellcasting"]
        attack = natural_attack_source([
            {"category": "energy", "descriptor": "fire", "amount": {"kind": "rolled",
                "dice": {"count": 2, "sides": 6, "bonus": -1}}},
            {"category": "keen", "amount": {"kind": "fixed", "amount": 3}},
        ], skill="light_weaponry", bonus=2, wind_up=90, recovery=70)
        manifest["creatures"]["species"]["human"]["melee"] = tomllib.loads(attack)["melee"]
        manifest_path.write_text(toml_source(manifest), encoding="utf-8", newline="\n")
        validated = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                   capture_output=True, text=True, encoding="utf-8", timeout=15)
        self.assertEqual(validated.returncode, 0, validated.stderr)
        server = self.server(scenario=package, wizard=True)
        wizard, _ = self.client(WIZARD_TOKEN, observe=True)

        def query(command, kind):
            ready = self.command(wizard, {"type": "wizard", "command": command})
            self.assertFalse(ready.get("error"), ready.get("error"))
            messages = [json.loads(line).get("message") for line in wizard.transcript if line.startswith("{")]
            return next(message["report"] for message in reversed(messages)
                        if message and message["type"] == kind)

        report = query("creature inspect 2", "creature_inspection")
        original = report["natural_attack"]
        original_strikes = self._private_strikes(wizard, report, {"kind": "natural"})
        self.assertEqual((original["skill"]["id"], original["bonus"], original["wind_up"], original["recovery"]),
                         ("light_weaponry", 2, 90, 70))
        self.assertEqual(original["source"]["id"], "human")
        self.assertEqual(original["strike_count"], 1)
        self.assertEqual(len(original_strikes), 1)
        strike = original_strikes[0]
        self.assertEqual((strike["ordinal"], strike["offset"], strike["unlock"]), (0, "0", None))
        self.assertIsNotNone(strike["check"])
        self.assertEqual(strike["on_failure"], [])
        self.assertEqual(strike["riders"], [])
        self.assertEqual(strike["on_success"][0]["id"], "damage")
        components = trace_fields(strike["on_success"][0])
        self.assertEqual(components, {"primary_component_0": "Energy (fire): 2d6-1",
                                      "additional_component_1": "Keen: 3"})
        text, _ = self.text_client(WIZARD_TOKEN, observe=True)
        output = text.command("wizard creature inspect 2")
        self.assertIn("check +2; wind-up 90; recovery 70", output)
        output = text.command("strikes natural")
        self.assertIn("Strike 1 (authored 0); offset 0; unlock none", output)
        self.assertIn("Primary success:", output)
        self.assertIn("Energy (fire): 2d6-1", output)
        self.assertIn("Keen: 3", output)
        query("combat capture on", "combat_diagnostics")
        start = len(wizard.transcript)
        ready = self.command(wizard, {"type": "wizard", "command": "arena step 32"})
        self.assertFalse(ready.get("error"), ready.get("error"))

        def completed(frame):
            message = frame.get("message") or {}
            return (message.get("type") == "waiting" and message.get("on") in ("paused", "stopped")
                    and frame.get("state") is not None
                    and int(frame["state"]["observation"]["tick"]) > 0)

        frames = [json.loads(line) for line in wizard.transcript[start:] if line.startswith("{")]
        if not any(completed(frame) for frame in frames):
            self.frame(wizard, completed)
        report = query("combat inspect", "combat_diagnostics")
        records = list(report["records"])
        while records and int(records[0]["sequence"]) > int(report["captured"]) - report["retained"] + 1:
            older = query(f'combat inspect {int(records[0]["sequence"]) - 1}', "combat_diagnostics")
            self.assertTrue(older["records"])
            records = older["records"] + records
        matching = []
        for record in records:
            steps = record["trace"]["steps"]
            if record["ability"] and record["ability"]["id"] == "basic_melee":
                components = [trace_fields(step) for step in steps if step["id"] == "damage.component"]
                if components:
                    matching.append(components)
        self.assertTrue(matching, "no resolved natural attack was captured")
        for components in matching:
            fire = next(component for component in components if component["category"] == "energy")
            self.assertEqual(fire["descriptor"], "fire")
            self.assertEqual((fire["amount_kind"], fire["count"], fire["sides"], fire["bonus"]), ("rolled", 2, 6, -1))
            keen = next(component for component in components if component["category"] == "keen")
            self.assertEqual(keen["amount_kind"], "fixed")
            self.assertEqual(keen["raw"], 3)
        wizard.stop()
        text.stop()
        server.stop()
        self.server(scenario=package, wizard=True)
        wizard, _ = self.client(WIZARD_TOKEN, observe=True)
        restored = query("creature inspect 2", "creature_inspection")
        self.assertEqual(restored["natural_attack"], original)
        self.assertEqual(self._private_strikes(wizard, restored, {"kind": "natural"}), original_strikes)


    def test_equipped_weapon_discloses_full_source_and_resolves_its_own_skill(self):
        import json
        package = creature_package(self, "equipped-creature", abilities=("fixture_stamina_strike",))
        manifest = package / "scenario.toml"
        source = manifest.read_text(encoding="utf-8")
        blade = ('fire_blade = { class = "weapon", name = "fire blade", equipment = { slot = "weapon", attack = { skill = "light_weaponry", bonus = 100, wind_up = 90, recovery = 70, damage = { primary = { category = "energy", descriptor = "fire", sides = 6 }, components = [{ category = "energy", descriptor = "fire", amount = { type = "rolled", count = 2, sides = 6, bonus = -1 } }, { category = "keen", amount = { type = "fixed", value = 3 } }] } }, modifiers = [] } }, ')
        self.assertIn('archetypes = { ', source)
        source = source.replace('archetypes = { ', 'archetypes = { ' + blade, 1)
        manifest.write_text(source, encoding="utf-8", newline="\n")
        region = package / "regions/1.toml"
        source = ('items = [{ id = 100, at = [1,1,0], archetype = "fire_blade", carried_by = 1, '
                  'equipped_slot = 0 }]\n' + region.read_text(encoding="utf-8"))
        region.write_text(source, encoding="utf-8", newline="\n")
        validated = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                   capture_output=True, text=True, encoding="utf-8", timeout=15)
        self.assertEqual(validated.returncode, 0, validated.stderr)
        server = self.server(scenario=package, wizard=True)
        observer, initial = self.client(observe=True)
        wizard, _ = self.client(WIZARD_TOKEN, observe=True)

        def weapon(frame):
            return next(item["known_equipment"]["attack"]
                        for item in frame["state"]["observation"]["interactions"]["inventory"]
                        if item["equipped_slot"] == 0)

        self.assertIsNone(next(item["known_equipment"] for item in
                              initial["state"]["observation"]["interactions"]["inventory"]
                              if item["equipped_slot"] == 0))
        self.wizard_command(wizard, "identify 1 100")
        initial = self.request(observer, {"type": "snapshot"})
        declared = weapon(initial)
        self.assertEqual((declared["skill"]["id"], declared["bonus"], declared["wind_up"], declared["recovery"]),
                         ("light_weaponry", 100, 90, 70))
        self.assertEqual(declared["damage"]["primary"]["descriptor"]["id"], "fire")
        self.assertEqual(len(declared["damage"]["components"]), 2)

        def query(command):
            ready = self.command(wizard, {"type": "wizard", "command": command})
            self.assertFalse(ready.get("error"), ready.get("error"))
            messages = [json.loads(line).get("message") for line in wizard.transcript if line.startswith("{")]
            return next(message["report"] for message in reversed(messages)
                        if message and message["type"] == "combat_diagnostics")

        inspection = self._private_report(wizard, "creature inspect 1", "creature_inspection")
        selected = inspection["selected_attack"]
        self.assertEqual(selected["attack"], {"kind": "weapon", "attack": declared})
        self.assertEqual((selected["source"]["id"], selected["category"], selected["socket"]),
                         ("100", "equipment", 0))
        self.assertEqual(selected["check_bonus"], "101")
        self.assertNotEqual(inspection["natural_attack"], declared)
        self.assertEqual((selected["attribute"]["id"], selected["attribute_value"], selected["rank"]),
                         ("speed", 1, 0))
        profiles = [trace_fields(row) for row in selected["abilities"]]
        basic = next(profile for profile in profiles if profile["ability"]["id"] == "basic_melee")
        self.assertEqual((basic["wind_up"], basic["recovery"]), (75, 59))
        text_wizard, _ = self.text_client(WIZARD_TOKEN, observe=True)
        disclosed = text_wizard.command("wizard creature inspect 1")
        for detail in ("Species melee: Heavy weaponry", "Selected melee: Equipment 100",
                       "socket 0", "Light weaponry; weapon check +100; wind-up 90; recovery 70",
                       "Selected check: +101; Speed 1; skill rank 0; weapon +100",
                       "Selected damage: Energy (Fire) 2d6-1"):
            self.assertIn(detail, disclosed)
        text_wizard.stop()
        query("combat capture on")
        text, _ = self.adventure()
        for command, ability in [("attack practice target", "Attack"),
                                 ("fixture stamina strike practice target", "Fixture Stamina Strike")]:
            before = self.request(observer, {"type": "snapshot"})
            tick = int(before["state"]["observation"]["tick"])
            output = self.say(text, command)
            self.assertNotIn("Server error", output)
            if ability == "Attack":
                # The private report binds the weapon source; ordinary combat
                # events disclose only the basic selector and primary outcome.
                self.assertIn("practice target", output)
                self.assertNotIn("fire blade", output)
                self.assertNotIn("use Attack", output)
            else:
                self.assertIn("You prepare to use " + ability + ".", output)

            def completed(frame):
                observation = (frame.get("state") or {}).get("observation") or {}
                return (int(observation.get("tick", 0)) > tick
                        and observation.get("ready")
                        and observation["combat"]["preparation_remaining"] is None)

            snapshot = self.request(observer, {"type": "snapshot"})
            if not completed(snapshot):
                self.frame(observer, completed)
        # The target may retaliate under its own engagement policy. Check the
        # two selected actor/target resolutions without treating that as a third
        # player attack or suppressing the target's ordinary behavior.
        records = [record for record in query("combat inspect")["records"]
                   if record["actor"] == "1" and record["target"] == "2"]
        self.assertEqual(len(records), 2)
        self.assertEqual([record["ability"]["id"] for record in records],
                         ["basic_melee", "fixture_stamina_strike"])
        self.assertIsNone(records[0]["charge"])
        for record in records:
            steps = record["trace"]["steps"]
            check = next(trace_fields(step) for step in steps if step["id"] == "check")
            self.assertEqual(check["bonus"], 101)
            components = [trace_fields(step) for step in steps if step["id"] == "damage.component"]
            fire = next(component for component in components if component["category"] == "energy")
            self.assertEqual(fire["descriptor"], "fire")
            self.assertEqual((fire["amount_kind"], fire["count"], fire["sides"], fire["bonus"]), ("rolled", 2, 6, -1))
            keen = next(component for component in components if component["category"] == "keen")
            self.assertEqual(keen["amount_kind"], "fixed")
            self.assertEqual(keen["raw"], 3)
        paid = next(record for record in records if record["charge"] is not None)
        self.assertEqual(paid["charge"], {"mana": None, "focus": None,
            "stamina": {"resource": {"id": "stamina", "name": "Stamina"}, "start": 1, "resolution": 1}})
        impact = next(trace_fields(step) for step in paid["trace"]["steps"]
                      if step["id"] == "damage.component" and trace_fields(step)["category"] == "impact")
        self.assertEqual(impact["raw"], 3)
        text.stop()
        observer.stop()
        wizard.stop()
        server.stop()
        self.server(scenario=package, wizard=True)
        observer, restored = self.client(observe=True)
        self.assertEqual(weapon(restored), declared)
        wizard, _ = self.client(WIZARD_TOKEN, observe=True)
        restored_inspection = self._private_report(wizard, "creature inspect 1", "creature_inspection")
        self.assertEqual(restored_inspection["selected_attack"], selected)
        self.assertEqual(restored_inspection["natural_attack"], inspection["natural_attack"])
        self.assertEqual(restored_inspection["hit_dice"], inspection["hit_dice"])


class CombatDiagnosticsProcesses(ProcessTestCase):
    def test_paid_resolution_private_query_paging_and_restart(self):
        import json
        package = self.directory / "diagnostic-arena"
        shutil.copytree(ROOT / "scenarios/mob-arena", package)
        manifest = package / "scenario.toml"
        source = manifest.read_text(encoding="utf-8").replace(
            'control = "manual"', 'control = "all_ai", start_paused = true', 1)
        manifest.write_text(source, encoding="utf-8", newline="\n")
        result = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                capture_output=True, text=True, encoding="utf-8", timeout=15)
        self.assertEqual(result.returncode, 0, result.stderr)
        server = self.server(scenario=package, wizard=True)
        wizard, _ = self.client(WIZARD_TOKEN, observe=True)
        player, _ = self.client(observe=True)

        def query(operation):
            ready = self.command(wizard, {"type": "wizard", "command": operation})
            self.assertFalse(ready.get("error"), ready.get("error"))
            messages = [json.loads(line).get("message") for line in wizard.transcript if line.startswith("{")]
            return next(message["report"] for message in reversed(messages)
                        if message and message["type"] == "combat_diagnostics")

        self.assertFalse(query("combat inspect")["enabled"])
        self.assertTrue(query("combat capture on")["enabled"])
        def step():
            start = len(wizard.transcript)
            ready = self.command(wizard, {"type": "wizard", "command": "arena step 32"})
            self.assertFalse(ready.get("error"), ready.get("error"))
            def completed(frame):
                message = frame.get("message") or {}
                return (message.get("type") == "waiting" and message.get("on") in ("paused", "stopped")
                        and frame.get("state") is not None
                        and int(frame["state"]["observation"]["tick"]) > 0)
            frames = [json.loads(line) for line in wizard.transcript[start:] if line.startswith("{")]
            if not any(completed(frame) for frame in frames):
                self.frame(wizard, completed)
        step()
        before = self.request(wizard, {"type": "snapshot"})
        report = query("combat inspect")
        self.assertGreater(int(report["captured"]), 0)
        self.assertTrue(report["records"])
        self.assertLessEqual(len(report["records"]), 8)
        for record in report["records"]:
            for field in ("actor_before", "actor_after", "target_before", "target_after"):
                combatant = record[field]
                self.assertLessEqual(u64(combatant["health"]), u64(combatant["maximum_health"]))
                if combatant["injury"] is not None:
                    self.assertGreaterEqual(u64(combatant["injury"]), 0)
        self.assertTrue(any(record["charge"] for record in report["records"]))
        self.assertTrue(any(step["id"] == "check" for record in report["records"] for step in record["trace"]["steps"]))
        after = self.request(wizard, {"type": "snapshot"})
        self.assertEqual(after["state"], before["state"])
        denied = self.command(player, {"type": "wizard", "command": "combat capture off"})
        self.assertIn("Wizard authority", denied["error"])
        self.assertTrue(query("combat inspect")["enabled"])
        self.assertNotIn('"type":"combat_diagnostics"', "".join(player.transcript).replace(" ", ""))
        first = int(report["records"][0]["sequence"])
        self.assertGreater(first, int(report["dropped"]) + 1, "fixture must exercise backwards paging")
        older = query(f"combat inspect {first - 1}")
        self.assertEqual(int(older["records"][-1]["sequence"]), first - 1)
        first = int(older["records"][0]["sequence"])
        while first > int(report["dropped"]) + 1:
            older = query(f"combat inspect {first - 1}")
            first = int(older["records"][0]["sequence"])
        text, _ = self.text_client(WIZARD_TOKEN, observe=True)
        output = text.command("wizard combat inspect")
        self.assertIn("Combat diagnostics: enabled", output)
        self.assertIn("Record", output)
        self.assertIn("Rng before", output)
        import sys
        from combat_diagnostics import verify_export
        def export(name):
            transcript = self.directory / f"{name}.jsonl"
            output = self.directory / f"{name}.json"
            transcript.write_text("\n".join(wizard.transcript) + "\n", encoding="utf-8", newline="\n")
            result = subprocess.run([sys.executable, ROOT / "scripts/combat_diagnostics.py", "export", transcript, output],
                                    capture_output=True, text=True, encoding="utf-8", timeout=15)
            self.assertEqual(result.returncode, 0, result.stderr)
            document = json.loads(output.read_text(encoding="utf-8"))
            self.assertTrue(verify_export(document))
            self.assertEqual(len(document["records"]), document["retained"])
            return output, document
        baseline_path, baseline = export("baseline")
        rewound = self.command(wizard, {"type": "wizard", "command": "rewind initial"})
        self.assertFalse(rewound.get("error"), rewound.get("error"))
        fresh = query("combat inspect")
        self.assertTrue(fresh["enabled"])
        self.assertEqual(fresh["records"], [])
        step()
        replay_report = query("combat inspect")
        first = int(replay_report["records"][0]["sequence"])
        while first > int(replay_report["dropped"]) + 1:
            older = query(f"combat inspect {first - 1}")
            first = int(older["records"][0]["sequence"])
        replay_path, replay = export("replay")
        self.assertNotEqual(baseline["sha256"], replay["sha256"], "rewind uses fresh payment owners")
        self.assertEqual(baseline["numerical_sha256"], replay["numerical_sha256"])
        compared = subprocess.run([sys.executable, ROOT / "scripts/combat_diagnostics.py", "compare", baseline_path, replay_path],
                                  capture_output=True, text=True, encoding="utf-8", timeout=15)
        self.assertEqual(compared.returncode, 0, compared.stderr)
        text.stop()
        wizard.stop()
        player.stop()
        server.stop()
        self.server(scenario=package, wizard=True)
        wizard, _ = self.client(WIZARD_TOKEN, observe=True)
        restored = query("combat inspect")
        self.assertFalse(restored["enabled"])
        self.assertEqual(restored["records"], [])
        self.assertEqual(restored["captured"], "0")


class NativeCreatureInspectionProcesses(ProcessTestCase):
    graphical = True

    def test_natural_melee_native_pages_show_both_rider_sources_with_real_keys(self):
        package = natural_melee_page_package(self)
        self.server(scenario=package, wizard=True)
        window = self.launch("tor-client-ascii", ["--connect", self.address, "--observe", "--report-frames"], token=WIZARD_TOKEN)
        initial = self.ascii_frame(window, lambda frame: frame["state"] is not None and not frame["busy"])
        key = self.native_keys(window)
        def visible(frame):
            offset = frame["screen"]["stats_scroll"]
            return " ".join(" ".join(frame["stats_rows"][offset:offset + 24]).split())
        self.native_press(window, key, "F7", "wizard")
        self.native_text(window, "creature inspect 2")
        self.ascii_frame(window, lambda frame: frame.get("wizard_command") == "creature inspect 2")
        report = self.native_press(window, key, "Return", "enter")
        self.assertTrue(report["screen"]["stats"])
        menu = self.native_press(window, key, "Tab", "tab")
        choices = menu["stats_rows"][1:24]
        selected = next(index for index, row in enumerate(choices) if "(triple_strike)" in row)
        for _ in range(selected):
            menu = self.native_press(window, key, "Down", "down")
        self.assertTrue(menu["stats_rows"][selected + 1].startswith("> "))
        shown = self.native_press(window, key, "Return", "enter")
        visible_sources = set()
        labels = ("species-authored 0", "ability-added 0", "Condition: at least 2 qualifying primary hits")
        for _ in range(40):
            self.assertIn("Natural source:", visible(shown))
            self.assertEqual(shown["state"], initial["state"])
            for label in labels:
                positions = [index for index, row in enumerate(shown["stats_rows"]) if label in row]
                if not positions:
                    continue
                for _ in range((positions[0] + 23) // 24):
                    if positions[0] < shown["screen"]["stats_scroll"] + 24:
                        break
                    previous_scroll = shown["screen"]["stats_scroll"]
                    shown = self.native_press(window, key, "PageDown", "recent_history")
                    self.assertGreater(shown["screen"]["stats_scroll"], previous_scroll)
                self.assertIn(label, visible(shown))
                visible_sources.add(label)
            if visible_sources == set(labels):
                break
            previous = shown["stats_rows"]
            shown = self.native_press(window, key, "bracketright", "faster")
            self.assertNotEqual(shown["stats_rows"], previous)
        self.assertEqual(visible_sources, set(labels))
        for _ in range(3):
            shown = self.native_press(window, key, "Escape", "escape")
        self.assertFalse(shown["screen"]["stats"])
        self.assertEqual(shown["state"], initial["state"])

    def test_ordered_strikes_native_source_menu_and_bracket_pages_use_real_keys(self):
        package = strike_page_package(self)
        self.server(scenario=package, wizard=True)
        wizard = self.launch("tor-client-ascii", ["--connect", self.address, "--observe", "--report-frames"], token=WIZARD_TOKEN)
        initial = self.ascii_frame(wizard, lambda frame: frame["state"] is not None and not frame["busy"])
        key = self.native_keys(wizard)
        key("F7", True)
        self.ascii_frame(wizard, lambda frame: frame.get("wizard_command") == "")
        key("F7", False)
        self.native_text(wizard, "creature inspect 2")
        self.ascii_frame(wizard, lambda frame: frame.get("wizard_command") == "creature inspect 2")
        key("Return", True)
        self.ascii_frame(wizard, lambda frame: frame["screen"]["stats"] and not frame["busy"])
        key("Return", False)
        key("Tab", True)
        self.ascii_frame(wizard, lambda frame: "Strike sources" in "\n".join(frame["stats_rows"]))
        key("Tab", False)
        key("Return", True)
        first = self.ascii_frame(wizard, lambda frame: not frame["busy"] and "Strike source:" in "\n".join(frame["stats_rows"]))
        key("Return", False)
        key("bracketright", True)
        second = self.ascii_frame(wizard, lambda frame: not frame["busy"] and frame["stats_rows"] != first["stats_rows"]
                                  and "Strike source:" in "\n".join(frame["stats_rows"]))
        key("bracketright", False)
        key("bracketleft", True)
        previous = self.ascii_frame(wizard, lambda frame: not frame["busy"] and frame["stats_rows"] == first["stats_rows"])
        key("bracketleft", False)
        for frame in (first, second, previous):
            self.assertEqual(frame["state"], initial["state"])
            self.assertLessEqual(len(frame["stats_rows"]), 600, "only bounded page rows are rendered")

        def release_escape_after_presentation(modal):
            # Windows posts key messages without xdotool's process boundary.
            # Give minifb an observed up interval before pressing Escape again:
            # Right is inert in these stats views, but its new presented receipt
            # proves the native event pump advanced rather than reusing a frame.
            key("Escape", False)
            key("Right", True)
            released = self.ascii_frame(wizard, lambda frame: frame["frame"] > modal["frame"]
                                        and frame.get("input_done") == "right" and not frame["busy"])
            key("Right", False)
            self.assertEqual(released["screen"], modal["screen"])
            self.assertEqual(released["stats_rows"], modal["stats_rows"])
            self.assertEqual(released["state"], initial["state"])

        key("Escape", True)
        sources = self.ascii_frame(wizard, lambda frame: "Strike sources" in "\n".join(frame["stats_rows"]))
        release_escape_after_presentation(sources)
        key("Escape", True)
        main = self.ascii_frame(wizard, lambda frame: frame["screen"]["stats"]
                                and "Species melee:" in "\n".join(frame["stats_rows"]))
        self.assertEqual(main["state"], initial["state"])
        release_escape_after_presentation(main)
        key("Escape", True)
        self.ascii_frame(wizard, lambda frame: not frame["screen"]["stats"])
        key("Escape", False)

    def test_native_wizard_console_presents_a_scrollable_readonly_report(self):
        self.server(scenario="mob-arena", wizard=True)
        wizard = self.launch("tor-client-ascii", ["--connect", self.address, "--observe", "--report-frames"], token=WIZARD_TOKEN)
        initial = self.ascii_frame(wizard, lambda frame: frame["state"] is not None and not frame["busy"])
        key = self.native_keys(wizard)
        key("F7", True)
        self.ascii_frame(wizard, lambda frame: frame.get("wizard_command") == "")
        key("F7", False)
        self.native_text(wizard, "creature inspect 2")
        self.ascii_frame(wizard, lambda frame: frame.get("wizard_command") == "creature inspect 2")
        key("Return", True)
        shown = self.ascii_frame(wizard, lambda frame: frame["screen"]["stats"] and not frame["busy"])
        key("Return", False)
        self.assertEqual(shown["state"], initial["state"])
        self.assertIn("blue sentinel", "\n".join(shown["stats_rows"]))
        self.assertIn("Hit die 1", "\n".join(shown["stats_rows"]))
        key("Down", True)
        self.ascii_frame(wizard, lambda frame: frame["screen"]["stats_scroll"] == 1)
        key("Down", False)
        key("Escape", True)
        self.ascii_frame(wizard, lambda frame: not frame["screen"]["stats"])
        key("Escape", False)
        key("F7", True)
        self.ascii_frame(wizard, lambda frame: frame.get("wizard_command") == "")
        key("F7", False)
        self.native_text(wizard, "combat capture on")
        self.ascii_frame(wizard, lambda frame: frame.get("wizard_command") == "combat capture on")
        key("Return", True)
        diagnostic = self.ascii_frame(wizard, lambda frame: frame["screen"]["stats"] and not frame["busy"])
        key("Return", False)
        self.assertEqual(diagnostic["state"], initial["state"])
        self.assertEqual(diagnostic["stats_rows"],
                         ["Combat diagnostics: enabled; captured 0, dropped 0, retained 0"])
        wizard.stop()
