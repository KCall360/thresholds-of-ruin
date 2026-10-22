"""Exact-workflow ordinary app transport, not test qualification or a Cargo cache.

Only six actual Cargo executables are transported. System libraries remain the
runner image/setup recipe's responsibility; consumers must launch their runtime
smoke, and never rebuild or fetch a missing runtime implicitly. The tar preserves
executable modes through upload-artifact's mode-losing ZIP transport. Its format
is deliberately smaller than general tar: exact ordered USTAR regular members,
no links, extension headers, extra entries or trailing payloads.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import subprocess
import sys
import tarfile
import tempfile
import tomllib

from ci_observation import checkout, probe

ROOT = Path(__file__).resolve().parent.parent
BINARIES = {**{name: f"crates/server/src/{suffix}" for name, suffix in (
    ("tor-server", "main.rs"), ("tor-scenario", "bin/tor-scenario.rs"), ("tor-arena", "bin/tor-arena.rs"))},
    **{f"tor-client-{kind}": f"crates/client-{kind}/src/main.rs" for kind in ("headless", "text", "ascii")}}
MAX_EXECUTABLE = 256 * 1024 * 1024
MAX_ARCHIVE = 1024 * 1024 * 1024
BUILD_ENV = {"CARGO_INCREMENTAL": "0", "CARGO_PROFILE_DEV_DEBUG": "0", "CARGO_PROFILE_RELEASE_DEBUG": "0"}
FORBIDDEN_ENV = ("RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS", "RUSTC", "RUSTC_WRAPPER", "RUSTC_WORKSPACE_WRAPPER",
                 "CARGO_BUILD_TARGET", "CARGO_BUILD_RUSTFLAGS", "CC", "CXX", "AR", "CFLAGS", "CXXFLAGS", "LDFLAGS")


def require(condition, reason):
    if not condition:
        raise ValueError(reason)


def unique_object(pairs):
    """Reject duplicate keys at every object boundary in strict JSON documents."""
    result = {}
    for key, value in pairs:
        require(key not in result, "duplicate JSON key")
        result[key] = value
    return result


def regular(path):
    info = path.lstat()
    require(stat.S_ISREG(info.st_mode) and not (getattr(info, "st_file_attributes", 0) & 0x400),
            "artifact must be a regular non-reparse file")
    return info


def directory(path):
    path = Path(path).absolute()
    for parent in (path, *path.parents):
        info = parent.lstat()
        require(stat.S_ISDIR(info.st_mode) and not (getattr(info, "st_file_attributes", 0) & 0x400),
                "artifact directory must not contain links or reparse points")
    return path.resolve(strict=True)


def stable_stat(info, *, cross_view=False):
    """Exact same-view snapshots; align only Windows path/handle API semantics."""
    mode, changed = info.st_mode, info.st_ctime_ns
    birth = getattr(info, "st_birthtime_ns", None)
    if cross_view and sys.platform == "win32":
        # CPython path stat synthesizes executable bits from the filename.
        # In 3.12 path ctime is CreationTime while fstat ctime is ChangeTime;
        # birthtime aligns them. Older APIs without birthtime use creation ctime.
        mode &= ~0o111
        changed = birth if birth is not None else changed
    return (info.st_dev, info.st_ino, mode, info.st_size, info.st_mtime_ns, changed,
            birth, getattr(info, "st_file_attributes", 0) & 0x400)


def file_identity(path):
    before = regular(path)
    require(0 < before.st_size <= MAX_ARCHIVE, "artifact size outside bound")
    digest = hashlib.sha256()
    with path.open("rb") as source:
        opened = os.fstat(source.fileno())
        require(stable_stat(opened, cross_view=True) == stable_stat(before, cross_view=True),
                "artifact changed while opening")
        remaining = before.st_size
        while remaining:
            block = source.read(min(remaining, 1024 * 1024))
            require(bool(block), "artifact truncated while hashing")
            digest.update(block)
            remaining -= len(block)
        require(not source.read(1), "artifact grew while hashing")
        require(stable_stat(os.fstat(source.fileno())) == stable_stat(opened), "artifact changed while hashing")
    require(stable_stat(regular(path)) == stable_stat(before), "artifact pathname changed while hashing")
    return {"bytes": before.st_size, "sha256": digest.hexdigest()}


def read_small(path, limit):
    before = regular(path)
    require(before.st_size <= limit, "metadata file exceeds size bound")
    with path.open("rb") as source:
        opened = os.fstat(source.fileno())
        require(stable_stat(opened, cross_view=True) == stable_stat(before, cross_view=True),
                "metadata changed while opening")
        raw = source.read(limit + 1)
        require(len(raw) <= limit and stable_stat(os.fstat(source.fileno())) == stable_stat(opened)
                and stable_stat(regular(path)) == stable_stat(before), "metadata changed while reading")
    return raw


def cargo_configuration(root=ROOT, home=None):
    """Bind tracked checkout Cargo config; reject external configuration unread.

    Cargo reads ancestors plus CARGO_HOME. External file contents (which might
    contain credentials) are never read or logged; presence is unsupported.
    """
    if home is None:
        home = Path(os.environ.get("CARGO_HOME", Path.home() / ".cargo"))
        if not home.is_absolute():
            home = ROOT / home
    roots = [(f"ancestor-{index}", parent / ".cargo") for index, parent in enumerate((root, *root.parents))]
    roots.append(("cargo-home", home))
    identities = []
    for scope, parent in roots:
        for filename in ("config", "config.toml"):
            path = parent / filename
            if path.exists() or path.is_symlink():
                directory(parent)
                require(parent.resolve() == root / ".cargo", "unsupported external Cargo configuration")
                relative = path.relative_to(root).as_posix()
                require(command_output(["git", "ls-files", "--error-unmatch", "--", relative]) == relative,
                        "untracked Cargo configuration")
                raw = read_small(path, 64 * 1024)
                configuration = tomllib.loads(raw.decode("utf-8"))
                require(set(configuration) <= {"net", "term"}
                        and all(isinstance(table, dict) for table in configuration.values()),
                        "unsupported code-affecting Cargo configuration")
                identities.append({"scope": scope, "name": filename, "sha256": hashlib.sha256(raw).hexdigest()})
    return identities


def recipe(profile):
    require(profile in ("debug", "release"), "unsupported profile")
    return {"command": ["cargo", "build", "--workspace", "--bins", "--locked",
                        "--message-format=json-render-diagnostics", *(["--release"] if profile == "release" else [])],
            "environment": BUILD_ENV, "features": [], "runtime": "same-runner-family-and-checked-out-setup-v2"}


def cargo_profile(profile):
    return {"opt_level": "1" if profile == "debug" else "3", "debuginfo": 0,
            "debug_assertions": profile == "debug", "overflow_checks": profile == "debug", "test": False}


def command_output(command):
    issues = []
    output = probe(command, ROOT, issues, "binary-artifact", timeout=30, max_output_bytes=4 * 1024 * 1024)
    require(output is not None and not issues, "required identity command failed")
    return output.decode("utf-8").strip()


def image_version_kind(value):
    """Hosted patch releases may vary; local SDK fingerprints must remain exact."""
    if isinstance(value, str):
        if re.fullmatch(r"[0-9]{1,8}(?:\.[0-9]{1,8}){1,3}", value):
            return "hosted"
        if re.fullmatch(r"[0-9a-f]{64}", value):
            return "local"
    return None


def require_compatible_context(producer, consumer):
    """Compare fixed contract fields; never echo untrusted keys or values."""
    require(isinstance(producer, dict), "artifact environment/source mismatch: context.keys")
    differences = []
    if set(producer) != set(consumer):
        differences.append("context.keys")
    for field in ("profile", "source_sha", "tree", "cargo_config", "platform", "target"):
        if producer.get(field) != consumer.get(field):
            differences.append(field)
    groups = {"runner": ("os", "arch", "image", "version"),
              "workflow": ("repository", "run", "attempt"), "toolchain": ("rustc", "cargo"),
              "recipe": ("command", "environment", "features", "runtime")}
    for group, fields in groups.items():
        actual, expected = producer.get(group), consumer[group]
        if not isinstance(actual, dict) or set(actual) != set(expected):
            differences.append(f"{group}.keys")
        if not isinstance(actual, dict):
            continue
        for field in fields:
            left, right = actual.get(field), expected.get(field)
            if group == "runner" and field == "version":
                kinds = image_version_kind(left), image_version_kind(right)
                matches = (kinds == ("hosted", "hosted")
                           or (kinds == ("local", "local") and left == right))
            else:
                matches = left == right
            if not matches:
                differences.append(f"{group}.{field}")
    require(not differences, "artifact environment/source mismatch: " + ", ".join(differences))


def current_context(profile, source_sha):
    require(isinstance(source_sha, str) and re.fullmatch(r"[0-9a-f]{40}", source_sha), "invalid source SHA")
    require(not any(os.environ.get(key) for key in FORBIDDEN_ENV), "unsupported compiler override")
    require(not any((key.startswith("CARGO_TARGET_") and key.endswith(("_RUSTFLAGS", "_LINKER", "_RUNNER")))
                    or (key.startswith("CARGO_BUILD_") and key != "CARGO_BUILD_JOBS")
                    for key, value in os.environ.items() if value), "unsupported target/build override")
    require(not any(key.startswith("CARGO_PROFILE_") and key not in BUILD_ENV for key in os.environ),
            "unsupported Cargo profile override")
    issues = []
    source = checkout(ROOT, issues)
    require(not issues and source["sha"] == source_sha and source["tracked_dirty"] is False
            and source["untracked_present"] is False, "source checkout is not exact and clean")
    require(os.environ.get("GITHUB_SHA") == source_sha, "workflow checkout SHA mismatch")
    fields = {"runner": {"os": "RUNNER_OS", "arch": "RUNNER_ARCH", "image": "ImageOS", "version": "ImageVersion"},
              "workflow": {"repository": "GITHUB_REPOSITORY", "run": "GITHUB_RUN_ID", "attempt": "GITHUB_RUN_ATTEMPT"}}
    context = {"profile": profile, "source_sha": source_sha, "tree": source["tree"], "recipe": recipe(profile),
               "cargo_config": cargo_configuration()}
    for group, keys in fields.items():
        context[group] = {}
        for name, key in keys.items():
            value = os.environ.get(key, "")
            require(re.fullmatch(r"[A-Za-z0-9_./+-]{1,160}", value), "missing or invalid runner/workflow identity")
            context[group][name] = value
    require(image_version_kind(context["runner"]["version"]) is not None, "invalid runner.version")
    platform = context["runner"]["os"]
    require(platform in ("Linux", "Windows") and (platform == "Windows") == (os.name == "nt"), "unsupported platform")
    rustc, cargo = command_output(["rustc", "-Vv"]), command_output(["cargo", "-V"])
    hosts = re.findall(r"^host: ([A-Za-z0-9_-]+)$", rustc, re.MULTILINE)
    require(len(hosts) == 1, "missing native compiler target")
    context.update(platform=platform, target=hosts[0], toolchain={"rustc": rustc, "cargo": cargo})
    return context


def select_executables(rows, profile, target_directory):
    target_directory = directory(target_directory)
    selected = {}
    for row in rows:
        if row.get("reason") != "compiler-artifact":
            continue
        target = row.get("target", {})
        compiler_plugin = (target.get("kind") == ["proc-macro"]
                           and target.get("crate_types") == ["proc-macro"] and row.get("executable") is None)
        require(compiler_plugin or not any(re.search(r"\.(?:dll|dylib|so(?:\.[0-9]+)*)$", Path(name).name.lower())
                                           for name in row.get("filenames", [])),
                "unsupported emitted dynamic runtime asset")
        if target.get("kind") != ["bin"]:
            continue
        name = target.get("name")
        require(name in BINARIES and name not in selected, "unexpected or duplicate executable")
        require(target.get("crate_types") == ["bin"] and row.get("features") == []
                and row.get("profile") == cargo_profile(profile)
                and Path(target.get("src_path", "")).resolve() == ROOT / BINARIES[name],
                "executable does not match ordinary app recipe")
        path = Path(row.get("executable") or "")
        require(path.is_absolute() and path.is_relative_to(target_directory), "executable outside Cargo target")
        canonical = directory(path.parent) / path.name
        require(canonical.is_relative_to(target_directory), "executable outside Cargo target")
        regular(canonical)
        require(path.name == name + (".exe" if os.name == "nt" else ""), "unexpected executable filename")
        selected[name] = canonical
    require(set(selected) == set(BINARIES), "missing required app executable")
    return selected


def header(name, size):
    info = tarfile.TarInfo(name)
    info.size, info.mode = size, 0o755
    return info.tobuf(format=tarfile.USTAR_FORMAT)


def package(executables, context, output):
    require(set(executables) == set(BINARIES), "incomplete ordinary apps")
    suffix = ".exe" if context["platform"] == "Windows" else ""
    identities = {}
    for name, path in executables.items():
        directory(path.parent)
        identity = file_identity(path)
        require(identity["bytes"] <= MAX_EXECUTABLE, "executable exceeds size bound")
        identities[name] = {**identity, "path": name + suffix, "mode": 0o755, "features": []}
    require(sum(512 + row["bytes"] + (-row["bytes"] % 512) for row in identities.values()) + 10240
            <= MAX_ARCHIVE, "combined executable archive exceeds bound")
    output = directory(Path(output).parent) / Path(output).name
    output.mkdir()  # Never replace an earlier or partial artifact.
    archive = output / "executables.tar"
    with archive.open("xb") as stream:
        for name in sorted(identities):
            row = identities[name]
            stream.write(header(row["path"], row["bytes"]))
            with executables[name].open("rb") as source:
                remaining = row["bytes"]
                while remaining:
                    block = source.read(min(remaining, 1024 * 1024))
                    require(bool(block), "producer executable truncated")
                    stream.write(block)
                    remaining -= len(block)
                require(not source.read(1), "producer executable grew")
            stream.write(b"\0" * (-row["bytes"] % 512))
        stream.write(b"\0" * (1024 + (-(stream.tell() + 1024) % 10240)))
        stream.flush()
        os.fsync(stream.fileno())
    manifest = {"version": 1, "context": context, "executables": identities, "archive": file_identity(archive)}
    check_archive(archive, identities)
    for name, path in executables.items():
        require(file_identity(path) == {key: identities[name][key] for key in ("bytes", "sha256")}, "producer changed")
    with (output / "manifest.json").open("x", encoding="utf-8", newline="\n") as stream:
        json.dump(manifest, stream, sort_keys=True, indent=2)
        stream.write("\n")
        stream.flush()
        os.fsync(stream.fileno())
    return output


def check_archive(archive, identities, destination=None):
    # Do not call extractall: exact headers reject links/extensions/path aliases.
    with archive.open("rb") as source:
        for name in sorted(identities):
            row = identities[name]
            require(source.read(512) == header(row["path"], row["bytes"]), "unsafe or unexpected archive member")
            digest, remaining = hashlib.sha256(), row["bytes"]
            output = (destination / row["path"]).open("xb") if destination else None
            try:
                while remaining:
                    block = source.read(min(remaining, 1024 * 1024))
                    require(bool(block), "incomplete executable archive")
                    remaining -= len(block)
                    digest.update(block)
                    if output:
                        output.write(block)
                require(digest.hexdigest() == row["sha256"], "executable digest mismatch")
            finally:
                if output:
                    output.close()
            require(source.read(-row["bytes"] % 512) == b"\0" * (-row["bytes"] % 512), "invalid archive padding")
            if destination:
                (destination / row["path"]).chmod(row["mode"])
        tail = 1024 + (-(source.tell() + 1024) % 10240)
        require(source.read(tail) == b"\0" * tail and not source.read(1), "extra or incomplete archive payload")


def validate_details(path, profile, source_sha):
    root = directory(path)
    require({p.name for p in root.iterdir()} <= {"manifest.json", "executables.tar", "bin"}, "unexpected bundle file")
    manifest_path = root / "manifest.json"
    raw_manifest = read_small(manifest_path, 64 * 1024)
    manifest = json.loads(raw_manifest, object_pairs_hook=unique_object)
    require(isinstance(manifest, dict) and set(manifest) == {"version", "context", "executables", "archive"}
            and type(manifest["version"]) is int and manifest["version"] == 1, "unsupported artifact manifest")
    context = current_context(profile, source_sha)
    require(context["profile"] == profile, "artifact environment/source mismatch: profile")
    require(context["source_sha"] == source_sha, "artifact environment/source mismatch: source_sha")
    require_compatible_context(manifest["context"], context)
    identities = manifest["executables"]
    require(isinstance(identities, dict) and set(identities) == set(BINARIES), "incomplete executable manifest")
    suffix = ".exe" if context["platform"] == "Windows" else ""
    for name, row in identities.items():
        require(isinstance(row, dict) and set(row) == {"bytes", "sha256", "path", "mode", "features"}
                and type(row["bytes"]) is int and 0 < row["bytes"] <= MAX_EXECUTABLE
                and isinstance(row["sha256"], str) and re.fullmatch(r"[0-9a-f]{64}", row["sha256"])
                and row["path"] == name + suffix and type(row["mode"]) is int and row["mode"] == 0o755
                and row["features"] == [], "invalid executable identity")
    archive_identity = manifest["archive"]
    require(isinstance(archive_identity, dict) and set(archive_identity) == {"bytes", "sha256"}
            and type(archive_identity["bytes"]) is int and 0 < archive_identity["bytes"] <= MAX_ARCHIVE
            and isinstance(archive_identity["sha256"], str)
            and re.fullmatch(r"[0-9a-f]{64}", archive_identity["sha256"]), "invalid archive identity")
    archive = root / "executables.tar"
    require(file_identity(archive) == manifest["archive"], "archive digest mismatch")
    check_archive(archive, identities)
    bindir = root / "bin"
    if not bindir.exists():
        staging = Path(tempfile.mkdtemp(prefix=".bin-", dir=root))
        check_archive(archive, identities, staging)
        require(not bindir.exists(), "concurrent artifact materialization")
        staging.rename(bindir)
    directory(bindir)
    require({p.name for p in bindir.iterdir()} == {row["path"] for row in identities.values()}, "unexpected app inventory")
    for row in identities.values():
        executable = bindir / row["path"]
        require(file_identity(executable) == {key: row[key] for key in ("bytes", "sha256")}, "materialized app mismatch")
        require(os.name == "nt" or stat.S_IMODE(regular(executable).st_mode) == row["mode"], "app mode mismatch")
    provenance = {"policy": context["recipe"]["runtime"],
                  "manifest_sha256": hashlib.sha256(raw_manifest).hexdigest(),
                  "source_sha": source_sha, "profile": profile,
                  "producer_runner": manifest["context"]["runner"], "consumer_runner": context["runner"]}
    return bindir, provenance


def validate(path, profile, source_sha):
    """Validate transport/compatibility only; callers must still run runtime smoke."""
    return validate_details(path, profile, source_sha)[0]


def build(profile, output):
    context = current_context(profile, os.environ.get("GITHUB_SHA"))
    metadata = json.loads(command_output(["cargo", "metadata", "--no-deps", "--format-version", "1", "--locked"]))
    rows = []
    with subprocess.Popen(recipe(profile)["command"], cwd=ROOT, env={**os.environ, **BUILD_ENV},
                          stdout=subprocess.PIPE, text=True, encoding="utf-8") as process:
        try:
            total = 0
            while line := process.stdout.readline(4 * 1024 * 1024 + 1):
                total += len(line.encode("utf-8"))
                require(len(line) <= 4 * 1024 * 1024 and total <= 32 * 1024 * 1024,
                        "Cargo messages exceed bound")
                row = json.loads(line)
                require(isinstance(row, dict), "malformed Cargo message")
                if row.get("reason") == "compiler-message":
                    print(row.get("message", {}).get("rendered", ""), file=sys.stderr, end="")
                if row.get("reason") == "compiler-artifact":
                    rows.append(row)
        except BaseException:
            process.kill()
            process.wait()
            raise
        require(process.wait() == 0, "Cargo app build failed")
    require(current_context(profile, context["source_sha"]) == context, "build inputs changed")
    return package(select_executables(rows, profile, Path(metadata["target_directory"])), context, Path(output))


def smoke(bindir):
    """Launch help/read-only scenario check; missing runtime is a hard failure."""
    bindir = directory(bindir)
    suffix = ".exe" if os.name == "nt" else ""
    for name in sorted(BINARIES):
        executable = bindir / (name + suffix)
        regular(executable)
        issues = []
        arguments = (["check", str(ROOT / "scenarios/two-room")] if name == "tor-scenario" else ["--help"])
        output = probe([str(executable), *arguments], ROOT, issues, "application-smoke",
                       timeout=30, max_output_bytes=64 * 1024)
        require(bool(output) and not issues, "application smoke failed")
        if name == "tor-scenario":
            expected = json.loads(read_small(ROOT / "scenarios/two-room/validation.json", 64 * 1024),
                                  object_pairs_hook=unique_object)
            require(json.loads(output, object_pairs_hook=unique_object) == expected, "scenario smoke certificate mismatch")


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    producer = commands.add_parser("build")
    producer.add_argument("--output", required=True, type=Path)
    producer.add_argument("--profile", required=True, choices=("debug", "release"))
    consumer = commands.add_parser("validate")
    consumer.add_argument("directory", type=Path)
    consumer.add_argument("--profile", required=True, choices=("debug", "release"))
    consumer.add_argument("--source-sha", required=True)
    consumer.add_argument("--smoke", action="store_true")
    args = parser.parse_args(argv)
    try:
        if args.command == "build":
            result = build(args.profile, args.output)
        else:
            result, provenance = validate_details(args.directory, args.profile, args.source_sha)
        if args.command == "validate" and args.smoke:
            smoke(result)
            print(json.dumps({**provenance, "smoke": "passed"}, sort_keys=True))
        print(result)
        return 0
    except (OSError, ValueError, KeyError, TypeError) as error:
        print(f"Binary artifact rejected: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
