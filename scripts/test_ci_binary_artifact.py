"""Tiny artifact transport fixtures: no compiler, game process or network."""
import copy
import io
import json
import hashlib
import os
import stat
import sys
from types import SimpleNamespace
from pathlib import Path
import tarfile
import tempfile
import unittest
from unittest.mock import patch, MagicMock

import ci_binary_artifact as artifact


class BinaryArtifactTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name).resolve()
        self.context = {"profile": "debug", "source_sha": "a" * 40, "tree": "b" * 40,
                        "platform": "Linux", "target": "x86_64-unknown-linux-gnu",
                        "toolchain": {"rustc": "fixture rustc", "cargo": "fixture cargo"},
                        "runner": {"os": "Linux", "arch": "X64", "image": "ubuntu24", "version": "1.0"},
                        "workflow": {"repository": "owner/repo", "run": "12", "attempt": "1"},
                        "recipe": artifact.recipe("debug")}
        self.executables = {}
        for name in artifact.BINARIES:
            path = self.root / (name + (".exe" if os.name == "nt" else ""))
            path.write_bytes((name + "\x00fixture\n").encode())
            path.chmod(0o755)
            self.executables[name] = path
        self.output = self.root / "artifact"
        artifact.package(self.executables, self.context, self.output)
        self.context_patch = patch.object(artifact, "current_context", return_value=self.context)
        self.context_patch.start()
        self.addCleanup(self.context_patch.stop)
        # Independent valid baseline before any negative mutation.
        self.assertEqual(artifact.validate(self.output, "debug", "a" * 40), self.output / "bin")
        for name, original in self.executables.items():
            self.assertEqual((self.output / "bin" / name).read_bytes(), original.read_bytes())

    def manifest(self):
        return json.loads((self.output / "manifest.json").read_text(encoding="utf-8"))

    def write_manifest(self, value):
        (self.output / "manifest.json").write_text(json.dumps(value), encoding="utf-8")

    def reject(self):
        with self.assertRaises((ValueError, OSError)):
            artifact.validate(self.output, "debug", "a" * 40)

    def test_strict_json_preserves_unique_objects_and_rejects_nested_duplicates(self):
        self.assertEqual(json.loads('{"outer": {"value": 1}}', object_pairs_hook=artifact.unique_object),
                         {"outer": {"value": 1}})
        for raw in ('{"value": 1, "value": 1}', '{"outer": {"value": 1, "value": 2}}'):
            with self.subTest(raw=raw), self.assertRaisesRegex(ValueError, "duplicate JSON key"):
                json.loads(raw, object_pairs_hook=artifact.unique_object)

    def test_exact_roundtrip_and_repeat_validation(self):
        self.assertEqual(set(self.manifest()["executables"]), set(artifact.BINARIES))
        self.assertEqual(artifact.validate(self.output, "debug", "a" * 40), self.output / "bin")
        self.assertEqual(set(p.name for p in self.output.iterdir()), {"manifest.json", "executables.tar", "bin"})

    def test_identity_mismatch_is_rejected_before_materialization(self):
        for key in ("source_sha", "tree", "profile", "target", "toolchain", "runner", "workflow", "recipe"):
            with self.subTest(key=key):
                value = self.manifest()
                value["context"][key] = "wrong"
                self.write_manifest(value)
                self.reject()
                value["context"] = copy.deepcopy(self.context)
                self.write_manifest(value)

    def test_corrupt_missing_extra_and_symlink_executables_fail(self):
        path = self.output / "bin" / "tor-server"
        original = path.read_bytes()
        path.write_bytes(b"wrong")
        self.reject()
        path.write_bytes(original)
        path.unlink()
        self.reject()
        path.write_bytes(original)
        path.chmod(0o755)
        extra = self.output / "bin" / "unexpected"
        extra.write_bytes(b"extra")
        self.reject()
        extra.unlink()
        if hasattr(Path, "symlink_to"):
            path.unlink()
            try:
                path.symlink_to(self.executables["tor-server"])
            except OSError:
                self.skipTest("Host cannot create symlinks")
            self.reject()

    def test_transport_hash_and_strict_json(self):
        original = (self.output / "manifest.json").read_bytes()
        for raw in (original.replace(b'"version": 1', b'"version": 1, "version": 1'), original.replace(b'"version": 1', b'"version": true')):
            (self.output / "manifest.json").write_bytes(raw)
            self.reject()
        (self.output / "manifest.json").write_bytes(original)
        archive = self.output / "executables.tar"
        archive.write_bytes(archive.read_bytes() + b"unlisted")
        self.reject()

    def test_tar_membership_types_paths_and_mode_are_strict(self):
        # Recompute the outer hash: rejection must come from independent member checks.
        original = self.manifest()
        first = sorted(original["executables"])[0]
        for name, kind, mode in (("../escape", tarfile.REGTYPE, 0o755),
                                 (first, tarfile.SYMTYPE, 0o755),
                                 (first, tarfile.REGTYPE, 0o777)):
            with self.subTest(name=name, kind=kind, mode=mode):
                archive = self.output / "executables.tar"
                with tarfile.open(archive, "w", format=tarfile.USTAR_FORMAT) as output:
                    for key in sorted(self.executables):
                        data = self.executables[key].read_bytes()
                        row = tarfile.TarInfo(name if key == first else key)
                        row.type = kind if key == first else tarfile.REGTYPE
                        row.mode = mode if key == first else 0o755
                        row.size = len(data) if row.type == tarfile.REGTYPE else 0
                        row.linkname = "outside" if row.type == tarfile.SYMTYPE else ""
                        output.addfile(row, io.BytesIO(data) if row.size else None)
                value = copy.deepcopy(original)
                value["archive"] = artifact.file_identity(archive)
                self.write_manifest(value)
                self.reject()
        self.assertFalse((self.root / "escape").exists())

    def test_cargo_selection_requires_real_named_ordinary_artifacts(self):
        rows = []
        for name, path in self.executables.items():
            rows.append({"reason": "compiler-artifact", "executable": str(path), "features": [],
                         "profile": artifact.cargo_profile("debug"), "filenames": [str(path)],
                         "target": {"name": name, "kind": ["bin"], "crate_types": ["bin"],
                                    "src_path": str(artifact.ROOT / artifact.BINARIES[name])}})
        self.assertEqual(artifact.select_executables(rows, "debug", self.root), self.executables)
        compiler_plugin = {"reason": "compiler-artifact", "executable": None,
                           "target": {"name": "serde_derive", "kind": ["proc-macro"], "crate_types": ["proc-macro"]},
                           "filenames": [str(self.root / ("serde_derive.dll" if os.name == "nt" else "libserde_derive.so"))]}
        ordinary_library = {"reason": "compiler-artifact", "executable": None,
                            "target": {"name": "serde", "kind": ["lib"], "crate_types": ["lib"]},
                            "filenames": [str(self.root / "libserde.rlib")]}
        build_script = {"reason": "build-script-executed", "package_id": "fixture native build", "out_dir": str(self.root)}
        self.assertEqual(artifact.select_executables([compiler_plugin, ordinary_library, build_script, *rows],
                                                    "debug", self.root), self.executables)
        for target in ({"kind": ["lib"], "crate_types": ["cdylib"]},
                       {"kind": ["proc-macro"], "crate_types": ["dylib"]}):
            with self.subTest(runtime_target=target), self.assertRaises(ValueError):
                artifact.select_executables([{**compiler_plugin, "target": target}, *rows], "debug", self.root)
        for mutation in ("duplicate", "missing", "feature", "profile", "source", "relative", "dll"):
            with self.subTest(mutation=mutation):
                invalid = copy.deepcopy(rows)
                if mutation == "duplicate": invalid.append(invalid[0])
                if mutation == "missing": invalid.pop()
                if mutation == "feature": invalid[0]["features"] = ["service-contract-host"]
                if mutation == "profile": invalid[0]["profile"]["test"] = True
                if mutation == "source": invalid[0]["target"]["src_path"] = str(self.root / "other.rs")
                if mutation == "relative": invalid[0]["executable"] = "tor-server"
                if mutation == "dll": invalid[0]["filenames"].append(str(self.root / "custom.dll"))
                with self.assertRaises(ValueError):
                    artifact.select_executables(invalid, "debug", self.root)

    def test_cargo_lexical_child_escape_cannot_leave_target(self):
        target = self.root / "cargo-target"
        (target / "child").mkdir(parents=True)
        outside = self.root / "outside"
        outside.mkdir()
        rows = []
        for name, source in self.executables.items():
            path = target / source.name
            path.write_bytes(source.read_bytes())
            rows.append({"reason": "compiler-artifact", "executable": str(path), "features": [],
                         "profile": artifact.cargo_profile("debug"), "filenames": [str(path)],
                         "target": {"name": name, "kind": ["bin"], "crate_types": ["bin"],
                                    "src_path": str(artifact.ROOT / artifact.BINARIES[name])}})
        self.assertEqual(len(artifact.select_executables(rows, "debug", target)), 6)
        name = self.executables["tor-server"].name
        (outside / name).write_bytes(b"wrong target")
        rows[0]["executable"] = str(target / "child" / ".." / ".." / "outside" / name)
        with self.assertRaisesRegex(ValueError, "outside Cargo target"):
            artifact.select_executables(rows, "debug", target)

    def test_actual_build_orchestration_accepts_mixed_json_stream_and_packages(self):
        rows = [{"reason": "compiler-artifact", "executable": None,
                 "target": {"kind": ["proc-macro"], "crate_types": ["proc-macro"]},
                 "filenames": [str(self.root / "macro.dll")]}]
        for name, path in self.executables.items():
            rows.append({"reason": "compiler-artifact", "executable": str(path), "features": [],
                         "profile": artifact.cargo_profile("debug"), "filenames": [str(path)],
                         "target": {"name": name, "kind": ["bin"], "crate_types": ["bin"],
                                    "src_path": str(artifact.ROOT / artifact.BINARIES[name])}})
        rows.append({"reason": "build-finished", "success": True})
        process = MagicMock()
        process.__enter__.return_value = process
        process.stdout = io.StringIO("".join(json.dumps(row) + "\n" for row in rows))
        process.wait.return_value = 0
        output = self.root / "successful-build"
        with patch.object(artifact, "command_output", return_value=json.dumps({"target_directory": str(self.root)})), \
                patch.object(artifact.subprocess, "Popen", return_value=process) as launch:
            self.assertEqual(artifact.build("debug", output), output)
        self.assertEqual(launch.call_args.args[0], artifact.recipe("debug")["command"])
        self.assertEqual(launch.call_args.kwargs["env"]["CARGO_INCREMENTAL"], "0")
        self.assertEqual(artifact.validate(output, "debug", "a" * 40), output / "bin")
        for name, source in self.executables.items():
            self.assertEqual((output / "bin" / name).read_bytes(), source.read_bytes())

    def test_windows_extension_survives_mode_losing_outer_transport(self):
        context = copy.deepcopy(self.context)
        context["platform"] = "Windows"
        output = self.root / "windows-artifact"
        artifact.package(self.executables, context, output)
        for path in output.iterdir():
            path.chmod(0o644)  # upload/download-artifact's outer ZIP behavior.
        with patch.object(artifact, "current_context", return_value=context):
            bindir = artifact.validate(output, "debug", "a" * 40)
        self.assertEqual({p.name for p in bindir.iterdir()}, {name + ".exe" for name in artifact.BINARIES})
        for name, source in self.executables.items():
            self.assertEqual((bindir / (name + ".exe")).read_bytes(), source.read_bytes())

    def test_archive_gap_extra_duplicate_and_bad_inner_digest(self):
        original = (self.output / "executables.tar").read_bytes()
        for changed in (original[:512], original + b"extra", original[:512] + original,
                        original[:512] + b"X" + original[513:]):
            with self.subTest(size=len(changed)):
                archive = self.output / "executables.tar"
                archive.write_bytes(changed)
                value = self.manifest()
                value["archive"] = artifact.file_identity(archive)
                self.write_manifest(value)
                self.reject()

    def test_manifest_contract_types_and_feature_separation(self):
        original = self.manifest()
        for field, replacement in (("path", "../tor-server"), ("path", "TOR-SERVER"),
                                   ("bytes", True), ("mode", True), ("features", ["service-contract-host"]),
                                   ("sha256", "z" * 64)):
            value = copy.deepcopy(original)
            value["executables"]["tor-server"][field] = replacement
            self.write_manifest(value)
            self.reject()
        self.write_manifest(original)
        value = self.manifest()
        value["executables"].pop("tor-arena")
        self.write_manifest(value)
        self.reject()

    def test_package_never_overwrites_existing_bundle_or_accepts_incomplete_apps(self):
        before = (self.output / "manifest.json").read_bytes()
        with self.assertRaises(FileExistsError):
            artifact.package(self.executables, self.context, self.output)
        with self.assertRaises(ValueError):
            artifact.package({}, self.context, self.root / "missing")
        self.assertFalse((self.root / "missing").exists())
        self.assertEqual((self.output / "manifest.json").read_bytes(), before)

    @staticmethod
    def changed_stat(info, **changes):
        fields = ('st_dev', 'st_ino', 'st_mode', 'st_size', 'st_mtime_ns', 'st_ctime_ns')
        return SimpleNamespace(**{**{name: getattr(info, name) for name in fields},
                                  'st_file_attributes': getattr(info, 'st_file_attributes', 0),
                                  **({'st_birthtime_ns': info.st_birthtime_ns} if hasattr(info, 'st_birthtime_ns') else {}),
                                  **changes})

    def test_actual_executable_path_and_current_interpreter_hash_and_read(self):
        path = self.root / 'actual-native.exe'
        data = b'actual binary I/O fixture\x00\n'
        path.write_bytes(data)
        expected = {'bytes': len(data), 'sha256': hashlib.sha256(data).hexdigest()}
        self.assertEqual(artifact.file_identity(path), expected)
        self.assertEqual(artifact.read_small(path, len(data)), data)
        interpreter = Path(sys.executable).resolve(strict=True)
        expected_hash = hashlib.sha256()
        with interpreter.open('rb') as stream:
            for block in iter(lambda: stream.read(1024 * 1024), b''):
                expected_hash.update(block)
        self.assertEqual(artifact.file_identity(interpreter),
                         {'bytes': interpreter.stat().st_size, 'sha256': expected_hash.hexdigest()})

    def test_windows_filename_execute_bits_do_not_mean_replacement(self):
        path = self.root / 'windows-mode.exe'
        data = b'complete executable bytes\x00\n'
        path.write_bytes(data)
        expected = artifact.file_identity(path)
        original = Path.lstat
        def windows_path_info(candidate):
            info = original(candidate)
            return self.changed_stat(info, st_mode=info.st_mode | 0o111) if candidate == path else info
        # Real file I/O; only model CPython's filename-derived Windows mode bits.
        with patch.object(artifact.sys, 'platform', 'win32'), patch.object(Path, 'lstat', windows_path_info):
            self.assertEqual(artifact.file_identity(path), expected)
            self.assertEqual(artifact.read_small(path, len(data)), data)
        with patch.object(artifact.sys, 'platform', 'linux'), patch.object(Path, 'lstat', windows_path_info):
            with self.assertRaisesRegex(ValueError, 'changed while opening'):
                artifact.file_identity(path)

    def test_open_handle_identity_permissions_type_and_reparse_changes_reject(self):
        path = self.executables['tor-server']
        baseline = artifact.file_identity(path)
        original = os.fstat
        info = path.stat()
        changes = ({'st_dev': info.st_dev + 1}, {'st_ino': info.st_ino + 1},
                   {'st_mode': info.st_mode ^ stat.S_IWUSR}, {'st_size': info.st_size + 1},
                   {'st_mtime_ns': info.st_mtime_ns + 1},
                   {'st_mode': stat.S_IFDIR | 0o755}, {'st_file_attributes': 0x400})
        for change in changes:
            with self.subTest(change=change), patch.object(artifact.sys, 'platform', 'win32'), \
                    patch.object(artifact.os, 'fstat', side_effect=lambda fd: self.changed_stat(original(fd), **change)):
                with self.assertRaises(ValueError):
                    artifact.file_identity(path)
                with self.assertRaises(ValueError):
                    artifact.read_small(path, 1024)
        self.assertEqual(artifact.file_identity(path), baseline)

    def test_windows_birthtime_aligns_distinct_path_and_handle_ctime(self):
        path = self.root / 'windows-times.bin'
        data = b'unchanged bytes'
        path.write_bytes(data)
        expected = artifact.file_identity(path)
        original_path, original_fd = Path.lstat, os.fstat
        def path_info(candidate):
            return self.changed_stat(original_path(candidate), st_birthtime_ns=100, st_ctime_ns=100)
        def handle_info(fd):
            return self.changed_stat(original_fd(fd), st_birthtime_ns=100, st_ctime_ns=200)
        with patch.object(artifact.sys, 'platform', 'win32'), patch.object(Path, 'lstat', path_info), \
                patch.object(artifact.os, 'fstat', handle_info):
            self.assertEqual(artifact.file_identity(path), expected)
            self.assertEqual(artifact.read_small(path, len(data)), data)
        with patch.object(artifact.sys, 'platform', 'linux'), patch.object(Path, 'lstat', path_info), \
                patch.object(artifact.os, 'fstat', handle_info), self.assertRaises(ValueError):
            artifact.file_identity(path)
        for birthtime in (101, None):
            def mismatched(fd):
                info = handle_info(fd)
                if birthtime is None:
                    del info.st_birthtime_ns
                else:
                    info.st_birthtime_ns = birthtime
                return info
            with self.subTest(birthtime=birthtime), patch.object(artifact.sys, 'platform', 'win32'), \
                    patch.object(Path, 'lstat', path_info), patch.object(artifact.os, 'fstat', mismatched), \
                    self.assertRaises(ValueError):
                artifact.file_identity(path)

    def test_windows_legacy_absent_birthtime_keeps_creation_time_comparison(self):
        path = self.root / 'legacy.bin'
        path.write_bytes(b'legacy metadata')
        expected = artifact.file_identity(path)
        original_path, original_fd = Path.lstat, os.fstat
        def legacy(info, ctime):
            info = self.changed_stat(info, st_ctime_ns=ctime)
            if hasattr(info, 'st_birthtime_ns'):
                del info.st_birthtime_ns
            return info
        for handle_ctime in (100, 101):
            with patch.object(artifact.sys, 'platform', 'win32'), \
                    patch.object(Path, 'lstat', lambda candidate: legacy(original_path(candidate), 100)), \
                    patch.object(artifact.os, 'fstat', lambda fd: legacy(original_fd(fd), handle_ctime)):
                if handle_ctime == 100:
                    self.assertEqual(artifact.file_identity(path), expected)
                else:
                    with self.assertRaises(ValueError):
                        artifact.file_identity(path)

    def test_windows_same_view_metadata_changes_are_not_masked(self):
        path = self.executables['tor-server']
        artifact.file_identity(path)
        original_fd, original_path = os.fstat, Path.lstat
        for view in ('handle', 'path'):
            for field in ('st_mode', 'st_ctime_ns', 'st_birthtime_ns', 'st_file_attributes'):
                for read in (artifact.file_identity, lambda file: artifact.read_small(file, 1024)):
                    calls = 0
                    def altered(value):
                        nonlocal calls
                        info = original_fd(value) if view == 'handle' else original_path(value)
                        calls += 1
                        if calls == 1:
                            return info
                        changed = (info.st_mode ^ 0o111 if field == 'st_mode' else
                                   0x400 if field == 'st_file_attributes' else getattr(info, field, 0) + 1)
                        return self.changed_stat(info, **{field: changed})
                    with self.subTest(view=view, field=field), patch.object(artifact.sys, 'platform', 'win32'), \
                            patch.object(artifact.os if view == 'handle' else Path,
                                         'fstat' if view == 'handle' else 'lstat', new=altered):
                        with self.assertRaises(ValueError):
                            read(path)

    def test_actual_same_byte_path_replacement_between_stat_and_open_rejects(self):
        path = self.root / 'replace.bin'
        path.write_bytes(b'same bytes')
        artifact.file_identity(path)
        saved = self.root / 'previous.bin'
        original = Path.open
        def replace(candidate, *args, **kwargs):
            if candidate == path and args == ('rb',):
                candidate.rename(saved)
                candidate.write_bytes(b'same bytes')
            return original(candidate, *args, **kwargs)
        with patch.object(Path, 'open', replace), self.assertRaisesRegex(ValueError, 'changed while opening'):
            artifact.file_identity(path)
        self.assertEqual(path.read_bytes(), saved.read_bytes())
        self.assertNotEqual(path.stat().st_ino, saved.stat().st_ino)

    def test_actual_content_drift_after_read_rejects(self):
        path = self.root / 'mutate.bin'
        path.write_bytes(b'original')
        artifact.file_identity(path)
        before = path.stat()
        original = os.fstat
        calls = 0
        def mutate(fd):
            nonlocal calls
            calls += 1
            if calls == 2:
                path.write_bytes(b'modified')
                os.utime(path, ns=(before.st_atime_ns, before.st_mtime_ns + 1_000_000_000))
            return original(fd)
        with patch.object(artifact.os, 'fstat', side_effect=mutate), \
                self.assertRaisesRegex(ValueError, 'changed while hashing'):
            artifact.file_identity(path)

    def test_hashing_does_not_confuse_read_access_time_with_mutation(self):
        path = self.executables["tor-server"]
        os.utime(path, (1, 2))
        self.assertEqual(artifact.file_identity(path), artifact.file_identity(path))

    def test_cargo_failure_cannot_publish_or_fall_back(self):
        process = MagicMock()
        process.__enter__.return_value = process
        process.stdout.readline.return_value = ""
        process.wait.return_value = 1
        output = self.root / "failed-build"
        with patch.object(artifact, "command_output", return_value=json.dumps({"target_directory": str(self.root)})), \
                patch.object(artifact.subprocess, "Popen", return_value=process) as launch:
            with self.assertRaisesRegex(ValueError, "Cargo app build failed"):
                artifact.build("debug", output)
        launch.assert_called_once()
        self.assertFalse(output.exists())


