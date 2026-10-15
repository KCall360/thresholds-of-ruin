"""Report a proposed CI selection; never execute it or change the mandatory gate.

The reviewed manifest pins only explicitly audited deferrals and required boundary
methods plus their relevant source closures. Every other default-discovered method,
including a new unrelated test, is retained automatically. Explicit supplemental
processes remain visible in inventory but run only in the reviewed candidate. Missing
or stale audit inputs select exhaustive Windows with recorded validation errors;
ordinary selection succeeds while check and manual candidate measurement stay strict.
Runtime inventory hashes identify all coverage; no names imply redundancy.

Examples (from the repository root, with the pinned Python environment):
    python scripts/ci_policy.py inventory
    python scripts/ci_policy.py check
    python scripts/ci_policy.py select --event pull_request --changes changes.json

`changes.json` is a complete reviewed list of {"path": ..., "status": "M"|"A"|...}
from the proposed change, including both sides of renames. Missing evidence,
deletions, risk paths and unknown paths select exhaustive Windows. This supplied
list is not a trusted CI diff; live workflow integration and qualification are
intentionally absent. `--output-dir` writes an immutable-by-convention report and
per-platform/profile ID lists for the EXISTING runner's --from-file interface.
The caller must build the binaries, provide native desktops, set TOR_TEST_PROFILE,
and retain timing evidence as usual. Nothing in this tool waives a required check.
"""
import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path, PurePosixPath
import re
import sys
import unittest

import parallel_tests
from process_harness import ProcessTestCase, is_graphical_test

ROOT = Path(__file__).resolve().parent.parent
DEFAULT_MANIFEST = ROOT / "scripts/ci_coverage_manifest.json"
REQUIRED_OBLIGATIONS = {"native_graphical", "transport_readiness", "sqlite_handles_paths",
                        "kill_checkpoint", "source_deleted_restart"}
OBLIGATIONS = REQUIRED_OBLIGATIONS | {"feature_behavior", "feature_restart", "feature_source_deleted_restart"}
ROW_FIELDS = {"test_id", "kind", "discovery", "graphical", "native_lane", "fixture_group", "source"}
HEX = re.compile(r"[0-9a-f]{64}\Z")
PHYSICS_FULL = "test_performance_process.PerformanceProcesses.test_physics_benchmark_reports_completed_durable_commit_timings"
PHYSICS_SMOKE = "physics_smoke_process.PhysicsCorrectnessSmoke.test_all_shapes_preserve_actions_draw_flush_and_resume"
# Explicit supplemental entry points only. This is not another discovery glob:
# ordinary unittest discovery and the exhaustive baseline remain unchanged.
SUPPLEMENTAL_TESTS = {PHYSICS_SMOKE: "scripts/physics_smoke_process.py"}
PHYSICS_SOURCE_CLOSURE = {
    "scripts/physics_smoke_process.py", "scripts/test_performance_process.py",
    "scripts/performance_process_support.py", "scripts/physics_performance_report.py",
    "scripts/workload_report.py", "scripts/process_harness.py", "scripts/parallel_tests.py",
    "scripts/test_runner.py", "crates/server/examples/physics_bench.rs",
    "Cargo.toml", "Cargo.lock", "crates/server/Cargo.toml",
    "crates/simulation/data/default-ruleset.toml",
    "scenarios/tests/physics/scenario.toml", "scenarios/tests/physics/rules.toml",
    "scenarios/tests/physics/index.json", "scenarios/tests/physics/validation.json",
    "scenarios/tests/physics/regions/1.toml",
}


def canonical_hash(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(",", ":"),
                                     ensure_ascii=True, allow_nan=False).encode()).hexdigest()


def read_json(path):
    def unique(pairs):
        result = {}
        for key, value in pairs:
            if key in result:
                raise ValueError(f"duplicate JSON key: {key}")
            result[key] = value
        return result
    return json.loads(Path(path).read_text(encoding="utf-8"), object_pairs_hook=unique)


