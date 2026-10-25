"""The authoritative compiler resolves local catalogs and launches their content."""
import json
import shutil
import subprocess

from process_harness import ProcessTestCase, ROOT


class ScenarioSourceProcesses(ProcessTestCase):
    def compiler(self, *arguments, source=None):
        return subprocess.run([self.bin / ("tor-scenario" + self.suffix), *arguments],
                              input=source, capture_output=True, text=True,
                              encoding="utf-8", timeout=30)

    def package(self, name="module-package"):
        package = self.directory / name
        shutil.copytree(ROOT / "scenarios/two-room", package)
        manifest = package / "scenario.toml"
        text = manifest.read_text(encoding="utf-8")
        text = text.replace('archetypes = { "token" = { "name" = "copper token" }, "tablet" = { "name" = "stone tablet" } }',
                            'modules = ["catalog/items.toml"]')
        manifest.write_text(text, encoding="utf-8")
        (package / "catalog").mkdir()
        (package / "catalog/items.toml").write_text(
            'namespace = "demo"\n[items."demo.token"]\nname = "copper token"\n'
            '[items."demo.tablet"]\nname = "stone tablet"\n', encoding="utf-8")
        for region in (package / "regions").glob("*.toml"):
            region.write_text(region.read_text(encoding="utf-8").replace('"token"', '"demo.token"')
                              .replace('"tablet"', '"demo.tablet"'), encoding="utf-8")
        return package

    def test_local_catalog_inspection_check_and_play(self):
        package = self.package()
        before = {p.relative_to(package): p.read_bytes() for p in package.rglob("*") if p.is_file()}
        inspected = self.compiler("inspect", str(package))
        self.assertEqual(inspected.returncode, 0, inspected.stderr)
        resolved = json.loads(inspected.stdout)
        self.assertEqual(resolved["provenance"]["archetypes.demo.token"], "catalog/items.toml")
        self.assertIn("catalog/items.toml", resolved["source_hashes"])
        self.assertNotIn("modules", resolved["manifest"])
        checked = self.compiler("check", str(package))
        self.assertEqual(checked.returncode, 0, checked.stderr)
        self.assertEqual(before, {p.relative_to(package): p.read_bytes() for p in package.rglob("*") if p.is_file()})
        validated = self.compiler("validate", str(package))
        self.assertEqual(validated.returncode, 0, validated.stderr)
        self.assertIn("catalog/items.toml", json.loads(validated.stdout)["files"])
        server = self.server(scenario=package)
        player, ready = self.client()
        self.assertIsNone(ready.get("error"))
        self.flush_save()
        player.stop()
        server.stop()
        # Restore consumes the pinned merged content, not live authoring modules.
        shutil.rmtree(package / "catalog")
        self.server()
        _, restored = self.client()
        self.assertEqual(restored["state"], ready["state"])
        self.assertEqual(restored["history"], ready["history"])

    def test_duplicate_namespace_and_include_failures_are_atomic(self):
        for name, content in (
                ("duplicate", 'namespace = "demo"\n[items."demo.token"]\nname = "duplicate"\n'),
                ("foreign", 'namespace = "other"\n[archetypes."demo.new"]\nname = "foreign"\n'),
                ("unknown", 'namespace = "demo"\nscript = "arbitrary.py"\n')):
            with self.subTest(name=name):
                package = self.package(name)
                root = package / "scenario.toml"
                root.write_text(root.read_text(encoding="utf-8").replace(
                    '["catalog/items.toml"]', '["catalog/items.toml", "other.toml"]'), encoding="utf-8")
                (package / "other.toml").write_text(content, encoding="utf-8")
                before = {p.relative_to(package): p.read_bytes() for p in package.rglob("*") if p.is_file()}
                checked = self.compiler("check", str(package))
                self.assertNotEqual(checked.returncode, 0)
                self.assertIn("other.toml", json.loads(checked.stderr)["error"]["message"])
                self.assertEqual(before, {p.relative_to(package): p.read_bytes() for p in package.rglob("*") if p.is_file()})

    def test_topology_module_checks_and_starts_a_mixed_world(self):
        from arena_matrix import toml_source
        import tomllib
        package = self.package("mixed-modules")
        manifest = package / "scenario.toml"
        manifest.write_text(manifest.read_text(encoding="utf-8").replace(
            '["catalog/items.toml"]', '["catalog/items.toml", "world.toml"]'), encoding="utf-8")
        region = package / "regions/1.toml"
        data = tomllib.loads(region.read_text(encoding="utf-8"))
        data["anchors"]["descent"] = data["anchors"]["start"]
        region.write_text(toml_source(data), encoding="utf-8")
        (package / "world.toml").write_text('''namespace = "caves"
[generation_recipes."caves.rooms"]
kind = "grid_rooms"
version = 1
room_width = [4, 6]
room_height = [3, 5]
extra_connections = [0, 1]
[procedural_worlds."caves.depths"]
kind = "grid_chain"
version = 1
base_region = 100
grid = [1, 1]
size = [12, 10, 2]
recipe = "caves.rooms"
domain = {kind = "seeded_finite", min = 2, max = 6}
[stair_pairs."caves.entry"]
upper = "1/descent"
lower = "100/up"
''', encoding="utf-8")
        checked = self.compiler("check", str(package))
        self.assertEqual(checked.returncode, 0, checked.stderr)
        validated = self.compiler("validate", str(package))
        self.assertEqual(validated.returncode, 0, validated.stderr)
        self.assertFalse((package / "regions/100.toml").exists())
        self.server(scenario=package)
        _, ready = self.client()
        self.assertIsNone(ready.get("error"))

    def test_module_semantic_failures_name_their_actual_definition_source(self):
        declarations = (
            ('[items."demo.bad"]\nname="bad"\nappearance_pool="missing"\n', 'archetype "demo.bad"'),
            ('[creatures.templates."demo.bad"]\npriority=0\nkind="missing_type"\n', 'template "demo.bad"'),
            ('[actors."demo.bad"]\ncreature={species="missing",name="Bad",faction="neutral",binding="intellect",starting_skills=["heavy_weaponry","athletics"],restoration="authored", nutrition = { kind = "exempt" },hit_dice=[{source="racial"}]}\n', 'archetype "demo.bad"'),
        )
        for index, (declaration, identity) in enumerate(declarations):
            with self.subTest(identity=identity):
                package = self.package("bad-module-" + str(index))
                module = package / "catalog/items.toml"
                module.write_text(module.read_text(encoding="utf-8") + declaration, encoding="utf-8")
                result = self.compiler("check", str(package))
                self.assertNotEqual(result.returncode, 0)
                message = json.loads(result.stderr)["error"]["message"]
                self.assertIn("catalog/items.toml", message)
                self.assertIn(identity, message)

    def test_rules_resolution_uses_the_same_stable_id_override_engine(self):
        result = self.compiler("resolve-rules", source='import = "interactions-v53"\n'
                               'overrides = [{op="set", path=["resources","mana","recovery_period"], value=997}]\n')
        self.assertEqual(result.returncode, 0, result.stderr)
        resolved = json.loads(result.stdout)
        mana = next(value for value in resolved["document"]["resources"] if value["resource"] == "mana")
        self.assertEqual(mana["recovery_period"], 997)
        self.assertIn("interactions-v53", resolved["dependencies"])
