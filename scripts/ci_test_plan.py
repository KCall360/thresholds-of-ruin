"""Canonical selected-PR plan; fresh exhaustive coverage remains a separate mode.

Missing scope, unreviewed inventories and unknown ownership fail closed to full.
Only the trusted Git event/checkout relationship supplies paths in CI. A plan is
not test evidence and may never qualify an exhaustive run.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import unittest

import ci_scope
import parallel_tests

ROOT = Path(__file__).resolve().parent.parent
POLICY_VERSION = 3
SHA = re.compile(r"[0-9a-f]{40}\Z")
TEST_ID = re.compile(r"[A-Za-z_]\w*(?:\.[A-Za-z_]\w*){2,}\Z")
CONTROL_PREFIXES = (".github/", ".cargo/", "scripts/ci_", "scripts/test_ci_",
                    "scripts/test_runner.py", "scripts/parallel_tests.py", "scripts/verify.py",
                    "scripts/process_harness.py", "scripts/graphical_waiver.py",
                    "scripts/service_contract_ci.py", "scripts/service_contract_host.py")
CONTROL_FILES = {"Cargo.toml", "Cargo.lock", "rust-toolchain.toml", "deny.toml"}
LANES = (("linux-debug-rust", "ubuntu-latest", "debug", "rust"),
         ("linux-debug-python", "ubuntu-latest", "debug", "python"),
         ("windows-debug-python", "windows-latest", "debug", "python"),
         ("linux-release-python", "ubuntu-latest", "release", "python"))
SUPPLEMENTAL = ("physics_smoke_process.PhysicsCorrectnessSmoke.test_all_shapes_preserve_actions_draw_flush_and_resume",)


def require(condition, reason):
    if not condition:
        raise ValueError(reason)


def digest(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(",", ":"), allow_nan=False).encode()).hexdigest()


def diff_paths(raw):
    """Parse bounded --raw --no-renames records, retaining old and new sides."""
    require(len(raw) <= ci_scope.MAX_DIFF_BYTES and (not raw or raw.endswith(b"\0")), "invalid diff framing")
    fields = raw.split(b"\0")[:-1]
    require(len(fields) % 2 == 0 and len(fields) <= 20000, "invalid diff records")
    paths = []
    for meta, name in zip(fields[::2], fields[1::2]):
        parts = meta.decode("ascii").split()
        require(len(parts) == 5, "invalid raw metadata")
        before, after, old, new, status = parts
        require(SHA.fullmatch(old) and SHA.fullmatch(new), "invalid blob identity")
        require((before, after, status) in ((":100644", "100644", "M"), (":100755", "100755", "M"),
                (":000000", "100644", "A"), (":100644", "000000", "D"),
                (":000000", "100755", "A"), (":100755", "000000", "D")), "unsupported file mode/change")
        path = name.decode("utf-8")
        require(re.fullmatch(r"[A-Za-z0-9_./-]+", path) and not path.startswith("/")
                and all(part not in ("", ".", "..") for part in path.split("/")) and path not in paths,
                "invalid or duplicate path")
        paths.append(path)
    return sorted(paths)


def scope_paths(root, event_name, event, source_sha):
    require(event_name in ("pull_request", "push"), "selected mode requires a pull request or main push")
    require(isinstance(source_sha, str) and SHA.fullmatch(source_sha), "invalid checkout SHA")
    git = lambda *args: ci_scope.git(root, *args)
    require(git("rev-parse", "--is-shallow-repository").strip() == b"false", "shallow scope")
    require(git("rev-parse", "HEAD").decode().strip() == source_sha, "checkout does not match event")
    if event_name == "pull_request":
        base, head = (event["pull_request"][key]["sha"] for key in ("base", "head"))
        require(all(isinstance(sha, str) and SHA.fullmatch(sha) for sha in (base, head)), "invalid PR identities")
        parents = git("rev-list", "--parents", "-n", "1", source_sha).decode().split()
        require(parents == [source_sha, base, head] and base != head, "unsupported synthetic merge")
        ancestors = git("merge-base", "--all", base, head).decode().split()
        require(len(ancestors) == 1 and SHA.fullmatch(ancestors[0]), "ambiguous merge base")
        ranges = ((ancestors[0], head), (base, source_sha))
    else:
        base, head = event["before"], event["after"]
        require(event.get("created") is False and event.get("deleted") is False and event.get("forced") is False
                and event.get("ref") == "refs/heads/main" and head == source_sha
                and isinstance(base, str) and SHA.fullmatch(base) and base != "0" * 40 and base != head,
                "unsupported main push")
        git("merge-base", "--is-ancestor", base, head)
        ranges = ((base, head),)
    paths = set()
    for before, after in ranges:
        selected = diff_paths(git("diff", "--raw", "--no-abbrev", "-z", "--no-renames", "--no-ext-diff",
                                  "--no-textconv", "--ignore-submodules=none", before, after, "--"))
        require(selected, "empty change evidence")
        paths.update(selected)
    return sorted(paths)


def discover_inventory(root=ROOT):
    """Independent current-source inventory; discovery must not run test setup."""
    loader = unittest.TestLoader()
    suite = loader.discover(str(root / "scripts"), pattern="test_*.py", top_level_dir=str(root / "scripts"))
    tests = list(parallel_tests.flatten(suite))
    for name in SUPPLEMENTAL:
        tests.extend(parallel_tests.flatten(loader.loadTestsFromName(name)))
    ids = [test.id() for test in tests]
    require(not loader.errors and ids and len(ids) == len(set(ids))
            and all(TEST_ID.fullmatch(name) and not name.startswith("unittest.loader.") for name in ids),
            "incomplete current-source discovery")
    from process_harness import ProcessTestCase, is_graphical_test
    from ci_test_contracts import APPLICATION_OWNERS
    metadata = {test.id(): {"process": isinstance(test, ProcessTestCase)
                           or tuple(test.id().split(".")[:2]) in APPLICATION_OWNERS,
                           "graphical": is_graphical_test(test)} for test in tests}
    return sorted(ids), metadata


def discover_ids(root=ROOT):
    return discover_inventory(root)[0]


def full_plan(source_sha, tree, reasons):
    return {"policy_version": POLICY_VERSION, "mode": "full", "assurance": "exhaustive-matrix",
            "source_sha": source_sha, "source_tree": tree, "reasons": reasons, "paths": [],
            "contracts": [], "inventory_sha256": None, "lanes": [], "consumer_3d": True}


def make_plan(event_name, source_sha, tree, paths, inventory, selection):
    result = full_plan(source_sha, tree, ["event requires exhaustive qualification"])
    if event_name not in ("pull_request", "push"):
        return result
    if not paths or any(path in CONTROL_FILES or path.startswith(CONTROL_PREFIXES)
                        or path.endswith(("/Cargo.toml", "/Cargo.lock", "/build.rs")) for path in paths):
        result["reasons"] = ["missing scope or changed verification/build controls"]
        return result
    try:
        require(isinstance(selection, dict) and selection.get("full_required") is False, "unreviewed contract ownership")
        require(type(selection["consumer_3d"]) is bool, "invalid consumer selection")
        require(isinstance(selection["contracts"], list) and selection["contracts"]
                and all(isinstance(name, str) and name for name in selection["contracts"]), "missing contracts")
        require(type(selection["requires_rust"]) is bool and type(selection["requires_apps"]) is bool
                and (not selection["requires_rust"] or selection["requires_apps"]), "invalid build requirements")
        apps, rust = selection["requires_apps"], selection["requires_rust"]
        selected = {}
        for key in ("linux_debug", "windows_debug", "linux_release"):
            ids = selection[key]
            require(isinstance(ids, list) and (ids or key == "linux_release" and not apps) and len(ids) == len(set(ids))
                    and all(isinstance(name, str) and TEST_ID.fullmatch(name) and name in inventory for name in ids),
                    "incomplete selected test identities")
            selected[key] = sorted(ids)
        lanes = [{"id": name, "os": os_name, "profile": profile, "partition": part,
                  "test_ids": [] if part == "rust" else selected[("windows" if os_name == "windows-latest" else "linux") + "_" + profile]}
                 for name, os_name, profile, part in LANES]
        if not apps:
            require(not selection["consumer_3d"], "consumer requires real builds")
            lanes = [{"id": prefix + "-tooling", "os": os_name, "profile": "debug", "partition": "tooling",
                      "test_ids": selected[prefix + "_debug"]} for prefix, os_name in (("linux", "ubuntu-latest"), ("windows", "windows-latest"))]
        elif not rust:
            lanes[0] = {**lanes[0], "id": "linux-debug-build", "partition": "build"}
        if selection["consumer_3d"]:
            lanes.append({"id": "linux-3d-consumer", "os": "ubuntu-latest", "profile": "debug", "partition": "consumer", "test_ids": []})
        result.update(mode="selected", assurance="selected-pr-contracts", paths=sorted(paths),
                      contracts=sorted(selection["contracts"]), reasons=selection["reasons"],
                      inventory_sha256=digest(sorted(inventory)), requires_apps=apps, requires_rust=rust, consumer_3d=selection["consumer_3d"], lanes=lanes)
    except (ValueError, KeyError, TypeError):
        result["reasons"] = ["unreviewed or invalid contract selection", *selection.get("reasons", [])] if isinstance(selection, dict) else ["invalid registry"]
    return result


def normal_plan(*, root=ROOT, event_name=None, event=None, source_sha=None):
    event_name = event_name or os.environ.get("GITHUB_EVENT_NAME", "local")
    source_sha = source_sha or os.environ.get("GITHUB_SHA", "")
    tree = ""
    try:
        tree = ci_scope.git(root, "rev-parse", "HEAD^{tree}").decode().strip()
        if event_name not in ("pull_request", "push"):
            return full_plan(source_sha, tree, ["event requires exhaustive qualification"])
        if event is None:
            raw = Path(os.environ["GITHUB_EVENT_PATH"]).read_bytes()
            require(len(raw) <= ci_scope.MAX_DIFF_BYTES, "event budget")
            event = json.loads(raw)
        paths = scope_paths(root, event_name, event, source_sha)
        # Controls must choose full even if changed registry code would narrow it.
        provisional = make_plan(event_name, source_sha, tree, paths, [], {})
        if provisional["reasons"] == ["missing scope or changed verification/build controls"]:
            return provisional
        inventory, metadata = discover_inventory(root)
        import ci_test_contracts
        selection = ci_test_contracts.select(paths, inventory, root=root, test_metadata=metadata)
        result = make_plan(event_name, source_sha, tree, paths, inventory, selection)
        if result["mode"] == "selected":
            result["inventory_sha256"] = digest({"ids": inventory, "metadata": metadata})
        return result
    except (OSError, ValueError, TypeError, KeyError, ImportError, subprocess.SubprocessError):
        return full_plan(source_sha, tree, ["scope, discovery or ownership evidence unavailable"])


def current_plan(*, root=ROOT, event_name=None, event=None, source_sha=None):
    name = event_name or os.environ.get('GITHUB_EVENT_NAME', 'local')
    normal = normal_plan(root=root, event_name=name, event=event, source_sha=source_sha)
    # Preserve ordinary selected/prose behavior. Only otherwise-full main
    # pushes may use an exact-tree prior-full qualification; all other events
    # and any unavailable authority retain their normal assurance requirement.
    if name != 'push' or normal['mode'] != 'full':
        return normal
    try:
        if event is None:
            raw = Path(os.environ['GITHUB_EVENT_PATH']).read_bytes()
            require(len(raw) <= ci_scope.MAX_DIFF_BYTES, 'event budget')
            event = json.loads(raw)
        if ci_scope.classify(root, name, event, normal['source_sha'])['docs_only']:
            return normal
        import ci_postmerge
        return ci_postmerge.resolve(normal, event, root)
    except (OSError, ValueError, TypeError, KeyError, ImportError, subprocess.SubprocessError):
        return normal


def local_plan(base, root=ROOT):
    """Explain working-copy selection only; never emit CI qualification evidence."""
    sha, tree = "", ""
    try:
        sha = ci_scope.git(root, "rev-parse", "HEAD").decode().strip()
        tree = ci_scope.git(root, "rev-parse", "HEAD^{tree}").decode().strip()
        require(isinstance(base, str) and base and not base.startswith("-"), "invalid local base")
        ancestors = ci_scope.git(root, "merge-base", "--all", base, "HEAD").decode().split()
        require(len(ancestors) == 1 and SHA.fullmatch(ancestors[0]), "ambiguous local base")
        paths = set(diff_paths(ci_scope.git(root, "diff", "--raw", "--no-abbrev", "-z", "--no-renames",
                                           "--no-ext-diff", "--no-textconv", ancestors[0], "HEAD", "--")))
        paths.update(diff_paths(ci_scope.git(root, "diff", "--raw", "--no-abbrev", "-z", "--no-renames",
                                            "--no-ext-diff", "--no-textconv", "HEAD", "--")))
        # Untracked executable inputs are not reviewable ownership evidence.
        require(not ci_scope.git(root, "ls-files", "--others", "--exclude-standard").strip(), "untracked source")
        inventory, metadata = discover_inventory(root)
        import ci_test_contracts
        selection = ci_test_contracts.select(sorted(paths), inventory, root=root, test_metadata=metadata)
        result = make_plan("pull_request", sha, tree, sorted(paths), inventory, selection)
    except (OSError, ValueError, TypeError, KeyError, ImportError, subprocess.SubprocessError):
        result = full_plan(sha, tree, ["local scope, discovery or ownership evidence unavailable"])
    result.update(assurance="local-proposed-plan-not-qualification", local_base=base)
    return result


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base", help="Explain local working-copy scope against this ref; never CI qualification")
    parser.add_argument("--output", type=Path, default=ROOT / ".local/ci-plan/plan.json")
    args = parser.parse_args(argv)
    require(not (args.base and os.environ.get("GITHUB_OUTPUT")), "local plans cannot set CI job outputs")
    result = local_plan(args.base) if args.base else current_plan()
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"mode": result["mode"], "assurance": result["assurance"], "reasons": result["reasons"],
                      "contracts": result["contracts"], "plan_sha256": digest(result)}, sort_keys=True))
    if output := os.environ.get("GITHUB_OUTPUT"):
        matrix = {"include": [{key: row[key] for key in ("id", "os", "profile", "partition")} for row in result["lanes"]]}
        with Path(output).open("a", encoding="utf-8") as handle:
            handle.write("mode=" + result["mode"] + "\n")
            handle.write("postmerge_release=" + result.get('postmerge_proof', {}).get('current_toolchain', {}).get('release', '') + "\n")
            handle.write("consumer_3d=" + ("true" if result["consumer_3d"] else "false") + "\n")
            handle.write("requires_apps=" + ("true" if result.get("requires_apps") else "false") + "\n")
            handle.write("build_lane=" + ("linux-debug-rust" if result.get("requires_rust") else "linux-debug-build") + "\n")
            handle.write("linux_lane=" + ("linux-debug-python" if result.get("requires_apps") else "linux-tooling") + "\n")
            handle.write("windows_lane=" + ("windows-debug-python" if result.get("requires_apps") else "windows-tooling") + "\n")
            handle.write("selected_matrix=" + json.dumps(matrix, separators=(",", ":")) + "\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