def make_inventory(rows, sources):
    if not isinstance(rows, list) or not rows:
        raise ValueError("discovery must contain at least one test")
    if not isinstance(sources, dict) or any(not isinstance(p, str) or not isinstance(h, str)
                                            or not HEX.fullmatch(h) for p, h in sources.items()):
        raise ValueError("source fingerprints must be a path-to-SHA256 mapping")
    for row in rows:
        if not isinstance(row, dict) or set(row) != ROW_FIELDS:
            raise ValueError("malformed inventory row")
        if (not isinstance(row["test_id"], str) or len(row["test_id"].split(".")) < 3
                or any(c.isspace() for c in row["test_id"]) or row["kind"] not in ("process", "tool")
                or row["discovery"] not in ("default", "supplemental")
                or type(row["graphical"]) is not bool or type(row["native_lane"]) is not bool
                or row["source"] not in sources
                or (row["fixture_group"] is not None and not isinstance(row["fixture_group"], str))):
            raise ValueError("invalid inventory identity, classification or source")
    duplicates = sorted(k for k, count in Counter(row["test_id"] for row in rows).items() if count != 1)
    if duplicates:
        raise ValueError("duplicate discovered IDs: " + ", ".join(duplicates))
    payload = {"tests": sorted(rows, key=lambda row: row["test_id"]), "sources": dict(sorted(sources.items()))}
    return {**payload, "sha256": canonical_hash(payload)}


def discover(root=ROOT, *, audit_inputs=()):
    """Use unittest discovery and existing runner fixture/native metadata; run no bodies."""
    root = Path(root).resolve()
    scripts = root / "scripts"
    loader = unittest.TestLoader()
    suite = loader.discover(str(scripts), pattern="test_*.py")
    if loader.errors:
        raise ValueError("unittest discovery failed:\n" + "\n".join(loader.errors))
    tests = [(test, "default") for test in parallel_tests.flatten(suite)]
    for name, source in SUPPLEMENTAL_TESTS.items():
        if not (root / source).is_file():
            continue  # The audit rejects a missing supplemental method/source.
        supplemental = list(parallel_tests.flatten(loader.loadTestsFromName(name)))
        if loader.errors or [test.id() for test in supplemental] != [name]:
            raise ValueError("explicit supplemental discovery failed: " + name)
        tests.extend((test, "supplemental") for test in supplemental)
    rows = []
    for test, discovery in tests:
        cls = type(test)
        module = sys.modules.get(cls.__module__)
        if module is None or not getattr(module, "__file__", None):
            raise ValueError("test has no inspectable source: " + test.id())
        source = Path(module.__file__).resolve().relative_to(root).as_posix()
        if discovery == "supplemental" and source != SUPPLEMENTAL_TESTS[test.id()]:
            raise ValueError("supplemental source does not match its explicit entry point: " + test.id())
        is_process = isinstance(test, ProcessTestCase) or cls.__module__.endswith("_process")
        rows.append({"test_id": test.id(), "kind": "process" if is_process else "tool",
                     "discovery": discovery,
                     "graphical": is_graphical_test(test),
                     "native_lane": getattr(cls, "graphical", False) is True,
                     "fixture_group": (cls.__module__ + "." + cls.__qualname__
                                       if parallel_tests.has_own_fixture(cls) else None),
                     "source": source})
    # Pin helper bodies and shared fixture inputs as well as methods, not just
    # method names. LF normalization makes checkout line endings irrelevant.
    extra = set()
    for relative in audit_inputs:
        if (not isinstance(relative, str) or not relative or "\\" in relative
                or relative.startswith("/") or any(p in ("", ".", "..") for p in relative.split("/"))):
            raise ValueError("invalid audit input path")
        candidate = root / relative
        if not candidate.resolve().is_relative_to(root):
            raise ValueError("audit input must stay inside the repository")
        if candidate.is_file():
            extra.add(candidate)
    paths = sorted(set(scripts.glob("*.py")) | set((scripts / "fixtures").rglob("*")) | extra)
    sources = {p.relative_to(root).as_posix(): hashlib.sha256(p.read_bytes().replace(b"\r\n", b"\n")).hexdigest()
               for p in paths if p.is_file()}
    return make_inventory(rows, sources)


def default_test_ids(inventory):
    """Exhaustive baseline: all default tests, never duplicate explicit smokes.

    The fixed supplemental registry is also excluded if its metadata is corrupt;
    invalid supplemental evidence must never expand the mandatory baseline.
    """
    return sorted(row["test_id"] for row in inventory["tests"]
                  if row["discovery"] == "default" and row["test_id"] not in SUPPLEMENTAL_TESTS)


def audit_snapshot(inventory, test_ids, source_paths=()):
    """Build a compact explicit review record; never use it as automatic approval."""
    rows = {row["test_id"]: dict(row) for row in inventory["tests"] if row["test_id"] in test_ids}
    if set(rows) != set(test_ids):
        raise ValueError("audited test is absent from discovery")
    paths = set(source_paths) | {row["source"] for row in rows.values()}
    return {"tests": dict(sorted(rows.items())),
            "sources": {path: inventory["sources"][path] for path in sorted(paths)}}


