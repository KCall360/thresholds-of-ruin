"""Offline validation and ordinary package startup through actual executables."""
import json
import os
import re
import shutil
import subprocess
import unittest

from process_harness import ProcessTestCase, Process, ROOT, TOKEN


class ScenarioPackageProcesses(ProcessTestCase):
    def validation_failure(self, package):
        before = {p.relative_to(package): p.read_bytes()
                  for p in package.rglob("*") if p.is_file()}
        result = subprocess.run(
            [self.bin / ("tor-scenario" + self.suffix), "validate", package],
            capture_output=True, text=True, encoding="utf-8", timeout=15)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(before, {p.relative_to(package): p.read_bytes()
                                 for p in package.rglob("*") if p.is_file()})
        error = json.loads(result.stderr)["error"]
        self.assertEqual(error["code"], "scenario_invalid")
        return error

    def test_region_and_generated_references_report_exact_source_values(self):
        cases = [
            ("portal", "two-room", "regions/1.toml", '"to" = "2/landing"', '"to" = "2/missing"', '"2/missing"', "portal anchor"),
            ("known-identity", "tests/items", "scenario.toml", 'known_identities = ["healing"]', 'known_identities = ["healing", "missing"]', '"missing"', "initial item identity"),
            ("objective-anchor", "first-dungeon", "scenario.toml", 'objective = { anchor = "1/exit"', 'objective = { anchor = "1/missing"', '"1/missing"', "objective anchor"),
            ("objective-item", "first-dungeon", "scenario.toml", 'item = 100', 'item = 999', '999', "objective"),
            ("zone", "two-room", "regions/1.toml", 'zone = "entry"', 'zone = "missing"', '"missing"', "zone"),
            ("generated-actor", "tests/generated-filler", "regions/2.toml", 'archetypes = ["rat"]', 'archetypes = ["rat", "missing"]', '"missing"', "archetype"),
            ("generated-item", "tests/generated-filler", "regions/2.toml", 'archetypes = ["coin"]', 'archetypes = ["coin", "missing"]', '"missing"', "archetype"),
            ("generated-ai", "tests/generated-filler", "regions/2.toml", 'ai = "wander"', 'ai = "missing"', '"missing"', "ai profile"),
            ("carrier", "tests/items", "regions/1.toml", 'id = 10, at = [1, 1, 0]', 'id = 10, at = [1, 1, 0], carried_by = 999', '999', "inventory owner"),
        ]
        for name, fixture, source, old, new, expected, diagnostic in cases:
            with self.subTest(reference=name):
                package = self.directory / name
                shutil.copytree(ROOT / "scenarios" / fixture, package)
                path = package / source
                text = path.read_text(encoding="utf-8")
                self.assertEqual(text.count(old), 1)
                text = '# Écho: "missing" and 999 are decoys.\n' + text.replace(old, new)
                path.write_bytes(text.encode("utf-8"))
                reference_line = next(line for line in text.splitlines() if new in line)
                line = text.splitlines().index(reference_line) + 1
                column = reference_line.index(new) + new.index(expected) + 1
                error = self.validation_failure(package)
                self.assertIn(diagnostic, error["message"].lower())
                self.assertIn(f"{source}:{line}:{column}", error["message"])

    def test_missing_identity_name_reports_its_declaration_without_blaming_a_valid_pool(self):
        package = self.directory / "missing-identity-name"
        shutil.copytree(ROOT / "scenarios/tests/items", package)
        manifest = package / "scenario.toml"
        text = manifest.read_text(encoding="utf-8")
        self.assertIn('name = "potion of healing"', text)
        manifest.write_bytes(text.replace('name = "potion of healing"', '# name is deliberately absent').encode("utf-8"))
        error = self.validation_failure(package)
        self.assertEqual(error["message"], 'scenario.toml: archetype "healing": Missing identity name for appearance pool')

    def test_manifest_references_report_the_exact_character_or_named_archetype_field(self):
        cases = [
            ("character-anchor", "two-room", '"anchor" = "1/start"',
             '"anchor" = "1/missing"', '"anchor" = ', "character 1", "anchor"),
            ("appearance-pool", "tests/items", 'appearance_pool = "potions"',
             'appearance_pool = "missing"', 'appearance_pool = ', "healing", "appearance pool"),
        ]
        for name, fixture, old, new, field, declaration, diagnostic in cases:
            with self.subTest(reference=name):
                package = self.directory / name
                shutil.copytree(ROOT / "scenarios" / fixture, package)
                manifest = package / "scenario.toml"
                text = manifest.read_text(encoding="utf-8")
                self.assertIn(old, text)
                text = '# Écho: "1/missing" and "missing" are decoys.\n' + text.replace(old, new)
                manifest.write_bytes(text.encode("utf-8"))
                reference_line = next(line for line in text.splitlines() if new in line)
                line = text.splitlines().index(reference_line) + 1
                column = reference_line.index(new) + len(field) + 1
                error = self.validation_failure(package)
                self.assertIn(diagnostic, error["message"].lower())
                self.assertIn(declaration, error["message"].lower())
                self.assertIn(f"scenario.toml:{line}:{column}", error["message"])

    def test_semantic_reference_diagnostic_locates_the_field_among_repeated_values(self):
        package = self.directory / "source-locations"
        shutil.copytree(ROOT / "scenarios/two-room", package)
        region = package / "regions/1.toml"
        actor = (' { id = 3, at = [3, 1, 0], controller = "ai", '
                 'combat = { name = "Gárd", max_hp = 41 }, ai = "missing", '
                 'body = { cells = [[0, 0, 0]], eye = [1, 0, 0], mass = 91 } },')
        text = region.read_text(encoding="utf-8") + (
            '\n# ai = "missing" is a decoy, not the failing reference.\n'
            'actors = [\n'
            ' { id = 2, at = [2, 1, 0], controller = "external", '
            'combat = { name = "missing", max_hp = 41 } },\n'
            + actor + '\n]\n')
        region.write_bytes(text.encode("utf-8"))
        line = text.splitlines().index(actor) + 1
        column = actor.index('ai = "missing"') + len('ai = ') + 1
        before = {p.relative_to(package): p.read_bytes() for p in package.rglob("*") if p.is_file()}
        result = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                capture_output=True, text=True, encoding="utf-8", timeout=15)
        self.assertNotEqual(result.returncode, 0)
        error = json.loads(result.stderr)["error"]
        self.assertEqual(error["code"], "scenario_invalid")
        self.assertIn('Unknown actor AI profile "missing"', error["message"])
        self.assertIn("actor 3", error["message"])
        self.assertIn(f"regions/1.toml:{line}:{column}", error["message"])
        self.assertNotIn("body eye", error["message"], "reference validation keeps its precedence")
        self.assertEqual(before, {p.relative_to(package): p.read_bytes()
                                 for p in package.rglob("*") if p.is_file()})

    def test_failed_lazy_generated_declaration_keeps_its_source_diagnostic(self):
        package = self.directory / "infeasible-declaration"
        shutil.copytree(ROOT / "scenarios/tests/generated-filler", package)
        region = package / "regions/2.toml"
        text = region.read_text().replace("size = [24, 12, 1]", "size = [24, 1, 1]")
        text = text.replace("[0, 6, 0]", "[0, 0, 0]").replace("[23, 6, 0]", "[23, 0, 0]")
        text = text.replace("rooms = [3, 6]", "rooms = [1, 1]")
        text = text.replace("count = [1, 3]", "count = [10, 10]")
        text = "\n".join(line for line in text.splitlines() if not line.startswith("items =")) + "\n"
        region.write_bytes(text.encode())
        (package / "validation.json").unlink()
        before = {p.relative_to(package): p.read_bytes() for p in package.rglob("*") if p.is_file()}
        env = {key: value for key, value in os.environ.items()
               if key not in ["TOR_SPECTATOR_TOKEN", "TOR_WIZARD_TOKEN"]}
        result = subprocess.run(
            [self.bin / ("tor-server" + self.suffix), "--scenario", package,
             "--allow-unvalidated", "--seed", "42", "--listen", "127.0.0.1:0", "--save", self.save],
            env={**env, "TOR_SERVER_TOKEN": TOKEN}, capture_output=True, text=True, timeout=15)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("InvalidAction", result.stderr)
        self.assertIn("Region 2", result.stderr)
        self.assertIn("capacity", result.stderr)
        self.assertNotIn("StorageFailure", result.stderr)
        self.assertFalse(self.save.exists(), "failed startup must not create a save")
        self.assertEqual(before, {p.relative_to(package): p.read_bytes()
                                 for p in package.rglob("*") if p.is_file()})

    def test_lf_checkout_packages_start_and_resume_without_revalidation(self):
        for name in ["two-room", "tests/generated-filler"]:
            with self.subTest(package=name):
                package = self.directory / name.replace("/", "-")
                shutil.copytree(ROOT / "scenarios" / name, package)
                for path in package.rglob("*"):
                    if path.is_file() and path.suffix in [".toml", ".json"]:
                        path.write_bytes(path.read_bytes().replace(b"\r\n", b"\n"))
                before = {p.relative_to(package): p.read_bytes() for p in package.rglob("*") if p.is_file()}
                self.save = self.directory / (name.replace("/", "-") + ".db")
                server = self.server(scenario=package)
                player, initial = self.client()
                self.flush_save()
                player.stop()
                server.stop()
                self.assertEqual(before, {p.relative_to(package): p.read_bytes()
                                         for p in package.rglob("*") if p.is_file()})
                package.rename(package.with_name(package.name + "-unavailable"))
                server = self.server(seed=None)
                player, restored = self.client()
                self.assertEqual(restored["state"], initial["state"])
                player.stop()
                server.stop()

    def test_validator_rejects_oversized_generated_counts_without_panicking_or_rewriting(self):
        package = self.directory / "oversized-pool"
        shutil.copytree(ROOT / "scenarios/tests/generated-filler", package)
        region = package / "regions/2.toml"
        text = region.read_text()
        self.assertIn("count = [1, 3]", text)
        region.write_text(text.replace("count = [1, 3]", "count = [4294967295, 4294967295]"))
        before = {p.relative_to(package): p.read_bytes() for p in package.rglob("*") if p.is_file()}
        result = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                capture_output=True, text=True, timeout=15)
        self.assertNotEqual(result.returncode, 0)
        self.assertNotIn("panicked at", result.stderr)
        error = json.loads(result.stderr)["error"]
        self.assertEqual(error["code"], "scenario_invalid")
        self.assertIn("Region 2", error["message"])
        self.assertIn("pools", error["message"])
        self.assertEqual({p.relative_to(package): p.read_bytes() for p in package.rglob("*") if p.is_file()}, before)

    def test_generated_comments_preserve_real_client_content_and_pinned_restart(self):
        observations, hashes, cell_keys = [], [], []
        self_targets = []
        for name, comment in [("original", ""), ("annotated", "# Author notes do not reroll content.\n")]:
            package = self.directory / name
            shutil.copytree(ROOT / "scenarios/tests/generated-filler", package)
            manifest = package / "scenario.toml"
            manifest.write_text(manifest.read_text().replace('anchor = "1/start"', 'anchor = "2/west"'))
            for region in [2, 3]:
                path = package / f"regions/{region}.toml"
                text = path.read_text().replace("count = [1, 3]", "count = [0, 0]")
                path.write_text((comment if region == 2 else "") + text)
            validated = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                       capture_output=True, text=True, timeout=15)
            self.assertEqual(validated.returncode, 0, validated.stderr)
            index = json.loads((package / "index.json").read_text())
            hashes.append(next(region["hash"] for region in index["regions"] if region["id"] == 2))
            self.save = self.directory / f"{name}.db"
            server = self.server(scenario=package, seed=42)
            player, initial = self.client()
            observation = initial["state"]["observation"]
            self_targets.append(observation["self_target"])
            cell_keys.append([cell["key"] for cell in observation["visible_cells"]])
            # Fresh saves have independent privacy salts; compare generated
            # content across them, but retain exact token equality on restart.
            content = {**observation, "visible_cells": [
                {key: value for key, value in cell.items() if key != "key"}
                for cell in observation["visible_cells"]]}
            # Compare the complete disclosed graph up to save-scoped identity:
            # aliases, occurrence order and every content field remain checked.
            targets = {}

            def normalize(value):
                if isinstance(value, dict):
                    return {key: normalize(child) for key, child in value.items()}
                if isinstance(value, list):
                    return [normalize(child) for child in value]
                if isinstance(value, str) and re.fullmatch(r"[aid]_[0-9a-f]{64}", value):
                    return targets.setdefault(value, f"{value[:2]}reference-{len(targets)}")
                return value

            observations.append(normalize(content))
            self.flush_save()
            player.stop()
            server.stop()
            package.rename(self.directory / f"{name}-unavailable")
            server = self.server(seed=None)
            player, restored = self.client()
            self.assertEqual(restored["state"], initial["state"])
            player.stop()
            server.stop()
        self.assertNotEqual(*hashes)
        self.assertNotEqual(*cell_keys)
        self.assertNotEqual(*self_targets)
        self.assertEqual(*observations)

    def test_validator_reports_construction_references_without_rewriting_package(self):
        cases = [
            ('actor-ai', 'regions/1.toml',
             '\nactors = [{ id = 9, at = [3,1,0], controller = "ai", ai = "missing", combat = { max_hp = 41 }, body = { cells = [[0,0,0]], eye = [1,0,0], mass = 91 } }]\n',
             'regions/1.toml: region 1, actor 9: Unknown actor AI profile "missing"'),
            ('actor-body', 'regions/1.toml',
             '\nactors = [{ id = 9, at = [3,1,0], body = { cells = [[0,0,0]], eye = [1,0,0], mass = 91 } }]\n',
             'regions/1.toml: region 1, actor 9: Actor body eye must be one of its cells'),
            ('actor-archetype', 'regions/1.toml',
             '\nactors = [{ id = 9, at = [3,1,0], archetype = "missing" }]\n',
             'regions/1.toml: region 1, actor 9: Unknown archetype missing'),
            ('item-archetype', 'regions/2.toml',
             '\nitems = [{ id = 9, at = [1,1,0], archetype = "missing" }]\n',
             'regions/2.toml: region 2, item 9: Unknown archetype missing'),
        ]
        for name, source, edit, message in cases:
            with self.subTest(reference=name):
                package = self.directory / name
                shutil.copytree(ROOT / 'scenarios/two-room', package)
                path = package / source
                text = path.read_text()
                if name == 'item-archetype':
                    text = '\n'.join(line for line in text.splitlines() if not line.startswith('items =')) + '\n'
                path.write_text(text + edit)
                if name != 'actor-body':
                    field = 'ai' if name == 'actor-ai' else 'archetype'
                    offset = text.count('\n') + edit[:edit.index(f'{field} = "missing"')].count('\n') + 1
                    column = edit.splitlines()[1].index(f'{field} = "missing"') + len(f'{field} = ') + 1
                    message = message.replace(f'{source}: ', f'{source}:{offset}:{column}: ', 1)
                before = {p.relative_to(package): p.read_bytes() for p in package.rglob('*') if p.is_file()}
                result = subprocess.run([self.bin / ('tor-scenario' + self.suffix), 'validate', package],
                                        capture_output=True, text=True, timeout=15)
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(json.loads(result.stderr)['error'], {'code': 'scenario_invalid', 'message': message})
                self.assertEqual({p.relative_to(package): p.read_bytes() for p in package.rglob('*') if p.is_file()}, before)

    def test_validator_identifies_invalid_declarations_without_rewriting_package(self):
        cases = [
            ('faction', 'scenario.toml', '\nfactions = { guard = ["missing"] }\n',
             'scenario.toml: faction "guard": Invalid faction relationship'),
            ('profile', 'scenario.toml', '\nai_profiles = { guard = { flee_percent = 101 } }\n',
             'scenario.toml: AI profile "guard": Invalid AI profile'),
            ('actor', 'regions/1.toml', '\nactors = [{ id = 9, at = [1,1,0], controller = "bad" }]\n',
             'regions/1.toml: region 1, actor 9: Invalid actor controller'),
            ('combat', 'regions/1.toml', '\nactors = [{ id = 9, at = [1,1,0], combat = { max_hp = 0 } }]\n',
             'regions/1.toml: region 1, actor 9: Invalid combat attributes or faction'),
        ]
        for name, source, edit, message in cases:
            with self.subTest(declaration=name):
                package = self.directory / name
                shutil.copytree(ROOT / 'scenarios/two-room', package)
                path = package / source
                path.write_text(path.read_text() + edit)
                before = {p.relative_to(package): p.read_bytes() for p in package.rglob('*') if p.is_file()}
                result = subprocess.run([self.bin / ('tor-scenario' + self.suffix), 'validate', package],
                                        capture_output=True, text=True, timeout=15)
                self.assertNotEqual(result.returncode, 0)
                error = json.loads(result.stderr)['error']
                self.assertEqual(error, {'code': 'scenario_invalid', 'message': message})
                after = {p.relative_to(package): p.read_bytes() for p in package.rglob('*') if p.is_file()}
                self.assertEqual(after, before)

    def test_compiled_inheritance_and_overrides_survive_save_restart(self):
        package = self.directory / 'compiled-package'
        shutil.copytree(ROOT / 'scenarios/two-room', package)
        manifest = package / 'scenario.toml'
        text = manifest.read_text()
        text = text.replace('"token" = { "name" = "copper token" }',
                            '"token" = { name = "compiled coin", stackable = true, '
                            'properties = { quality = "fine" } }')
        text = text.replace('"turn_ticks" = 100, body =',
                            '"turn_ticks" = 100, combat = { name = "compiler hero", max_hp = 41 }, body =')
        manifest.write_text(text.replace('mass = 80', 'mass = 91'))
        region = package / 'regions/1.toml'
        text = region.read_text()
        start = text.index('items = ')
        region.write_text(text[:start] + '''items = [
            { id = 1, at = [1, 1, 0], archetype = "token", quantity = 3, properties = { quality = "ordinary" } },
            { id = 3, at = [1, 1, 0], archetype = "token", name = "named gift", stackable = false }
        ]
        ''')
        validated = subprocess.run([self.bin / ('tor-scenario' + self.suffix), 'validate', package],
                                   capture_output=True, text=True, timeout=15)
        self.assertEqual(validated.returncode, 0, validated.stderr)
        server = self.server(scenario=package)
        player, initial = self.client()
        observation = initial['state']['observation']
        self.assertEqual(observation['combat']['max_hp'], 41)
        items = {entry['item']['name']: entry['item'] for entry in observation['ground_items']}
        self.assertEqual(items['compiled coin']['quantity'], '3')
        self.assertEqual(items['named gift']['quantity'], '1')
        taken = self.act(player, {'type': 'take', 'item': items['compiled coin']['id'], 'quantity': '2'})
        self.assertIsNone(taken['error'])
        inventory = taken['state']['observation']['inventory']
        self.assertEqual([(item['name'], item['quantity']) for item in inventory], [('compiled coin', '2')])
        self.flush_save()
        player.stop()
        server.stop()
        self.server(scenario=package)
        resumed, restored = self.client()
        self.assertEqual(restored['state'], taken['state'])
        self.assertEqual(restored['history'], taken['history'])
        dropped = self.act(resumed, {'type': 'drop', 'item': inventory[0]['id'], 'quantity': '1'})
        self.assertIsNone(dropped['error'])
        self.assertEqual(dropped['state']['observation']['inventory'][0]['quantity'], '1')

    def test_validator_stale_rejection_and_ordinary_startup(self):
        package = self.directory / 'package'
        shutil.copytree(ROOT / 'scenarios/two-room', package)
        env = {k:v for k,v in os.environ.items() if k not in ('TOR_WIZARD_TOKEN','TOR_SPECTATOR_TOKEN')}
        env['TOR_SERVER_TOKEN'] = TOKEN
        def start(*args):
            return subprocess.run([self.bin / ('tor-server' + self.suffix), *args,
                                   '--save', self.directory / 'game.db'], env=env,
                                  capture_output=True, text=True, timeout=15)
        # An edited manifest is stale when the package loads; an edited region
        # file when its region is built, which here is at the start.
        manifest = package / 'scenario.toml'
        original = manifest.read_bytes()
        manifest.write_bytes(original + b'\n# author revision\n')
        result = start('--scenario', package)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('unvalidated or stale', result.stderr)
        manifest.write_bytes(original)
        path = package / 'regions' / '1.toml'
        path.write_text(path.read_text() + '\n# author revision\n')
        result = start('--scenario', package)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('changed since', result.stderr)
        self.assertFalse((self.directory / 'game.db').exists())
        result = subprocess.run([self.bin / ('tor-scenario' + self.suffix), 'validate', package],
                                capture_output=True, text=True, timeout=15)
        self.assertEqual(result.returncode, 0, result.stderr)
        certificate = json.loads(result.stdout)
        self.assertEqual(certificate['regions'], 2)
        self.assertIn('all authored regions', certificate['coverage'])
        server = Process(self.bin / ('tor-server' + self.suffix),
                                 ['--listen','127.0.0.1:0','--scenario',package,'--save',self.directory/'game.db'])
        self.addCleanup(server.stop)
        address = json.loads(server.until(lambda line:line.startswith('{')))['address']
        client = Process(self.bin / ('tor-client-headless' + self.suffix), ['--connect', address])
        self.addCleanup(client.stop)
        client.until(lambda line: '"type":"ready"' in line)
        client.child.stdin.write(json.dumps({"type": "act", "action": {"type": "wait"}}) + "\n")
        client.child.stdin.flush()
        ready = client.until(lambda line: '"type":"ready"' in line)
        self.assertIn('"type":"ready"', ready)
        client.stop(); server.stop()
        # Both regions were built at the start, so the save holds both region
        # files and resumes with the package gone.
        moved = self.directory / 'moved'
        package.rename(moved)
        server = Process(self.bin / ('tor-server' + self.suffix),
                                 ['--listen','127.0.0.1:0','--save',self.directory/'game.db'])
        self.addCleanup(server.stop)
        address = json.loads(server.until(lambda line:line.startswith('{')))['address']
        client = Process(self.bin / ('tor-client-headless' + self.suffix), ['--connect', address])
        self.addCleanup(client.stop)
        client.until(lambda line: '"type":"ready"' in line)
        client.child.stdin.write(json.dumps({"type": "act", "action": {"type": "wait"}}) + "\n")
        client.child.stdin.flush()
        ready = client.until(lambda line: '"type":"ready"' in line)
        self.assertIn('"type":"ready"', ready)
        client.stop(); server.stop()
        moved.rename(package)
        manifest = package / 'scenario.toml'
        manifest.write_text(manifest.read_text().replace('1/start', '1/missing'))
        invalid = subprocess.run([self.bin / ('tor-scenario' + self.suffix), 'validate', package],
                                 capture_output=True, text=True, timeout=15)
        self.assertNotEqual(invalid.returncode, 0)
        error = json.loads(invalid.stderr)['error']
        self.assertEqual(error['code'], 'scenario_invalid')
        self.assertIn('anchor', error['message'])


if __name__ == '__main__':
    unittest.main()
