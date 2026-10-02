"""Wizard acceptance through the actual server, text client and native ASCII window."""
import os
import subprocess
import unittest

from process_harness import ProcessTestCase, TOKEN, SPECTATOR_TOKEN, WIZARD_TOKEN, load_fixture


class WizardProcesses(ProcessTestCase):
    graphical = True

    def text(self, token=WIZARD_TOKEN, actor=1):
        client = self.launch("tor-client-text", ["--script", "--connect", self.address, "--actor", actor], token=token)
        welcome = client.until(lambda line: line == "Ready.")
        return client, welcome

    def spectator(self, capture=None):
        args = ["--connect", self.address, "--automation"]
        if capture:
            args += ["--capture", capture]
        client = self.launch("tor-client-ascii", args, token=SPECTATOR_TOKEN)
        frame = self.ascii_frame(client, lambda f: f["state"] is not None and not f["busy"])
        self.assertTrue(frame["window_open"])
        return client, frame

    def test_scripted_wizard_scenario_rewind_multiple_frontends_and_disabled_resume(self):
        server = self.server(wizard=True)
        wizard, welcome = self.text()
        self.assertIn("WIZARD GAME", welcome)
        capture = self.save.parent / "wizard.ppm"
        spectator, initial = self.spectator(capture)
        text_spectator, welcome = self.text(SPECTATOR_TOKEN)
        self.assertIn("WIZARD GAME", welcome)
        self.assertIn("read-only", welcome)
        self.assertTrue(initial["state"]["wizard_game"])
        fixture = load_fixture("wizard-foundation.json")
        for step in fixture["steps"]:
            output = wizard.command(step["command"])
            self.assertNotIn("Server error", output)
            if not step["command"].startswith("note"):
                seen = self.ascii_frame(spectator, lambda f: f["state"]["revision"] == step["revision"])
                self.assertEqual(seen["state"]["observation"]["position"], {"x":0,"y":0,"z":0})
                self.assertEqual(seen["state"]["observation"]["tick"], step["tick"])
                self.assertTrue(seen["state"]["wizard_game"])
        old_branch = seen["branch"]
        self.assertNotIn("Private abandoned future", str(seen))
        self.assertFalse(any(e["content"]["type"] == "wizard" for e in seen["history"]))
        self.assertIn("Your surroundings", text_spectator.command("sync"))
        for command in ["wizard rewind initial", "wizard item tablet 1 1 1 0", "note Forbidden"]:
            self.assertIn("read-only", text_spectator.command(command))
        self.assertIn("read-only", self.key(spectator, "control")["status"])
        self.flush_save()
        before = self.save.read_bytes()
        self.assertIn("read-only", self.key(spectator, "pickup")["status"])
        self.assertEqual(self.save.read_bytes(), before)
        self.assertNotIn("Server error", wizard.command("wizard rewind initial"))
        rewound = self.ascii_frame(spectator, lambda f: f["branch"] != old_branch)
        self.assertEqual(rewound["state"]["observation"]["tick"], 0)
        self.assertEqual(rewound["state"]["observation"]["inventory"], [])
        self.assertNotIn("known_places", rewound["state"]["observation"])
        self.assertIn("Private abandoned future", wizard.command(f"branch-history {old_branch}"))
        self.assertNotIn("Private abandoned future", text_spectator.command(f"branch-history {old_branch}"))
        wizard.command("take token")
        alternate = self.ascii_frame(spectator, lambda f: f["state"]["revision"] == 1)
        self.assertEqual(alternate["state"]["observation"]["tick"], 50)
        target = alternate["history"][-1]["id"]
        wizard.command("wizard actor 75 1 2 1 0")
        second, _ = self.text(actor=2)
        wizard.command("wait")
        # A response on actor 1's socket does not mean actor 2 has consumed its
        # pushed readiness revision. Establish that boundary before acting.
        self.assertIn("Ready to act", second.command("sync"))
        self.assertNotIn("Server error", second.command("wait"))
        self.assertNotIn("Server error", wizard.command(f"wizard rewind {target}"))
        restored = self.ascii_frame(spectator, lambda f: f["branch"] != alternate["branch"])
        self.assertEqual(restored["state"], alternate["state"])
        # Removed actors' connections end explicitly at rewind.
        self.assertNotEqual(second.child.wait(timeout=15), 0)
        wizard.command("release")
        player, _ = self.text(TOKEN)
        self.assertIn("Done.", player.command("wait"))
        final = self.ascii_frame(spectator, lambda f: f["state"]["observation"]["tick"] == 150)
        player.stop()
        wizard.stop()
        text_spectator.stop()
        self.key(spectator, "escape")
        spectator.child.wait(timeout=10)
        self.assertTrue(capture.exists())
        server.stop()
        server = self.server(wizard=False)
        player, welcome = self.text(TOKEN)
        self.assertIn("WIZARD GAME", welcome)
        self.assertIn("Wizard authority", player.command("wizard rewind initial"))
        resumed, current = self.spectator()
        self.assertEqual(current["state"], final["state"])
        self.assertEqual(current["branch"], final["branch"])
        self.assertEqual(current["history"], final["history"])

    def test_invalid_configuration_never_creates_or_marks_save(self):
        executable = self.bin / ("tor-server" + self.suffix)
        base_env = {k: v for k, v in os.environ.items() if k not in ("TOR_WIZARD_TOKEN", "TOR_SPECTATOR_TOKEN")}
        for flags, extra in [(["--wizard"], {}), (["--wizard"], {"TOR_WIZARD_TOKEN": TOKEN}), ([], {"TOR_WIZARD_TOKEN": WIZARD_TOKEN})]:
            result = subprocess.run([str(executable), "--listen", "127.0.0.1:0", "--save", str(self.save), *flags], env={**base_env, "TOR_SERVER_TOKEN": TOKEN, **extra}, capture_output=True, text=True, timeout=15)
            self.assertNotEqual(result.returncode, 0)
            self.assertFalse(self.save.exists())
            self.assertNotIn(TOKEN, result.stdout + result.stderr)


if __name__ == "__main__":
    unittest.main()