def check(inventory, manifest):
    """Return coverage errors; any error forbids focused selection.

    Retention is the default. Only explicit, source-audited entries can defer a
    method. New unrelated methods are classified as retained automatically. Only
    protected identities/metadata and their audited source closure need re-review.
    """
    errors = []
    try:
        current = make_inventory(inventory["tests"], inventory["sources"])
        if inventory != current:
            errors.append("current inventory hash or ordering is stale")
        if (not isinstance(manifest, dict) or manifest.get("schema_version") != 1
                or type(manifest.get("schema_version")) is not int
                or manifest.get("status") != "shadow_only_unqualified"
                or not isinstance(manifest.get("review_basis"), str) or not manifest["review_basis"].strip()):
            raise ValueError("manifest requires schema 1, shadow-only status and an audit basis")
        baseline = manifest["audit"]
        if not isinstance(baseline, dict) or set(baseline) != {"tests", "sources"}:
            raise ValueError("manifest audit requires protected test metadata and source hashes")
        if not isinstance(baseline["tests"], dict):
            raise ValueError("protected test metadata must be an object")
        if (not isinstance(baseline["sources"], dict)
                or any(not isinstance(p, str) or not isinstance(h, str) or not HEX.fullmatch(h)
                       for p, h in baseline["sources"].items())):
            raise ValueError("invalid audited source fingerprints")
        actual = {row["test_id"]: row for row in current["tests"]}
        known = set(baseline["tests"])
        for name, reviewed in sorted(baseline["tests"].items()):
            if name not in actual:
                errors.append("audited test missing from discovery: " + name)
            elif actual[name] != reviewed:
                errors.append("audited fixture/native/source metadata changed: " + name)
            if not isinstance(reviewed, dict) or reviewed.get("source") not in baseline["sources"]:
                errors.append("audited test lacks a source fingerprint: " + name)
        for path, fingerprint in sorted(baseline["sources"].items()):
            if current["sources"].get(path) != fingerprint:
                errors.append("audited source changed or missing: " + path)
        deferred, required, supplemental = manifest["deferred_tests"], manifest["required_tests"], manifest["supplemental_tests"]
        if any(not isinstance(group, dict) for group in (deferred, required, supplemental)):
            raise ValueError("deferred, required and supplemental tests must be objects")
        if known != deferred.keys() | required.keys() | supplemental.keys():
            errors.append("every deferral/required/supplemental method must have a protected metadata record")
        explicit = {name for name, row in actual.items() if row["discovery"] == "supplemental"}
        if explicit != supplemental.keys() or not explicit <= SUPPLEMENTAL_TESTS.keys():
            errors.append("supplemental inventory and explicit classifications must match")
        if PHYSICS_FULL in deferred and PHYSICS_SMOKE not in supplemental:
            errors.append("physics deferral requires its audited supplemental smoke")
        for name, classification in supplemental.items():
            if (name != PHYSICS_SMOKE or not isinstance(classification, dict)
                    or set(classification) != {"reason", "replaces", "source_closure"}
                    or not isinstance(classification["reason"], str) or not classification["reason"].strip()
                    or classification["replaces"] != PHYSICS_FULL
                    or not isinstance(classification["source_closure"], list)
                    or any(not isinstance(path, str) for path in classification["source_closure"])
                    or set(classification["source_closure"]) != PHYSICS_SOURCE_CLOSURE
                    or len(classification["source_closure"]) != len(PHYSICS_SOURCE_CLOSURE)):
                errors.append("invalid audited supplemental classification: " + name)
                continue
            if (name not in known or name not in actual or actual[name]["kind"] != "process"
                    or actual[name]["discovery"] != "supplemental" or actual[name]["graphical"]
                    or actual[name]["native_lane"] or name in deferred or name in required
                    or PHYSICS_FULL not in deferred or PHYSICS_FULL not in actual
                    or actual[PHYSICS_FULL]["discovery"] != "default"):
                errors.append("supplemental replacement must be a reviewed headless process: " + name)
            for path in sorted(PHYSICS_SOURCE_CLOSURE - baseline["sources"].keys()):
                errors.append("supplemental source lacks an audited fingerprint: " + path)
            group = actual.get(name, {}).get("fixture_group")
            if group and any(row["fixture_group"] == group and row["discovery"] == "default"
                             for row in actual.values()):
                errors.append("supplemental fixture cannot share default discovery setup: " + name)
        covered = {"native_graphical"} if any(row["graphical"] for row in actual.values()) else set()
        for selection, classifications in (("deferred", deferred), ("required", required)):
            for name, classification in sorted(classifications.items()):
                if (name not in known or name not in actual or actual[name]["kind"] != "process"
                        or actual[name]["discovery"] != "default"):
                    errors.append(selection + " classification names an unknown or non-process test: " + name)
                if (not isinstance(classification, dict) or set(classification) != {"reason", "obligations"}
                        or not isinstance(classification["reason"], str) or not classification["reason"].strip()
                        or not isinstance(classification["obligations"], list) or not classification["obligations"]
                        or any(not isinstance(x, str) or x not in OBLIGATIONS for x in classification["obligations"])
                        or len(set(classification["obligations"])) != len(classification["obligations"])):
                    errors.append("invalid audited " + selection + " classification: " + name)
                    continue
                obligations = set(classification["obligations"])
                if selection == "deferred":
                    if name in required or actual.get(name, {}).get("graphical") or obligations & REQUIRED_OBLIGATIONS:
                        errors.append("required platform boundary cannot be deferred: " + name)
                else:
                    covered.update(obligations)
        for obligation in sorted(REQUIRED_OBLIGATIONS - covered):
            errors.append("missing focused boundary obligation: " + obligation)
    except (KeyError, TypeError, ValueError, AttributeError) as error:
        errors.append("invalid inventory or manifest: " + str(error))
    return errors


