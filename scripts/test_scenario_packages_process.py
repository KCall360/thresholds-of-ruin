"""Offline validation and ordinary package startup through actual executables."""
import json
import os
import shutil
import subprocess
import unittest

from process_harness import ProcessTestCase, Process, ROOT, TOKEN


class ScenarioPackageProcesses(ProcessTestCase):
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
        self.assertEqual(items['compiled coin']['quantity'], 3)
        self.assertEqual(items['named gift']['quantity'], 1)
        taken = self.act(player, {'type': 'take', 'item': items['compiled coin']['id'], 'quantity': 2})
        self.assertIsNone(taken['error'])
        inventory = taken['state']['observation']['inventory']
        self.assertEqual([(item['name'], item['quantity']) for item in inventory], [('compiled coin', 2)])
        self.flush_save()
        player.stop()
        server.stop()
        self.server(scenario=package)
        resumed, restored = self.client()
        self.assertEqual(restored['state'], taken['state'])
        self.assertEqual(restored['history'], taken['history'])
        dropped = self.act(resumed, {'type': 'drop', 'item': inventory[0]['id'], 'quantity': 1})
        self.assertIsNone(dropped['error'])
        self.assertEqual(dropped['state']['observation']['inventory'][0]['quantity'], 1)

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
