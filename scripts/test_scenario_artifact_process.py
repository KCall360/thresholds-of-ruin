"""Bounded compiler inputs and atomic immutable artifact publication."""
import json
import os
import shutil
import subprocess

from process_harness import ProcessTestCase, ROOT


class ScenarioArtifactProcesses(ProcessTestCase):
    def compiler(self, *arguments, cwd=None):
        return subprocess.run([str(self.bin / ("tor-scenario" + self.suffix)), *arguments],
                              cwd=cwd, capture_output=True, text=True, encoding="utf-8", timeout=10)

    def package(self, name):
        path = self.directory / name
        shutil.copytree(ROOT / "scenarios/two-room", path, ignore=shutil.ignore_patterns(".compiled"))
        return path

    def test_non_regular_source_is_rejected_without_waiting_for_a_writer(self):
        package = self.directory / "special-input"
        package.mkdir()
        path = package / "scenario.toml"
        if hasattr(os, "mkfifo"):
            os.mkfifo(path)
        else:
            path.mkdir()
        result = self.compiler("check", str(package))
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("regular file", json.loads(result.stderr)["error"]["message"])

    def test_rejected_output_does_not_replace_other_validation_files(self):
        package = self.package("unsafe-output")
        (package / "index.json").write_text("sentinel", encoding="utf-8")
        (package / "validation.json").unlink()
        (package / "validation.json").mkdir()
        result = self.compiler("validate", str(package))
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual((package / "index.json").read_text(encoding="utf-8"), "sentinel")
        self.assertFalse((package / ".compiled").exists())

    def test_bare_relative_export_is_portable_and_never_overwrites(self):
        package = self.package("source")
        result = self.compiler("validate", str(package))
        self.assertEqual(result.returncode, 0, result.stderr)
        result = self.compiler("export", str(package), "portable", cwd=self.directory)
        self.assertEqual(result.returncode, 0, result.stderr)
        portable = self.directory / "portable"
        self.assertTrue((portable / "package.json").is_file())
        self.assertFalse((portable / "scenario.toml").exists())
        snapshot = {path.relative_to(portable): path.read_bytes()
                    for path in portable.rglob("*") if path.is_file()}
        again = self.compiler("export", str(package), "portable", cwd=self.directory)
        self.assertNotEqual(again.returncode, 0)
        self.assertEqual(snapshot, {path.relative_to(portable): path.read_bytes()
                                   for path in portable.rglob("*") if path.is_file()})
        shutil.rmtree(package)
        self.server(scenario=portable)
        _, response = self.client()
        self.assertIsNone(response.get("error"))