def change_reasons(changes):
    if not isinstance(changes, list) or not changes:
        return ["complete changed-path evidence is missing or empty"]
    reasons = []
    for row in changes:
        if not isinstance(row, dict) or set(row) != {"path", "status"}:
            reasons.append("malformed changed-path evidence")
            continue
        path, status = row["path"], row["status"]
        if (not isinstance(path, str) or not path or "\\" in path
                or path.startswith("/") or any(p in ("", ".", "..") for p in path.split("/"))):
            reasons.append("unsafe or ambiguous changed path: " + repr(path))
            continue
        if status not in ("M", "A"):
            reasons.append("deleted, renamed or unknown change status: " + path)
            continue
        # Only a small architectural allowlist is trusted. OS, transport,
        # persistence, harness, dependencies and all unclassified paths are full.
        safe_prose = (path.startswith("docs/") and PurePosixPath(path).suffix == ".md"
                      and path != "docs/reference/protocol.md")
        safe_rules = any(path.startswith(prefix) for prefix in
                         ("crates/simulation/src/", "crates/world/src/")) and path.endswith(".rs")
        platform_boundary = re.search(
            r"(?i)(?:^|[/_.-])(os|platform|windows|unix|linux|socket|network|transport|protocol|"
            r"persistence|storage|sqlite|checkpoint|build|ffi|dependencies)(?:$|[/_.-])", path)
        if platform_boundary or not (safe_prose or safe_rules):
            reasons.append("risk or unclassified path: " + path)
    return reasons


def reviewed_candidate(inventory, manifest):
    """Return reviewed candidate IDs for an explicitly authorized measurement only.

    This bypasses event/cadence selection only to measure the proposal after the
    complete mandatory pass. It never qualifies or substitutes for that pass.
    Invalid audit evidence rejects measurement; it is never relabeled as focused.
    Use the same binaries, profiles and existing verifier worker policy.
    """
    errors = check(inventory, manifest)
    if errors:
        raise ValueError("candidate audit is invalid: " + "; ".join(errors))
    rows = inventory["tests"]
    selected = set(default_test_ids(inventory)) - manifest["deferred_tests"].keys()
    # Keep the existing runner's fixture units whole; its scheduler still owns
    # native-lane serialization and all batching/concurrency decisions.
    groups = {row["fixture_group"] for row in rows if row["test_id"] in selected and row["fixture_group"]}
    selected.update(row["test_id"] for row in rows
                    if row["discovery"] == "default" and row["fixture_group"] in groups)
    # A full method promoted by a shared fixture wins over its replacement.
    selected.update(name for name, info in manifest["supplemental_tests"].items()
                    if info["replaces"] not in selected)
    return sorted(selected)


