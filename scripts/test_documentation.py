import re
import unittest
from tempfile import TemporaryDirectory
from unittest.mock import patch
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
DOC_INDEX = ROOT / "docs" / "README.md"
ROADMAP = ROOT / "docs" / "milestones.md"
MARKDOWN_LINK = re.compile(r"(?<!!)\[[^]]+\]\(([^)]+)\)")

# Versioned identifiers that name benchmark workloads or fixtures, not rulesets.
WORKLOAD_IDENTIFIERS = {
    "performance-v1",
    "place-knowledge-v1",
    "structural-horizon-v1",
    "streaming-v1",
    "durable-place-workload-v1",
    "combat-workload-v1",
}


def local_target(source: Path, raw_target: str) -> Path | None:
    target = raw_target.split("#", 1)[0].strip()
    if not target or "://" in target or target.startswith("mailto:"):
        return None
    return (source.parent / target).resolve()


def markdown_sources() -> list[Path]:
    return sorted(ROOT.glob("*.md")) + sorted((ROOT / "docs").rglob("*.md"))


def current_guides() -> list[Path]:
    """Historical checkpoints retain their own versions, but links still resolve."""
    history = ROOT / "docs" / "history"
    return [source for source in markdown_sources() if not source.is_relative_to(history)]


def heading_anchors(source: Path) -> set[str]:
    anchors = set()
    counts = {}
    fence = None
    for line in source.read_text(encoding="utf-8").splitlines():
        marker = re.match(r"^\s*(`{3,}|~{3,})", line)
        if marker:
            kind = marker.group(1)[0]
            if fence is None:
                fence = kind
            elif fence == kind:
                fence = None
            continue
        if fence is not None:
            continue
        heading = re.match(r"^#{1,6}\s+(.+?)(?:\s+#+)?$", line)
        if heading:
            slug = re.sub(r"[^\w -]", "", heading.group(1).lower()).replace(" ", "-")
            count = counts.get(slug, 0)
            counts[slug] = count + 1
            anchors.add(slug if count == 0 else f"{slug}-{count}")
    return anchors


def code_constant(relative: str, pattern: str) -> str:
    match = re.search(pattern, (ROOT / relative).read_text(encoding="utf-8"))
    if match is None:
        raise AssertionError(f"Could not find {pattern!r} in {relative}")
    return match.group(1)


def current_artifact_formats() -> set[str]:
    return {code_constant(path, r'FORMAT = "([^\"]+)"') for path in (
        "scripts/arena_search_sampling.py", "scripts/arena_search_parameters.py", "scripts/arena_search.py")}


def current_versions() -> dict[str, str]:
    return {
        "protocol": code_constant("crates/protocol/src/wire.rs", r"PROTOCOL_VERSION: u32 = (\d+);"),
        "save format": code_constant("crates/server/src/engine.rs", r"ARCHIVE_VERSION: u32 = (\d+);"),
        "ruleset": code_constant("crates/server/src/scenario_package.rs", r'RULESET: &str = "([^"]+)";'),
        "validator": code_constant("crates/server/src/scenario_package.rs", r'VALIDATOR: &str = "([^"]+)";'),
    }


