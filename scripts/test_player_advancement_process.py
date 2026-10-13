"""Ordinary owner-only advancement in the actual Text, headless and native clients."""
import json

from stream_relay import StreamRelay
from process_harness import ProcessTestCase, ROOT, SPECTATOR_TOKEN, ascii_input_completion, package_path


PACKAGE = ROOT / "scenarios/tests/advancement-owner"


def observation(frame):
    return frame["state"]["observation"]


def ration_stock(frame):
    view = observation(frame)
    return sum(int(row["quantity"]) for row in
               view["inventory"] + [ground["item"] for ground in view["ground_items"]]
               if (row["known_supply"] or {}).get("id") == "ration")


def talents(frame):
    stats = observation(frame)["combat"]["own_stats"]
    return {row["id"] for row in stats["active_talents"] + stats["dormant_talents"]}


def maximum_owner_package(test):
    """Author a feasible joint ledger/catalog bound through the real compiler."""
    import shutil
    import subprocess
    import tomllib
    from arena_matrix import toml_source
    from ruleset_sources import load_rules

    package = test.directory / "maximum-owner"
    shutil.copytree(PACKAGE, package)
    compiler = test.bin / ("tor-scenario" + test.suffix)
    rules = load_rules(package / "rules.toml", compiler)
    pools = ["advancement", *(f"pool_{index:02}" for index in range(1, 16))]
    rules["point_pools"] = [
        {"id": pool, "name": f"Pool {index}", "allowed": ["skill_rank", "talent"]}
        for index, pool in enumerate(pools)]
    rules["constants"]["racial_point_grants"] = [
        {"pool": pool, "points": 241 if index == 0 else 1}
        for index, pool in enumerate(pools)]
    rules["talents"] = []
    for index in range(4096):
        if index == 0:
            # One expression reaches its 256-node ceiling. The saved-metadata
            # budget forbids pretending every catalog row can do so jointly.
            requirement = {"kind": "all", "requirements": [
                {"kind": "total_hd", "minimum": 1} for _ in range(255)]}
        elif index < 256:
            requirement = {"kind": "owned_talent", "talent": f"talent_{index - 1:04}"}
        else:
            requirement = {"kind": "total_hd", "minimum": 1}
        rules["talents"].append({"id": f"talent_{index:04}", "name": f"Talent {index}",
                                 "prerequisites": [], "requirement": requirement, "modifiers": []})
    (package / "rules.toml").write_text(toml_source(rules), encoding="utf-8", newline="\n")
    manifest = tomllib.loads((package / "scenario.toml").read_text(encoding="utf-8"))
    build = manifest["characters"][0]["creature"]
    skills = ("athletics", "heavy_weaponry", "agility", "light_weaponry", "stealth",
              "thievery", "crafting", "deduction", "lore", "medicine", "discipline",
              "intimidation", "insight", "perception", "survival", "deception",
              "leadership", "persuasion", "spellcasting")
    dice = [{"source": "racial", "purchases": []} for _ in range(256)]
    for index in range(256):
        dice[index // 16]["purchases"].append({"pool": pools[index % 16],
            "selection": {"kind": "talent", "talent": f"talent_{index:04}"}})
    purchases = 0
    for skill in skills:
        for _ in range(int(skill in build["starting_skills"]), 5):
            dice[16 + purchases // 16]["purchases"].append({"pool": pools[purchases % 16],
                "selection": {"kind": "skill_rank", "skill": skill}})
            purchases += 1
    test.assertEqual(purchases, 93)
    build["hit_dice"] = dice
    (package / "scenario.toml").write_text(toml_source(manifest), encoding="utf-8", newline="\n")
    # The region ally must also use the newly authored grants, but has no owned
    # catalog choices to rewrite. All inventory remains ordinary authored data.
    validated = subprocess.run([compiler, "validate", package], capture_output=True,
                               text=True, encoding="utf-8", timeout=60)
    test.assertEqual(validated.returncode, 0, validated.stderr)
    return package