def select(inventory, manifest, *, event, changes):
    errors = check(inventory, manifest)
    reasons = ["coverage validation failed"] if errors else []
    if event != "pull_request":
        reasons.append("exhaustive event: " + str(event))
    reasons.extend(change_reasons(changes))
    exhaustive = bool(reasons)
    rows = inventory["tests"]
    all_ids = default_test_ids(inventory)
    deferred = manifest.get("deferred_tests", {}) if isinstance(manifest, dict) else {}
    required = manifest.get("required_tests", {}) if isinstance(manifest, dict) else {}
    selected = all_ids if exhaustive else reviewed_candidate(inventory, manifest)
    promotions = sorted(set(selected) & deferred.keys()) if not exhaustive else []
    return {"schema_version": 1, "shadow_only": True, "qualified": False,
            "mandatory_checks_changed": False, "qualification": "unmeasured; all current checks remain required",
            "inventory_sha256": inventory.get("sha256"), "manifest_sha256": canonical_hash(manifest),
            "event": event, "changes": changes, "windows_mode": "exhaustive" if exhaustive else "focused",
            "reasons": reasons or ["reviewed coverage and allowlisted change; proposal only"],
            "validation_errors": errors, "fixture_promotions": sorted(promotions),
            "python": {"linux": {"debug": all_ids, "release": all_ids},
                       "windows": {"debug": selected, "release": selected}},
            "rust": {"linux": ["debug", "release"], "windows": ["debug", "release"] if exhaustive else ["debug"]},
            "deferred_windows": sorted(set(all_ids) - set(selected)),
            "supplemental_windows": sorted(set(selected) - set(all_ids)),
            "counts": {"all_tests": len(rows), "default_tests": len(all_ids),
                       "supplemental_tests": sum(row["discovery"] == "supplemental" for row in rows),
                       "all_process": sum(row["kind"] == "process" for row in rows),
                       "all_tools": sum(row["kind"] == "tool" for row in rows),
                       "native_methods": sum(row["graphical"] for row in rows),
                       "windows_selected": len(selected),
                       "windows_process": sum(row["kind"] == "process" and row["test_id"] in selected for row in rows)},
            "limitations": ["Deferred methods retain unique semantic coverage; they are not duplicates.",
                            "Linux runs them in both profiles; Windows-specific defects may be detected later.",
                            "No runtime/cost saving or qualification is established by this report.",
                            "Supplied changed-path evidence is not authenticated against a Git comparison."],
            "audited_deferrals": deferred, "required_boundaries": required,
            "audited_supplements": manifest.get("supplemental_tests", {}) if isinstance(manifest, dict) else {},
            "default_selection": "retain every other default-discovered test; all native methods required; explicit supplements are candidate-only",
            "runner": {"arguments": ["scripts/test_runner.py", "--workers", "<existing-verify-worker-count>", "--from-file",
                                      "<platform>-<profile>.ids"],
                       "worker_policy": "Use unchanged verify.test_workers(free_memory_gb(), os.cpu_count()) or the same explicit verifier override as the full comparison; do not change concurrency for selection measurements.",
                       "profile_environment": "TOR_TEST_PROFILE=<profile>",
                       "fixture_and_native_scheduler": "unchanged parallel_tests.plan",
                       "execution": "not run; report and ID lists only"},
            "unchanged_requirements": ["all 13 existing checks", "tooling lint", "dependency policy and advisories",
                                       "native desktops", "exact-head measured Windows/Linux qualification"]}


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("command", choices=("inventory", "check", "select"))
    parser.add_argument("--root", type=Path, default=ROOT)
    parser.add_argument("--manifest", type=Path, default=DEFAULT_MANIFEST)
    parser.add_argument("--event", default="unknown")
    parser.add_argument("--changes", type=Path, help="complete reviewed JSON path/status list; omitted means exhaustive")
    parser.add_argument("--output-dir", type=Path)
    args = parser.parse_args(argv)
    try:
        try:
            manifest = read_json(args.manifest)
        except (OSError, ValueError) as error:
            manifest = {"load_error": str(error)}
        audit_inputs = manifest.get("audit", {}).get("sources", {}) if isinstance(manifest, dict) and isinstance(manifest.get("audit", {}), dict) else {}
        if not isinstance(audit_inputs, dict):
            audit_inputs = {}
        inventory = discover(args.root, audit_inputs=audit_inputs)
        if args.command == "inventory":
            report, status = inventory, 0
        else:
            if args.command == "check":
                errors = check(inventory, manifest)
                report, status = {"shadow_only": True, "inventory_sha256": inventory["sha256"],
                                  "validation_errors": errors}, int(bool(errors))
            else:
                changes = read_json(args.changes) if args.changes else None
                report = select(inventory, manifest, event=args.event, changes=changes)
                # A stale review is a successful exhaustive fallback, not an
                # instruction to refresh hashes before an unrelated change.
                status = 0
                if args.output_dir:
                    args.output_dir.mkdir(parents=True, exist_ok=True)
                    for platform, profiles in report["python"].items():
                        for profile, ids in profiles.items():
                            (args.output_dir / f"{platform}-{profile}.ids").write_text("\n".join(ids) + "\n", encoding="utf-8")
                    (args.output_dir / "shadow-report.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
        print(json.dumps(report, indent=2, sort_keys=True))
        return status
    except (OSError, ValueError) as error:
        print(json.dumps({"shadow_only": True, "fatal_error": str(error), "execution": "not run"}), file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