class DocumentationTests(unittest.TestCase):
    def test_heading_anchors_ignore_code_and_disambiguate_duplicates(self):
        with TemporaryDirectory() as directory:
            source = Path(directory) / "guide.md"
            source.write_text(
                "# A `code` heading\n## Repeat\n## Repeat\n```text\n# Hidden\n```\n"
                "~~~text\n# Also hidden\n~~~\n",
                encoding="utf-8",
            )
            self.assertEqual({"a-code-heading", "repeat", "repeat-1"}, heading_anchors(source))

    def test_markdown_sources_include_nested_guides_and_history(self):
        with TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "docs" / "reference").mkdir(parents=True)
            (root / "docs" / "history").mkdir()
            expected = [
                root / "README.md",
                root / "docs" / "guide.md",
                root / "docs" / "reference" / "rules.md",
                root / "docs" / "history" / "review.md",
            ]
            for source in expected:
                source.write_text("# Guide\n", encoding="utf-8")
            with patch.dict(markdown_sources.__globals__, ROOT=root):
                self.assertEqual(sorted(expected), sorted(markdown_sources()))

    def test_local_markdown_links_resolve(self):
        failures = []
        for source in markdown_sources():
            for raw_target in MARKDOWN_LINK.findall(source.read_text(encoding="utf-8")):
                target = local_target(source, raw_target)
                if target is not None and not target.exists():
                    failures.append(f"{source.relative_to(ROOT)} -> {raw_target}")
                elif "#" in raw_target and "://" not in raw_target:
                    fragment = raw_target.split("#", 1)[1]
                    heading_source = target if target is not None else source
                    if (
                        fragment
                        and heading_source.suffix == ".md"
                        and fragment not in heading_anchors(heading_source)
                    ):
                        failures.append(f"{source.relative_to(ROOT)} -> {raw_target} (missing heading)")
        self.assertEqual([], failures, "Broken local Markdown links:\n" + "\n".join(failures))

    def test_every_guide_is_linked_from_the_index(self):
        indexed = {
            target
            for raw_target in MARKDOWN_LINK.findall(DOC_INDEX.read_text(encoding="utf-8"))
            if (target := local_target(DOC_INDEX, raw_target)) is not None
        }
        guides = set((ROOT / "docs").glob("*.md")) - {DOC_INDEX}
        missing = sorted(path.name for path in guides if path.resolve() not in indexed)
        self.assertEqual([], missing, "Documentation missing from docs/README.md")

    def test_roadmap_states_the_current_versions(self):
        versions = current_versions()
        line = next(
            (line for line in ROADMAP.read_text(encoding="utf-8").split("\n\n") if "**Current formats:**" in line),
            None,
        )
        self.assertIsNotNone(line, "docs/milestones.md must contain a **Current formats:** paragraph")
        stated = {
            "protocol": re.search(r"protocol \*\*(\d+)\*\*", line),
            "save format": re.search(r"save format \*\*(\d+)\*\*", line),
            "ruleset": re.search(r"ruleset\s+\*\*`([^`]+)`\*\*", line),
            "validator": re.search(r"validator\s+\*\*`([^`]+)`\*\*", line),
        }
        for name, match in stated.items():
            self.assertIsNotNone(match, f"Roadmap does not state the current {name}")
            self.assertEqual(versions[name], match.group(1), f"Roadmap {name} does not match the code")

    def test_documents_do_not_mention_other_versions(self):
        versions = current_versions()
        artifact_formats = current_artifact_formats()
        checks = [
            ("protocol", re.compile(r"\b[Pp]rotocol(?: version)?\s+\**(\d+)"), versions["protocol"]),
            ("protocol", re.compile(r'"protocol":\s*(\d+)'), versions["protocol"]),
            ("save format", re.compile(r"\bsave format\s+\**(\d+)"), versions["save format"]),
            ("validator", re.compile(r"\b(tor-scenario-\d+)\b"), versions["validator"]),
        ]
        ruleset = re.compile(r"\b([a-z]+(?:-[a-z]+)*-v\d+)\b")
        failures = []
        for source in current_guides():
            text = source.read_text(encoding="utf-8")
            name = source.relative_to(ROOT)
            for label, pattern, current in checks:
                for value in pattern.findall(text):
                    if value != current:
                        failures.append(f"{name}: {label} {value} (current is {current})")
            for token in ruleset.findall(text):
                if token not in WORKLOAD_IDENTIFIERS and token not in artifact_formats and token != versions["ruleset"]:
                    failures.append(
                        f"{name}: {token} is not the current ruleset {versions['ruleset']} "
                        "(add benchmark workload names to WORKLOAD_IDENTIFIERS)"
                    )
        self.assertEqual([], failures, "Outdated version references:\n" + "\n".join(failures))

    def test_artifact_version_changes_do_not_exempt_stale_formats_or_rulesets(self):
        versions = current_versions()
        with TemporaryDirectory(dir=ROOT / ".local") as directory:
            root = Path(directory)
            (root / "scripts").mkdir()
            (root / "scripts/arena_search_sampling.py").write_text(
                'FORMAT = "tor-arena-search-sampling-v1"', encoding="utf-8")
            (root / "scripts/arena_search_parameters.py").write_text(
                'FORMAT = "tor-arena-search-parameters-v2"', encoding="utf-8")
            (root / "scripts/arena_search.py").write_text(
                'FORMAT = "tor-arena-search-v1"', encoding="utf-8")
            guide = root / "guide.md"
            with patch.dict(globals(), ROOT=root, current_versions=lambda: versions,
                            current_guides=lambda: [guide]):
                guide.write_text("tor-arena-search-parameters-v2", encoding="utf-8")
                self.test_documents_do_not_mention_other_versions()
                for stale in ("tor-arena-search-parameters-v1", "interactions-v0"):
                    guide.write_text(stale, encoding="utf-8")
                    with self.subTest(stale=stale), self.assertRaises(AssertionError):
                        self.test_documents_do_not_mention_other_versions()

    def test_guides_do_not_cite_local_only_evidence(self):
        failures = []
        for source in current_guides():
            if not source.is_relative_to(ROOT / "docs"):
                continue
            for number, line in enumerate(source.read_text(encoding="utf-8").splitlines(), 1):
                if re.search(r"(?<![\w-])\.local/[\w{*]", line):
                    failures.append(f"{source.relative_to(ROOT)}:{number}")
        self.assertEqual([], failures, "Guides cite files that exist only on a development machine")


if __name__ == "__main__":
    unittest.main()