class PlayerAdvancementProcesses(ProcessTestCase):
    def advance(self, player, command):
        frame = self.command(player, {"type": "advancement", "command": command})
        self.assertIsNone(frame["error"], command)
        if (command.startswith("confirm") or command == "resume") and any(
                row["phase"] == "queued" for row in frame["intentions"]):
            frame = self.frame(player, lambda current: not any(
                row["phase"] == "queued" for row in current["intentions"]))
        return frame

    def rows(self, player, command):
        frame = self.advance(player, command)
        page = frame["advancement_page"]
        result = list(page["entries"])
        while page["next"] is not None:
            old = page
            frame = self.advance(player, "next")
            page = frame["advancement_page"]
            self.assertEqual(page["basis"], old["basis"])
            self.assertEqual(page["revision"], old["revision"])
            self.assertEqual(page["query"]["offset"], old["next"])
            result.extend(page["entries"])
        self.assertEqual(len(result), page["total"])
        return result

    def finish_respec(self, player, partner):
        # The fixture's disclosed preparation is 600 ticks; each ordinary
        # partner wait contributes 100. This bound is tied to that authored work.
        for _ in range(8):
            current = self.request(player, {"type": "snapshot"})
            if "steady_foundation" not in talents(current):
                return current
            self.assertIsNone(self.act(partner, {"type": "wait"})["error"])
        self.fail("Ordinary partner turns did not complete the authored respec")

    def test_headless_preview_traverses_dependencies_then_refunds_and_spends_one_ground_ration(self):
        self.server(scenario=PACKAGE)
        player, _ = self.client()
        partner, _ = self.client(actor=2)
        initial = self.advance(player, "overview")
        self.assertFalse(initial["state"]["wizard_game"])
        self.assertEqual(len(self.rows(player, "ledger")), 6)

        def thievery_rank(frame):
            return next(row["rank"] for row in
                        observation(frame)["combat"]["own_stats"]["skills"]
                        if row["skill"]["id"] == "thievery")

        self.assertEqual(thievery_rank(initial), 2)
        self.assertIn("dormant_anchor", {row["id"] for row in
                      observation(initial)["combat"]["own_stats"]["active_talents"]})
        # Initial talents must be eligible. Ordinary paid rank removal makes
        # this still-owned talent dormant before testing unrelated subtree removal.
        rank_preview = self.advance(player, "remove 2 3")
        rank_summary = rank_preview["advancement_page"]["entries"][0]["value"]
        self.assertEqual(rank_summary["removed_count"], 1)
        self.assertEqual(rank_summary["health"]["maximum"],
                         observation(initial)["combat"]["max_hp"])
        removed_rank = self.rows(player, "details removed")
        self.assertEqual(removed_rank[0]["value"]["selection"],
                         {"kind": "skill_rank", "skill": "thievery"})
        rank_refunds = self.rows(player, "details refunds")
        self.assertEqual(sum(row["value"]["points"] for row in rank_refunds), 1)
        self.advance(player, "confirm using carried")
        for _ in range(8):
            current = self.request(player, {"type": "snapshot"})
            if thievery_rank(current) == 1:
                break
            self.assertIsNone(self.act(partner, {"type": "wait"})["error"])
        self.assertEqual(thievery_rank(current), 1)
        self.assertEqual(ration_stock(current), ration_stock(initial) - 1)
        self.assertEqual(talents(current), talents(initial))
        self.assertIn("dormant_anchor", {row["id"] for row in
                      observation(current)["combat"]["own_stats"]["dormant_talents"]})
        baseline = self.advance(player, "overview")
        self.assertFalse(baseline["state"]["wizard_game"])
        stock = ration_stock(baseline)
        before_health = observation(baseline)["combat"]["max_hp"]
        self.assertEqual(len(self.rows(player, "ledger")), 5)
        self.assertTrue(self.rows(player, "balances"))
        self.assertTrue(self.rows(player, "skills"))
        self.assertTrue(self.rows(player, "talents"))
        preview = self.advance(player, "remove 1 1")
        summary = preview["advancement_page"]["entries"][0]["value"]
        self.assertEqual(summary["proposal"]["replacement"]["replacement"], None)
        self.assertEqual(summary["removed_count"], 2)
        self.assertEqual(summary["health"]["maximum"], before_health - 10)
        removed = self.rows(player, "details removed")
        self.assertEqual({row["value"]["selection"]["talent"] for row in removed},
                         {"steady_foundation", "foundation_followthrough"})
        refunds = self.rows(player, "details refunds")
        self.assertEqual(sum(row["value"]["points"] for row in refunds), 2)
        self.assertTrue(self.rows(player, "details balances"))
        self.assertTrue(self.rows(player, "details skills"))
        resulting = self.rows(player, "details talents")
        self.assertEqual({row["value"]["talent"]["id"] for row in resulting},
                         {"dormant_anchor", "alternate_foundation", "either_foundation"})
        self.assertEqual(next(row["value"]["status"] for row in resulting
                              if row["value"]["talent"]["id"] == "dormant_anchor"), "dormant")
        self.rows(player, "details conditions")
        unchanged = self.advance(player, "details summary")
        self.assertEqual(unchanged["state"], baseline["state"])
        ground = next(row for row in observation(unchanged)["ground_items"]
                      if row["item"]["name"] == "ground ration")
        self.assertTrue(ground["supply_reachable"])
        self.advance(player, f"confirm using #{ground['item']['id']}")
        completed = self.finish_respec(player, partner)
        self.assertEqual(ration_stock(completed), stock - 1)
        self.assertEqual(talents(completed),
                         {"dormant_anchor", "alternate_foundation", "either_foundation"})
        self.assertEqual(observation(completed)["combat"]["max_hp"], before_health - 10)
        self.advance(player, "overview")
        ledger = self.rows(player, "ledger")
        self.assertEqual(len(ledger), 3)

    def test_cancel_releases_unspent_ration_and_reconnect_does_not_restore_confirmation(self):
        self.assertEqual(package_path("advancement-owner").resolve(), PACKAGE.resolve())
        server = self.server(scenario="advancement-owner")
        player, _ = self.client()
        partner, _ = self.client(actor=2)
        baseline = self.advance(player, "overview")
        self.advance(player, "remove 1 1")
        self.advance(player, "confirm using carried")
        progress = self.advance(player, "overview")["advancement_page"]["entries"][0]["value"]["respec"]
        self.assertIsNotNone(progress)
        self.assertEqual(progress["replacement"]["source"], {"owner": 0, "index": 0})
        self.advance(player, "cancel")
        current = self.request(player, {"type": "snapshot"})
        self.assertEqual(ration_stock(current), ration_stock(baseline))
        self.assertEqual(talents(current), talents(baseline))
        self.advance(player, "overview")
        self.advance(player, "remove 1 1")
        player.stop(); partner.stop(); server.stop()
        self.server(scenario=PACKAGE)
        player, reopened = self.client()
        self.assertIsNone(reopened["advancement_page"])
        refused = self.command(player, {"type": "advancement", "command": "confirm"})
        self.assertIsNotNone(refused["error"])
        self.assertEqual(ration_stock(refused), ration_stock(baseline))
        self.assertEqual(talents(refused), talents(baseline))

    def test_control_loss_same_actor_and_spectator_cannot_retain_private_pages(self):
        self.server(scenario=PACKAGE)
        player, _ = self.client()
        self.advance(player, "overview")
        self.advance(player, "remove 1 1")
        released = self.request(player, {"type": "release_control"})
        self.assertFalse(released["has_control"])
        self.assertIsNone(released["advancement_page"])
        denied = self.command(player, {"type": "advancement", "command": "overview"})
        self.assertIsNotNone(denied["error"])
        spectator, _ = self.client(SPECTATOR_TOKEN)
        denied = self.command(spectator, {"type": "advancement", "command": "overview"})
        self.assertIsNotNone(denied["error"])
        self.assertIsNone(denied["advancement_page"])
        self.request(player, {"type": "acquire_control"})
        denied = self.command(player, {"type": "advancement", "command": "confirm"})
        self.assertIsNotNone(denied["error"])

    def test_paused_respec_resumes_exact_original_intention_and_basis(self):
        self.server(scenario=PACKAGE)
        player, _ = self.client()
        partner, _ = self.client(actor=2)
        self.advance(player, "overview")
        self.advance(player, "remove 1 1")
        self.advance(player, "confirm using carried")
        active = self.advance(player, "overview")["advancement_page"]["entries"][0]["value"]["respec"]
        self.assertIsNotNone(active)
        self.request(player, {"type": "release_control"})
        self.request(player, {"type": "acquire_control"})
        paused = self.advance(player, "overview")["advancement_page"]["entries"][0]["value"]["respec"]
        self.assertFalse(paused["active"])
        self.assertEqual(paused["intention"], active["intention"])
        self.assertEqual(paused["basis"], active["basis"])
        self.assertEqual(paused["replacement"], active["replacement"])
        self.assertEqual(paused["remaining_ticks"], active["remaining_ticks"])
        self.advance(player, "remove 1 1")
        refused = self.command(player, {"type": "advancement", "command": "confirm"})
        self.assertIsNotNone(refused["error"])
        self.advance(player, "resume")
        completed = self.finish_respec(player, partner)
        self.assertNotIn("steady_foundation", talents(completed))

    def test_text_adventure_and_script_preview_then_confirm_the_exact_owned_purchase(self):
        for adventure in (False, True):
            with self.subTest(adventure=adventure):
                self.save = self.directory / f"text-advancement-{adventure}.db"
                server = self.server(scenario=PACKAGE)
                partner, _ = self.client(actor=2)
                text, _ = self.adventure() if adventure else self.text_client()
                issue = (lambda line: self.say(text, line)) if adventure else text.command
                self.assertIn("Starting skills", issue("advancement"))
                self.assertIn("practice", issue("advancement balances"))
                preview = issue("advancement purchase 1 practice skill perception")
                self.assertIn("HD 1 / pool practice / skill perception", preview)
                self.assertIn("Confirm", preview)
                self.assertIn("permanent rank 1", issue("advancement details skills"))
                confirmed = issue("advancement confirm")
                self.assertNotIn("Server error", confirmed)
                observer, current = self.client(observe=True)
                rank = next(row["rank"] for row in observation(current)["combat"]["own_stats"]["skills"]
                            if row["skill"]["id"] == "perception")
                self.assertEqual(rank, 1)
                self.assertFalse(current["state"]["wizard_game"])
                self.assertIn("Starting skills", issue("advancement"))
                self.assertIn("Health", issue("advancement remove 1 1"))
                issue("sync")
                self.assertIn("stale", issue("advancement confirm"))
                text.stop(); observer.stop(); partner.stop(); server.stop()

    def test_maximum_legal_owner_ledger_balances_catalog_and_dependent_refunds_are_paged(self):
        import time
        package = maximum_owner_package(self)
        self.server(scenario=package)
        measured = {"pages": 0, "bytes": 0, "maximum": 0, "rows": 0}

        def capture_page(message):
            if message["type"] == "advancement_page":
                # Re-encode the complete received envelope, including request echo.
                size = len(json.dumps(message, ensure_ascii=False, separators=(",", ":")).encode("utf-8"))
                measured["pages"] += 1
                measured["bytes"] += size
                measured["maximum"] = max(measured["maximum"], size)
                measured["rows"] = max(measured["rows"], len(message["page"]["entries"]))

        relay = StreamRelay(self.address, on_server_message=capture_page)
        self.addCleanup(relay.close)
        player = self.launch("tor-client-headless", ["--connect", relay.address])
        initial = self.frame(player, lambda frame: frame["type"] == "ready")
        self.assertFalse(initial["state"]["wizard_game"])
        started = time.monotonic()
        overview = self.advance(player, "overview")
        points = overview["advancement_page"]["entries"][0]["value"]["points"]
        self.assertEqual((int(points["earned"]), int(points["spent"]), int(points["unspent"])),
                         (65536, 349, 65187))
        ledger = self.rows(player, "ledger")
        self.assertEqual(len(ledger), 349)
        self.assertEqual(len({(row["value"]["source"]["owner"], row["value"]["source"]["index"])
                              for row in ledger}), 349)
        balances = self.rows(player, "balances")
        self.assertEqual(len(balances), 4096)
        self.assertEqual(sum(row["value"]["granted"] for row in balances), 65536)
        self.assertEqual(sum(row["value"]["spent"] for row in balances), 349)
        self.assertEqual(len(self.rows(player, "talents")), 4096)
        preview = self.advance(player, "remove 1 1")
        summary = preview["advancement_page"]["entries"][0]["value"]
        self.assertEqual(summary["removed_count"], 256)
        removed = self.rows(player, "details removed")
        self.assertEqual(len(removed), 256)
        self.assertEqual({row["value"]["selection"]["talent"] for row in removed},
                         {f"talent_{index:04}" for index in range(256)})
        refunds = self.rows(player, "details refunds")
        self.assertEqual(len(refunds), 256)
        self.assertEqual(sum(row["value"]["points"] for row in refunds), 256)
        remaining = self.rows(player, "details balances")
        self.assertEqual(len(remaining), 4096)
        self.assertEqual(sum(row["value"]["spent"] for row in remaining), 93)
        self.assertEqual(self.rows(player, "details talents"), [])
        current = self.request(player, {"type": "snapshot"})
        self.assertEqual(current["state"], initial["state"])
        self.assertGreater(measured["pages"], 250)
        self.assertLessEqual(measured["maximum"], 32 * 1024)
        self.assertLessEqual(measured["rows"], 32)
        print(f"ordinary maximum owner traversal: {time.monotonic() - started:.3f}s, {measured}")


