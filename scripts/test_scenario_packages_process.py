"""Offline validation and ordinary package startup through actual executables."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import unittest

import test_text_process as support


class ScenarioPackageProcesses(unittest.TestCase):
    setUpClass = classmethod(support.TextProcesses.setUpClass.__func__)

    def test_validator_stale_rejection_and_ordinary_startup(self):
        directory = support.ProcessTestDirectory()
        self.addCleanup(directory.cleanup)
        package = Path(directory.name) / 'package'
        shutil.copytree(support.ROOT / 'scenarios/two-room', package)
        env = {k:v for k,v in os.environ.items() if k not in ('TOR_WIZARD_TOKEN','TOR_SPECTATOR_TOKEN')}
        env['TOR_SERVER_TOKEN'] = support.TOKEN
        def start(*args):
            return subprocess.run([self.bin / ('tor-server' + self.suffix), *args,
                                   '--save', Path(directory.name) / 'game.db'], env=env,
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
        self.assertFalse((Path(directory.name) / 'game.db').exists())
        result = subprocess.run([self.bin / ('tor-scenario' + self.suffix), 'validate', package],
                                capture_output=True, text=True, timeout=15)
        self.assertEqual(result.returncode, 0, result.stderr)
        certificate = json.loads(result.stdout)
        self.assertEqual(certificate['regions'], 2)
        self.assertIn('all authored regions', certificate['coverage'])
        server = support.Process(self.bin / ('tor-server' + self.suffix),
                                 ['--listen','127.0.0.1:0','--scenario',package,'--save',Path(directory.name)/'game.db'])
        self.addCleanup(server.stop)
        address = json.loads(server.until(lambda line:line.startswith('{')))['address']
        client = support.Process(self.bin / ('tor-client-headless' + self.suffix), ['--connect', address])
        self.addCleanup(client.stop)
        client.until(lambda line: '"type":"ready"' in line)
        client.child.stdin.write(json.dumps({"type": "act", "action": {"type": "wait"}}) + "\n")
        client.child.stdin.flush()
        ready = client.until(lambda line: '"type":"ready"' in line)
        self.assertIn('"type":"ready"', ready)
        client.stop(); server.stop()
        # Both regions were built at the start, so the save holds both region
        # files and resumes with the package gone.
        moved = Path(directory.name) / 'moved'
        package.rename(moved)
        server = support.Process(self.bin / ('tor-server' + self.suffix),
                                 ['--listen','127.0.0.1:0','--save',Path(directory.name)/'game.db'])
        self.addCleanup(server.stop)
        address = json.loads(server.until(lambda line:line.startswith('{')))['address']
        client = support.Process(self.bin / ('tor-client-headless' + self.suffix), ['--connect', address])
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
