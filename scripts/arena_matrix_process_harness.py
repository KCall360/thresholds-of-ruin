"""Test-only orchestration for compiling the complete curated arena matrix."""
from collections import Counter
import json
from pathlib import Path
import subprocess
import sys

from arena_matrix import FAMILIES, HD_LEVELS

ROOT = Path(__file__).resolve().parents[1]
EXCLUSIONS = {(family, 1) for family in ("mixed", "warrior_mage_dip", "mage_warrior_dip")}


def compile_matrix_families(compiler, output):
    """Keep each existing 60-second CLI deadline bounded to at most seven cases.

    Every case still uses four real compiler validations. Family inventories and
    all stdout/stderr stay separate so failures retain their original evidence.
    """
    logs = output / "compiler-logs"
    logs.mkdir(parents=True, exist_ok=False)
    batches = []
    for family in FAMILIES:
        matrix = output / family
        stdout_path = logs / (family + ".stdout.log")
        stderr_path = logs / (family + ".stderr.log")
        command = [sys.executable, str(ROOT / "scripts/arena_matrix.py"),
                   "--compiler", str(compiler), "--output", str(matrix), "--seeds", "42",
                   "--families", family, "--hd", *(str(hd) for hd in HD_LEVELS)]

        def diagnostic(reason):
            tails = []
            for path in (stdout_path, stderr_path):
                try:
                    with path.open("rb") as stream:
                        stream.seek(0, 2)
                        stream.seek(max(0, stream.tell() - 8000))
                        tail = stream.read().decode("utf-8", errors="replace")[-2000:]
                except OSError as error:
                    tail = f"Could not read log: {error}"
                tails.append(f"{path}:\n{tail}")
            return (f"Arena matrix family {family}: {reason}\nCommand: {command!r}\n"
                    f"Retained matrix: {matrix}\nCompiler output tails (full logs retained):\n"
                    + "\n".join(tails))

        try:
            with stdout_path.open("w", encoding="utf-8") as stdout, \
                    stderr_path.open("w", encoding="utf-8") as stderr:
                result = subprocess.run(command, cwd=ROOT, stdout=stdout, stderr=stderr,
                                        text=True, encoding="utf-8", timeout=60)
        except (OSError, subprocess.TimeoutExpired) as error:
            raise AssertionError(diagnostic(str(error))) from error
        if result.returncode:
            raise AssertionError(diagnostic(f"compiler CLI exited {result.returncode}"))
        try:
            inventory = json.loads((matrix / "matrix.json").read_text(encoding="utf-8"))
        except (OSError, ValueError) as error:
            raise AssertionError(diagnostic(f"missing or invalid complete inventory: {error}")) from error
        if (matrix / "partial.json").exists():
            raise AssertionError(diagnostic("complete inventory still has partial.json"))
        batches.append((matrix, inventory))
    return batches


def complete_matrix_cases(batches):
    """Reject dropped, duplicated or misfiled cases before running any encounter."""
    expected = {(family, hd) for family in FAMILIES for hd in HD_LEVELS} - EXCLUSIONS
    families = [matrix.name for matrix, _ in batches]
    if Counter(families) != Counter(FAMILIES):
        raise AssertionError(f"Incomplete or duplicate matrix families: {families!r}")
    cases, excluded = [], []
    identity = None
    for matrix, inventory in batches:
        current = {key: inventory[key] for key in
                   ("format", "compiler_sha256", "ruleset", "rules_dependencies", "seeds")}
        if identity is None:
            identity = current
        if current != identity or current["seeds"] != ["42"]:
            raise AssertionError(f"Matrix identity changed for {matrix}: {current!r}")
        for case in inventory["cases"]:
            case_id = f"{case['family']}-hd{case['hit_dice']}"
            if (case["family"] != matrix.name or case["id"] != case_id
                    or case["plan"] != case_id + "/plan.json"):
                raise AssertionError(f"Misfiled matrix case in {matrix}: {case!r}")
            cases.append((matrix, case))
        for exclusion in inventory["exclusions"]:
            if exclusion["family"] != matrix.name or not exclusion["reason"]:
                raise AssertionError(f"Misfiled matrix exclusion in {matrix}: {exclusion!r}")
            excluded.append((exclusion["family"], exclusion["hit_dice"]))
    actual = Counter((case["family"], case["hit_dice"]) for _, case in cases)
    if actual != Counter(expected):
        raise AssertionError(f"Incomplete or duplicate matrix cases: missing={Counter(expected) - actual!r}, "
                             f"unexpected={actual - Counter(expected)!r}")
    if Counter(excluded) != Counter(EXCLUSIONS):
        raise AssertionError(f"Incorrect matrix exclusions: {excluded!r}")
    return cases


def assert_matrix_ability_coverage(participant_abilities):
    """Offensive resolution allows resistance; defensive fixtures promise effects."""
    required = {name: "resolutions" for name in
                ("rattling_blow", "spark_lance_fire", "spark_lance_cold", "falter")}
    required.update({name: "applications" for name in ("brace_for_impact", "shell_impact")})
    totals = {name: 0 for name in required}
    for abilities in participant_abilities:
        for name, field in required.items():
            totals[name] += int(abilities.get(name, {}).get(field, 0))
    for name, count in totals.items():
        if count <= 0:
            raise AssertionError(f"{name} must record {required[name]} through ordinary AI")
    return totals