class PlayerAdvancementNativeProcesses(ProcessTestCase):
    graphical = True

    def test_native_f10_previews_dependencies_and_confirms_exact_ground_ration(self):
        self.server(scenario=PACKAGE)
        partner, _ = self.client(actor=2)
        window = self.launch("tor-client-ascii", ["--connect", self.address, "--report-frames"])
        initial = self.ascii_frame(window, lambda frame: frame["has_control"] and not frame["busy"])
        native = self.native_keys(window)

        def press(name, reported):
            native(name, True)
            try:
                return self.ascii_frame(window, ascii_input_completion(reported))
            finally:
                native(name, False)

        def command(text):
            self.native_text(window, text)
            self.ascii_frame(window, lambda frame: frame["advancement_command"] == text)
            return press("Return", "enter")

        opened = press("F10", "advancement")
        self.assertTrue(opened["advancement_open"])
        self.assertIn("Starting skills", " ".join(opened["advancement_rows"]))
        command("remove 1 1")
        removed = command("details removed")
        self.assertIn("Steady Foundation", " ".join(removed["advancement_rows"]))
        self.assertIn("Foundation Followthrough", " ".join(removed["advancement_rows"]))
        self.assertEqual(removed["state"], initial["state"])
        picker = command("confirm")
        self.assertIsNotNone(picker["supply_selection"])
        ground = next(row for row in picker["supply_selection"]["candidates"] if not row["carried"])
        position = picker["supply_selection"]["candidates"].index(ground) + 1
        for _ in range(position):
            press("Down", "down")
        selected = press("space", "wait")
        self.assertEqual(selected["supply_selection"]["checked"], [ground["item"]])
        press("Return", "enter")
        observer, _ = self.client(observe=True)
        for _ in range(8):
            current = self.request(observer, {"type": "snapshot"})
            if "steady_foundation" not in talents(current):
                break
            self.assertIsNone(self.act(partner, {"type": "wait"})["error"])
        self.assertNotIn("steady_foundation", talents(current))
        self.assertEqual(ration_stock(current), ration_stock(initial) - 1)
        shown = command("overview")
        self.assertTrue(shown["window_open"])
        self.assertEqual(shown["state"], current["state"])
        closed = press("Escape", "escape")
        self.assertFalse(closed["advancement_open"])
