import re
import unittest
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
    return sorted(ROOT.glob("*.md")) + sorted((ROOT / "docs").glob("*.md"))


def code_constant(relative: str, pattern: str) -> str:
    match = re.search(pattern, (ROOT / relative).read_text(encoding="utf-8"))
    if match is None:
        raise AssertionError(f"Could not find {pattern!r} in {relative}")
    return match.group(1)


def current_versions() -> dict[str, str]:
    return {
        "protocol": code_constant("crates/protocol/src/wire.rs", r"PROTOCOL_VERSION: u32 = (\d+);"),
        "save format": code_constant("crates/server/src/engine.rs", r"ARCHIVE_VERSION: u32 = (\d+);"),
        "ruleset": code_constant("crates/server/src/scenario_package.rs", r'RULESET: &str = "([^"]+)";'),
        "validator": code_constant("crates/server/src/scenario_package.rs", r'VALIDATOR: &str = "([^"]+)";'),
    }


class DocumentationTests(unittest.TestCase):
    def test_local_markdown_links_resolve(self):
        failures = []
        for source in markdown_sources():
            for raw_target in MARKDOWN_LINK.findall(source.read_text(encoding="utf-8")):
                target = local_target(source, raw_target)
                if target is not None and not target.exists():
                    failures.append(f"{source.relative_to(ROOT)} -> {raw_target}")
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
        checks = [
            ("protocol", re.compile(r"\b[Pp]rotocol(?: version)?\s+\**(\d+)"), versions["protocol"]),
            ("protocol", re.compile(r'"protocol":\s*(\d+)'), versions["protocol"]),
            ("save format", re.compile(r"\bsave format\s+\**(\d+)"), versions["save format"]),
            ("validator", re.compile(r"\b(tor-scenario-\d+)\b"), versions["validator"]),
        ]
        ruleset = re.compile(r"\b([a-z]+(?:-[a-z]+)*-v\d+)\b")
        failures = []
        for source in markdown_sources():
            text = source.read_text(encoding="utf-8")
            name = source.relative_to(ROOT)
            for label, pattern, current in checks:
                for value in pattern.findall(text):
                    if value != current:
                        failures.append(f"{name}: {label} {value} (current is {current})")
            for token in ruleset.findall(text):
                if token not in WORKLOAD_IDENTIFIERS and token != versions["ruleset"]:
                    failures.append(
                        f"{name}: {token} is not the current ruleset {versions['ruleset']} "
                        "(add benchmark workload names to WORKLOAD_IDENTIFIERS)"
                    )
        self.assertEqual([], failures, "Outdated version references:\n" + "\n".join(failures))

    def test_guides_do_not_cite_local_only_evidence(self):
        failures = []
        for source in sorted((ROOT / "docs").glob("*.md")):
            for number, line in enumerate(source.read_text(encoding="utf-8").splitlines(), 1):
                if re.search(r"(?<![\w-])\.local/[\w{*]", line):
                    failures.append(f"{source.relative_to(ROOT)}:{number}")
        self.assertEqual([], failures, "Guides cite files that exist only on a development machine")


if __name__ == "__main__":
    unittest.main()
