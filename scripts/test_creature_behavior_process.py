"""Species-owned behavior through real server, headless and Text sessions."""
from scalar_wire import u64
import subprocess
import tomllib

from arena_matrix import toml_source
from process_harness import ProcessTestCase, WIZARD_TOKEN, creature_package


def observation(frame):
    return frame["state"]["observation"]


class CreatureBehaviorProcesses(ProcessTestCase):
    def package(self):
        package = creature_package(self, "typed-creature-behavior", abilities=())
        path = package / "scenario.toml"
        manifest = tomllib.loads(path.read_text(encoding="utf-8"))
        species = manifest["creatures"]["species"]["human"]
        species["behavior"] = {"disposition": "passive", "idle": "wait", "memory_ticks": 1000}
        # Guaranteed zero-damage contact still licenses ordinary provocation.
        species["melee"]["bonus"] = 1000
        species["melee"]["strikes"][0]["on_success"][0]["damage"]["components"][0]["amount"] = {
            "kind": "fixed", "amount": 0}
        manifest["ai_profiles"]["practice"] = {"care": False, "equipment": False, "loot": False,
                                                 "flee_percent": 0}
        path.write_text(toml_source(manifest), encoding="utf-8", newline="\n")
        # Keep this witness stationary: AI selection hints do not determine
        # interaction intent, and a counterstrike would create a second grudge.
        rules_path = package / "rules.toml"
        rules = tomllib.loads(rules_path.read_text(encoding="utf-8"))
        next(ability for ability in rules["abilities"] if ability["id"] == "basic_melee")["ai"]["hostile"] = False
        rules_path.write_text(toml_source(rules), encoding="utf-8", newline="\n")
        validated = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                   capture_output=True, text=True, encoding="utf-8", timeout=20)
        self.assertEqual(validated.returncode, 0, validated.stderr)
        return package

    def test_visible_zero_damage_contact_creates_private_engagement_and_survives_restart(self):
        package = self.package()
        server = self.server("--checkpoint-interval", 1, scenario=package, wizard=True)
        player, initial = self.client()
        observer, calm = self.client(WIZARD_TOKEN, observe=True, actor=2)
        text = self.launch("tor-client-text", ["--script", "--connect", self.address,
                           "--observe", "--actor", "2"], token=WIZARD_TOKEN)
        text.until(lambda line: line == "Ready.")
        target = next(actor["id"] for actor in observation(initial)["visible_actors"]
                      if actor["name"] == "practice target")
        initial_hp = u64(observation(calm)["combat"]["hp"])
        self.assertFalse(any(actor["hostile"] for actor in observation(calm)["combat"]["actors"]))
        self.act(player, {"type": "wait"})
        still_calm = self.request(observer, {"type": "snapshot"})
        self.assertFalse(any(actor["hostile"] for actor in observation(still_calm)["combat"]["actors"]))
        result = self.act(player, {"type": "use_ability", "id": "basic_melee",
                                  "invocation": {"kind": "actor_target", "target": target}})
        self.assertIsNone(result["error"])
        engaged = self.request(observer, {"type": "snapshot"})
        for _ in range(8):
            if any(actor["hostile"] for actor in observation(engaged)["combat"]["actors"]):
                break
            self.act(player, {"type": "wait"})
            engaged = self.request(observer, {"type": "snapshot"})
        self.assertTrue(any(actor["hostile"] for actor in observation(engaged)["combat"]["actors"]))
        self.assertEqual(u64(observation(engaged)["combat"]["hp"]), initial_hp)
        # Text presents the same disclosed resolved contact without inventing
        # an injury or exposing the target's private engagement record.
        contact = "The trainee used Attack on you."
        self.assertIn(contact, text.until(lambda line: line == contact))
        presented = text.command("look")
        self.assertIn(f"HP {initial_hp}/{u64(observation(calm)['combat']['max_hp'])}", presented)
        self.assertNotIn("memory_ticks", presented)
        self.assertNotIn("engagement", presented)
        # The target's private grudge must not reverse the requester's alliance.
        owner = self.request(player, {"type": "snapshot"})
        relation = next(actor for actor in observation(owner)["combat"]["actors"] if actor["actor"] == target)
        self.assertFalse(relation["hostile"])
        self.assertTrue(relation["allied"])
        self.assertIsNone(self.request(player, {"type": "save"})["error"])
        saved_tick = observation(engaged)["tick"]
        text.stop()
        observer.stop()
        player.stop()
        server.stop()
        self.server(scenario=package, wizard=True, seed=None)
        _, restored = self.client(WIZARD_TOKEN, observe=True, actor=2)
        self.assertEqual(observation(restored)["tick"], saved_tick)
        self.assertTrue(any(actor["hostile"] for actor in observation(restored)["combat"]["actors"]))
        self.assertEqual(u64(observation(restored)["combat"]["hp"]), initial_hp)

    def test_compiler_rejects_missing_interaction_and_missing_behavior_without_replacing_certificate(self):
        package = self.package()
        certificate = package / "validation.json"
        before = certificate.read_bytes()
        manifest = package / "scenario.toml"
        original_manifest = manifest.read_text(encoding="utf-8")
        document = tomllib.loads(original_manifest)
        del document["creatures"]["species"]["human"]["behavior"]
        manifest.write_text(toml_source(document), encoding="utf-8", newline="\n")
        result = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                capture_output=True, text=True, encoding="utf-8", timeout=20)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("behavior", result.stderr)
        self.assertEqual(certificate.read_bytes(), before)
        manifest.write_text(original_manifest, encoding="utf-8", newline="\n")
        rules_path = package / "rules.toml"
        rules = tomllib.loads(rules_path.read_text(encoding="utf-8"))
        del rules["abilities"][0]["interaction"]
        rules_path.write_text(toml_source(rules), encoding="utf-8", newline="\n")
        result = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                capture_output=True, text=True, encoding="utf-8", timeout=20)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("interaction", result.stderr)
        self.assertEqual(certificate.read_bytes(), before)