class ContextTests(unittest.TestCase):
    def test_workflow_context_has_valid_baseline_and_rejects_each_missing_binding(self):
        env = {"GITHUB_SHA": "a" * 40, "GITHUB_REPOSITORY": "owner/repo", "GITHUB_RUN_ID": "12",
               "GITHUB_RUN_ATTEMPT": "1", "RUNNER_OS": "Windows" if os.name == "nt" else "Linux",
               "RUNNER_ARCH": "X64", "ImageOS": "fixture", "ImageVersion": "1.0"}
        checkout = {"sha": "a" * 40, "tree": "b" * 40, "tracked_dirty": False, "untracked_present": False}
        def tool(command):
            return "rustc fixture\nhost: x86_64-fixture" if command[0] == "rustc" else "cargo fixture"
        with patch.dict(os.environ, env, clear=True), patch.object(artifact, "checkout", return_value=checkout), \
                patch.object(artifact, "command_output", side_effect=tool), \
                patch.object(artifact, "cargo_configuration", return_value=[]):
            baseline = artifact.current_context("debug", "a" * 40)
            self.assertEqual(baseline["workflow"], {"repository": "owner/repo", "run": "12", "attempt": "1"})
            self.assertEqual(baseline["recipe"]["features"], [])
            for key in env:
                with self.subTest(key=key), patch.dict(os.environ, {key: ""}):
                    with self.assertRaises(ValueError):
                        artifact.current_context("debug", "a" * 40)
            for key in ("RUSTFLAGS", "CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS",
                        "CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER", "CARGO_BUILD_RUSTC_WRAPPER"):
                with self.subTest(override=key), patch.dict(os.environ, {key: "unsupported"}):
                    with self.assertRaises(ValueError):
                        artifact.current_context("debug", "a" * 40)
            for key in ("tracked_dirty", "untracked_present"):
                with self.subTest(key=key), patch.object(artifact, "checkout", return_value={**checkout, key: True}):
                    with self.assertRaises(ValueError):
                        artifact.current_context("debug", "a" * 40)

    def test_only_tracked_checkout_cargo_configuration_is_read_and_bound(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve() / "checkout"
            (root / ".cargo").mkdir(parents=True)
            home = root.parent / "cargo-home"
            home.mkdir()
            self.assertEqual(artifact.cargo_configuration(root, home), [])
            config = root / ".cargo/config.toml"
            config.write_text('[net]\noffline = true\n', encoding="utf-8")
            with patch.object(artifact, "command_output", return_value=".cargo/config.toml"):
                first = artifact.cargo_configuration(root, home)
                self.assertEqual(len(first), 1)
                config.write_text('[net]\noffline = false\n', encoding="utf-8")
                self.assertNotEqual(first, artifact.cargo_configuration(root, home))
                for table in ("build", "target", "source", "patch", "alias", "profile"):
                    config.write_text(f'[{table}]\nsetting = "unsupported"\n', encoding="utf-8")
                    with self.subTest(table=table), self.assertRaisesRegex(ValueError, "code-affecting"):
                        artifact.cargo_configuration(root, home)
                config.write_text('[net]\noffline = false\n', encoding="utf-8")
                external = home / "config.toml"
                external.write_text("never read this external configuration", encoding="utf-8")
                real_read = artifact.read_small
                def read(path, limit):
                    self.assertNotEqual(path, external)
                    return real_read(path, limit)
                with patch.object(artifact, "read_small", side_effect=read):
                    with self.assertRaisesRegex(ValueError, "external Cargo configuration"):
                        artifact.cargo_configuration(root, home)

    def test_help_smoke_launches_each_validated_app_and_never_falls_back(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            suffix = ".exe" if os.name == "nt" else ""
            for name in artifact.BINARIES:
                (root / (name + suffix)).write_bytes(b"tiny executable fixture")
            certificate = (artifact.ROOT / "scenarios/two-room/validation.json").read_bytes()
            def output(command, *args, **kwargs):
                return certificate if command[1] == "check" else b"usage\n"
            with patch.object(artifact, "probe", side_effect=output) as launch:
                artifact.smoke(root)
            expected = [[str(root / (name + suffix)), *(["check", str(artifact.ROOT / "scenarios/two-room")]
                         if name == "tor-scenario" else ["--help"])] for name in sorted(artifact.BINARIES)]
            self.assertEqual([call.args[0] for call in launch.call_args_list], expected)
            with patch.object(artifact, "probe", return_value=None) as launch:
                with self.assertRaisesRegex(ValueError, "application smoke failed"):
                    artifact.smoke(root)
            launch.assert_called_once()
            with patch.object(artifact, "probe", side_effect=lambda command, *a, **kw:
                              b'{}' if command[1] == "check" else b"usage\n"):
                with self.assertRaisesRegex(ValueError, "certificate mismatch"):
                    artifact.smoke(root)


if __name__ == "__main__":
    unittest.main()
