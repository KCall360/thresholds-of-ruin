"""The compile/model consumer needs native headers, not a graphical test desktop."""
import contextlib
import importlib
import io
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[1]


def consumer_job():
    workflow = (ROOT / '.github/workflows/ci.yml').read_text()
    return re.search(r'^  selected_consumer:\n(.*?)(?=^  \w|\Z)', workflow, re.M | re.S)[1]


class ConsumerDependencies(unittest.TestCase):
    def test_consumer_keeps_actual_compile_and_model_contracts(self):
        job = consumer_job()
        self.assertIn('cargo check --manifest-path crates/client-3d/Cargo.toml --bin tor-client-3d --locked', job)
        self.assertIn('cargo test --manifest-path crates/client-3d/Cargo.toml -p tor-client-3d-model --locked', job)
        self.assertIn('python scripts/ci_selected.py run --lane linux-3d-consumer', job)

    def test_workflow_calls_consumer_setup_before_either_cargo_lane(self):
        job = consumer_job()
        command = 'run: python scripts/ci_consumer_dependencies.py'
        self.assertIn(command, job)
        self.assertLess(job.index(command), job.index('Execute fresh selected contract assurance'))
        self.assertLess(job.index(command), job.index('Execute fresh exhaustive consumer compatibility'))
        self.assertNotIn('apt-get', job)
        # Other native owners still need their real graphical prerequisites.
        workflow = (ROOT / '.github/workflows/ci.yml').read_text()
        self.assertIn('install -y libx11-dev libxcursor-dev libxrandr-dev xvfb xauth xdotool', workflow)

    def test_exact_bounded_official_package_commands(self):
        helper = importlib.import_module('ci_consumer_dependencies')
        options = ['-o', 'Acquire::Retries=2', '-o', 'Acquire::http::Timeout=20',
                   '-o', 'Acquire::https::Timeout=20']
        self.assertEqual(helper.commands(), [
            ['timeout', '--kill-after=10s', '2m', 'sudo', 'apt-get', *options,
             '-o', 'APT::Update::Error-Mode=any', 'update'],
            ['timeout', '--kill-after=10s', '10m', 'sudo', 'apt-get', *options,
             'install', '--no-install-recommends', '-y', 'pkg-config', 'libasound2-dev', 'libudev-dev'],
            ['timeout', '--kill-after=5s', '15s', 'pkg-config', '--print-errors', '--exists', 'alsa', 'libudev'],
        ])

    def exercise(self, failing_stage=None, code=100):
        helper = importlib.import_module('ci_consumer_dependencies')
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            script = directory / 'benign-stage.py'
            record = directory / 'calls.txt'
            script.write_text('import pathlib,sys\n'
                              'with pathlib.Path(sys.argv[1]).open("a") as log: log.write(sys.argv[2]+"\\n")\n'
                              'sys.exit(int(sys.argv[3]))\n')
            commands = [[sys.executable, str(script), str(record), name,
                         str(code if name == failing_stage else 0)] for name in ('update', 'install', 'probe')]
            with mock.patch.object(helper, 'commands', return_value=commands), contextlib.redirect_stdout(io.StringIO()):
                if failing_stage is None:
                    helper.main()
                else:
                    with self.assertRaises(subprocess.CalledProcessError) as caught:
                        helper.main()
                    self.assertEqual(caught.exception.returncode, code)
                    self.assertEqual(caught.exception.cmd, commands[['update', 'install', 'probe'].index(failing_stage)])
            return record.read_text().splitlines()

    def test_actual_runner_success_and_each_failure_stop_before_next_stage(self):
        self.assertEqual(self.exercise(), ['update', 'install', 'probe'])
        for index, name in enumerate(('update', 'install', 'probe')):
            with self.subTest(stage=name):
                self.assertEqual(self.exercise(name), ['update', 'install', 'probe'][:index + 1])

    def test_acquisition_timeout_is_not_retried_or_treated_as_success(self):
        self.assertEqual(self.exercise('install', 124), ['update', 'install'])
