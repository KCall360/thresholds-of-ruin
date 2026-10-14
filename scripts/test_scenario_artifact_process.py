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

    def test_compiled_export_rejects_old_manifest_format_with_current_certificate(self):
        package = self.package("current-source")
        validated = self.compiler("validate", str(package))
        self.assertEqual(validated.returncode, 0, validated.stderr)
        portable = self.directory / "current-portable"
        exported = self.compiler("export", str(package), str(portable))
        self.assertEqual(exported.returncode, 0, exported.stderr)
        current = self.compiler("export", str(portable), str(self.directory / "current-copy"))
        self.assertEqual(current.returncode, 0, current.stderr)
        metadata = portable / "package.json"
        document = json.loads(metadata.read_text(encoding="utf-8"))
        self.assertEqual(document["package"]["manifest"]["format"], 9)
        for obsolete in (4, 5, 6, 7, 8):
            document["package"]["manifest"]["format"] = obsolete
            metadata.write_text(json.dumps(document), encoding="utf-8")
            before = {path.relative_to(portable): path.read_bytes()
                      for path in portable.rglob("*") if path.is_file()}
            destination = self.directory / f"rejected-copy-{obsolete}"
            rejected = self.compiler("export", str(portable), str(destination))
            self.assertNotEqual(rejected.returncode, 0)
            self.assertIn("Unsupported scenario format", json.loads(rejected.stderr)["error"]["message"])
            self.assertFalse(destination.exists())
            self.assertEqual(before, {path.relative_to(portable): path.read_bytes()
                                      for path in portable.rglob("*") if path.is_file()})

    def test_portable_export_rejects_previous_package_and_rules_envelopes_without_rewrite(self):
        package = self.package("envelope-source")
        validated = self.compiler("validate", str(package))
        self.assertEqual(validated.returncode, 0, validated.stderr)
        portable = self.directory / "envelope-portable"
        exported = self.compiler("export", str(package), str(portable))
        self.assertEqual(exported.returncode, 0, exported.stderr)
        for boundary, obsolete in (("package", 1), ("package", 2), ("package", 3), ("package", 4), ("package", 5),
                                   ("rules", 1), ("rules", 2), ("rules", 3), ("rules", 4), ("rules", 5)):
            with self.subTest(boundary=boundary, obsolete=obsolete):
                source = self.directory / (f"old-{boundary}-{obsolete}")
                shutil.copytree(portable, source)
                metadata = source / "package.json"
                document = json.loads(metadata.read_text(encoding="utf-8"))
                self.assertEqual(document["format"], 6)
                if boundary == "package":
                    document["format"] = obsolete
                else:
                    rules = document["package"]["rules_document"]
                    self.assertTrue(rules.startswith('{"format":6,'))
                    document["package"]["rules_document"] = rules.replace('"format":6', f'"format":{obsolete}', 1)
                metadata.write_text(json.dumps(document), encoding="utf-8")
                before = {path.relative_to(source): path.read_bytes()
                          for path in source.rglob("*") if path.is_file()}
                destination = self.directory / (f"rejected-{boundary}-{obsolete}")
                rejected = self.compiler("export", str(source), str(destination))
                self.assertNotEqual(rejected.returncode, 0)
                self.assertFalse(destination.exists())
                self.assertEqual(before, {path.relative_to(source): path.read_bytes()
                                          for path in source.rglob("*") if path.is_file()})
