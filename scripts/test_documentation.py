import ast
import io
import re
import tokenize
import unittest
from tempfile import TemporaryDirectory
from unittest.mock import patch
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
DOC_INDEX = ROOT / "docs" / "README.md"
ROADMAP = ROOT / "docs" / "status.md"
FIXED_SOURCE_ROLE = "<!-- documentation-role: fixed-source -->"
WIRE_EXAMPLE_SOURCE = ROOT / "docs" / "reference" / "protocol.md"
MARKDOWN_LINK = re.compile(r"(?<!!)\[[^]]+\]\(([^)]+)\)")

# Versioned identifiers that name benchmark workloads or fixtures, not rulesets.
WORKLOAD_IDENTIFIERS = {
    "rogue-descriptor-v2",
    "rogue-earned-baseline-v12",
    "tor-arena-run-v5",
    "performance-v1",
    "place-knowledge-v1",
    "structural-horizon-v1",
    "streaming-v1",
    "durable-place-workload-v1",
    "combat-workload-v1",
    "ordered-strike-workload-v1",
}


def local_target(source: Path, raw_target: str) -> Path | None:
    target = raw_target.split("#", 1)[0].strip()
    if not target or "://" in target or target.startswith("mailto:"):
        return None
    return (source.parent / target).resolve()


def markdown_sources() -> list[Path]:
    return sorted(ROOT.glob("*.md")) + sorted((ROOT / "docs").rglob("*.md"))


def current_guides() -> list[Path]:
    """Pinned research has its own versions, but remains linked and indexed."""
    return [source for source in markdown_sources()
            if FIXED_SOURCE_ROLE not in source.read_text(encoding="utf-8").splitlines()]


def reachable_markdown(index: Path) -> set[Path]:
    """Follow local document links, including nested indexes and cycles."""
    pending = [index.resolve()]
    visited = set()
    while pending:
        source = pending.pop()
        if source in visited or not source.is_file() or source.suffix != ".md":
            continue
        visited.add(source)
        pending.extend(target for raw in MARKDOWN_LINK.findall(source.read_text(encoding="utf-8"))
                       if (target := local_target(source, raw)) is not None
                       and target.is_relative_to(ROOT.resolve()))
    return visited


def source_document_references(source: Path) -> list[tuple[int, str]]:
    """Check narrative pointers, not arbitrary strings used as test fixtures.

    Rust: standalone line comments and println!/eprintln! help lines with
    `See docs/`. Python: comments and actual module/class/function docstrings.
    This is deliberately not a parser for all Rust strings or block comments.
    """
    text = source.read_text(encoding="utf-8")
    narratives = []
    if source.suffix == ".py":
        narratives.extend((token.start[0], token.string)
                          for token in tokenize.generate_tokens(io.StringIO(text).readline)
                          if token.type == tokenize.COMMENT)
        for node in ast.walk(ast.parse(text)):
            if isinstance(node, (ast.Module, ast.ClassDef, ast.FunctionDef, ast.AsyncFunctionDef)):
                docstring = ast.get_docstring(node, clean=False)
                if docstring is not None:
                    narratives.append((node.body[0].lineno, docstring))
    else:
        narratives.extend((number, line) for number, line in enumerate(text.splitlines(), 1)
                          if line.lstrip().startswith("//")
                          or (re.search(r"\b(?:e?println)!", line) and "See docs/" in line))
    pattern = re.compile(r"(?<![\w/])docs/[A-Za-z0-9_./-]+\.md(?:#[A-Za-z0-9_-]+)?")
    return [(number + text[:match.start()].count("\n"), match.group())
            for number, text in narratives for match in pattern.finditer(text)]


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
        "scripts/arena_search_sampling.py", "scripts/arena_search_parameters.py", "scripts/arena_search.py",
        "scripts/rogue_activation_candidate.py")}


def current_versions() -> dict[str, str]:
    return {
        "protocol": code_constant("crates/protocol/src/wire.rs", r"PROTOCOL_VERSION: u32 = (\d+);"),
        "save format": code_constant("crates/server/src/engine.rs", r"ARCHIVE_VERSION: u32 = (\d+);"),
        "scenario format": code_constant("crates/server/src/scenario_package.rs", r"pub const FORMAT: u32 = (\d+);"),
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

    def test_source_documentation_pointers_resolve(self):
        failures = []
        sources = sorted((ROOT / "crates").rglob("*.rs")) + sorted((ROOT / "scripts").glob("*.py"))
        for source in sources:
            for number, pointer in source_document_references(source):
                path, _, fragment = pointer.partition("#")
                target = ROOT / path
                if not target.is_file() or (fragment and fragment not in heading_anchors(target)):
                    failures.append(f"{source.relative_to(ROOT)}:{number} -> {pointer}")
        self.assertEqual([], failures, "Broken source documentation pointers:\n" + "\n".join(failures))

    def test_source_pointer_extraction_skips_fixtures_and_checks_missing_anchors(self):
        with TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "crates").mkdir()
            (root / "scripts").mkdir()
            (root / "docs").mkdir()
            guide = root / "docs" / "guide.md"
            guide.write_text("# Current\n", encoding="utf-8")
            rust = root / "crates" / "example.rs"
            python = root / "scripts" / "example.py"
            rust.write_text('//! See docs/guide.md#current\n'
                            'let fixture = "docs/missing.md";\n'
                            'println!("See docs/guide.md#current");\n', encoding="utf-8")
            python.write_text('"""See docs/guide.md#current."""\n'
                              '# See docs/guide.md#current\n'
                              'fixture = "docs/missing.md"\n', encoding="utf-8")
            self.assertEqual(2, len(source_document_references(rust)))
            self.assertEqual(2, len(source_document_references(python)))
            with patch.dict(globals(), ROOT=root):
                self.test_source_documentation_pointers_resolve()
                guide.write_text("# Changed\n", encoding="utf-8")
                with self.assertRaises(AssertionError):
                    self.test_source_documentation_pointers_resolve()
                guide.unlink()
                with self.assertRaises(AssertionError):
                    self.test_source_documentation_pointers_resolve()

    def test_every_guide_is_linked_from_the_index(self):
        indexed = reachable_markdown(DOC_INDEX)
        guides = set((ROOT / "docs").rglob("*.md")) - {DOC_INDEX}
        missing = sorted(str(path.relative_to(ROOT)) for path in guides
                         if path.resolve() not in indexed)
        self.assertEqual([], missing, "Documentation unreachable from docs/README.md")

    def test_nested_guides_must_be_reachable_even_when_they_form_a_cycle(self):
        with TemporaryDirectory() as directory:
            root = Path(directory)
            docs = root / "docs"
            (docs / "reference").mkdir(parents=True)
            index = docs / "README.md"
            overview = docs / "reference" / "README.md"
            leaf = docs / "reference" / "wire.md"
            index.write_text("# Docs\n", encoding="utf-8")
            overview.write_text("[Wire](wire.md)\n", encoding="utf-8")
            leaf.write_text("[Overview](README.md)\n", encoding="utf-8")
            with patch.dict(globals(), ROOT=root, DOC_INDEX=index):
                with self.assertRaises(AssertionError):
                    self.test_every_guide_is_linked_from_the_index()
                index.write_text("[Reference](reference/README.md)\n", encoding="utf-8")
                self.test_every_guide_is_linked_from_the_index()

    def test_fixed_source_roles_are_explicit_and_scoped_to_research(self):
        for source in markdown_sources():
            research = source.is_relative_to(ROOT / "docs" / "research")
            marked = FIXED_SOURCE_ROLE in source.read_text(encoding="utf-8").splitlines()
            self.assertEqual(research, marked,
                             f"{source.relative_to(ROOT)}: only pinned research uses fixed-source")

    def test_fixed_source_role_exempts_versions_but_not_discoverability(self):
        with TemporaryDirectory() as directory:
            root = Path(directory)
            research = root / "docs" / "research"
            research.mkdir(parents=True)
            index = root / "docs" / "README.md"
            index.write_text("# Docs\n", encoding="utf-8")
            pinned = research / "rules.md"
            pinned.write_text(FIXED_SOURCE_ROLE + "\nProtocol 1\n", encoding="utf-8")
            with patch.dict(globals(), ROOT=root, DOC_INDEX=index):
                self.assertEqual([index], current_guides())
                self.test_fixed_source_roles_are_explicit_and_scoped_to_research()
                with self.assertRaises(AssertionError):
                    self.test_every_guide_is_linked_from_the_index()
                index.write_text("[Research](research/rules.md)\n", encoding="utf-8")
                self.test_every_guide_is_linked_from_the_index()
                index.write_text(FIXED_SOURCE_ROLE, encoding="utf-8")
                with self.assertRaises(AssertionError):
                    self.test_fixed_source_roles_are_explicit_and_scoped_to_research()

    def test_complete_wire_examples_have_one_decoder_checked_owner(self):
        kinds = set()
        for source in markdown_sources():
            for kind in re.findall(r"^```json[ \t]+(.+)$",
                                   source.read_text(encoding="utf-8"), re.MULTILINE):
                self.assertIn(kind, {"client-message", "server-message"},
                              f"{source.relative_to(ROOT)}: unknown JSON example role")
                self.assertEqual(WIRE_EXAMPLE_SOURCE, source,
                                 "Complete wire examples belong in the decoder-tested reference")
                kinds.add(kind)
        self.assertEqual({"client-message", "server-message"}, kinds,
                         "The wire reference must include both complete envelope kinds")

    def test_wire_example_roles_reject_unknown_labels_and_wrong_owners(self):
        with TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "protocol.md"
            valid = "```json client-message\n```\n```json server-message\n```\n"
            source.write_text(valid, encoding="utf-8")
            with patch.dict(globals(), ROOT=root, WIRE_EXAMPLE_SOURCE=source):
                self.test_complete_wire_examples_have_one_decoder_checked_owner()
                source.write_text(valid + "```json client-mesage\n```\n", encoding="utf-8")
                with self.assertRaises(AssertionError):
                    self.test_complete_wire_examples_have_one_decoder_checked_owner()
                source.write_text(valid, encoding="utf-8")
                (root / "other.md").write_text(valid, encoding="utf-8")
                with self.assertRaises(AssertionError):
                    self.test_complete_wire_examples_have_one_decoder_checked_owner()

    def test_roadmap_states_the_current_versions(self):
        versions = current_versions()
        line = next(
            (line for line in ROADMAP.read_text(encoding="utf-8").split("\n\n") if "**Current formats:**" in line),
            None,
        )
        self.assertIsNotNone(line, "docs/status.md must contain a **Current formats:** paragraph")
        stated = {
            "protocol": re.search(r"protocol \*\*(\d+)\*\*", line),
            "save format": re.search(r"save format \*\*(\d+)\*\*", line),
            "scenario format": re.search(r"scenario format \*\*(\d+)\*\*", line),
            "ruleset": re.search(r"ruleset\s+\*\*`([^`]+)`\*\*", line),
            "validator": re.search(r"validator\s+\*\*`([^`]+)`\*\*", line),
        }
        for name, match in stated.items():
            self.assertIsNotNone(match, f"Roadmap does not state the current {name}")
            self.assertEqual(versions[name], match.group(1), f"Roadmap {name} does not match the code")

    def test_roadmap_rejects_stale_scenario_format(self):
        current = ROADMAP.read_text(encoding="utf-8")
        stale = re.sub(r"scenario format \*\*\d+\*\*", "scenario format **0**", current)
        self.assertNotEqual(stale, current)
        with TemporaryDirectory() as directory:
            roadmap = Path(directory) / "status.md"
            roadmap.write_text(stale, encoding="utf-8")
            with patch.dict(globals(), ROADMAP=roadmap):
                with self.assertRaisesRegex(AssertionError, "scenario format does not match"):
                    self.test_roadmap_states_the_current_versions()

    def test_roadmap_states_independent_compiled_input_versions(self):
        text = ROADMAP.read_text(encoding="utf-8")
        for label, constant in (("RulesInput", "RULES_INPUT_FORMAT"),
                                ("ContentPackage", "CONTENT_PACKAGE_FORMAT"),
                                ("RegionInput", "REGION_INPUT_FORMAT")):
            with self.subTest(boundary=label):
                current = code_constant("crates/server/src/save_scenario.rs",
                                        rf"const {constant}: u32 = (\d+);")
                stated = re.search(rf"\b{label}\s+\*\*(\d+)\*\*", text)
                self.assertIsNotNone(stated, f"Roadmap does not state the current {label}")
                self.assertEqual(current, stated.group(1))

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

    def test_artifact_regression_runs_without_preexisting_local_directory(self):
        versions = current_versions()
        with TemporaryDirectory() as directory:
            with patch.dict(globals(), ROOT=Path(directory), current_versions=lambda: versions):
                self.test_artifact_version_changes_do_not_exempt_stale_formats_or_rulesets()

    def test_artifact_version_changes_do_not_exempt_stale_formats_or_rulesets(self):
        versions = current_versions()
        (ROOT / ".local").mkdir(exist_ok=True)
        with TemporaryDirectory(dir=ROOT / ".local") as directory:
            root = Path(directory)
            (root / "scripts").mkdir()
            (root / "scripts/arena_search_sampling.py").write_text(
                'FORMAT = "tor-arena-search-sampling-v1"', encoding="utf-8")
            (root / "scripts/arena_search_parameters.py").write_text(
                'FORMAT = "tor-arena-search-parameters-v2"', encoding="utf-8")
            (root / "scripts/arena_search.py").write_text(
                'FORMAT = "tor-arena-search-v1"', encoding="utf-8")
            (root / "scripts/rogue_activation_candidate.py").write_text(
                'FORMAT = "tor-rogue-activation-inputs-v2"', encoding="utf-8")
            guide = root / "guide.md"
            with patch.dict(globals(), ROOT=root, current_versions=lambda: versions,
                            current_guides=lambda: [guide]):
                guide.write_text("tor-arena-search-parameters-v2 tor-rogue-activation-inputs-v2 rogue-earned-baseline-v12",
                                 encoding="utf-8")
                self.test_documents_do_not_mention_other_versions()
                for stale in ("tor-arena-search-parameters-v1", "tor-rogue-activation-inputs-v1",
                              "rogue-earned-baseline-v3", "interactions-v0"):
                    guide.write_text(stale, encoding="utf-8")
                    with self.subTest(stale=stale), self.assertRaises(AssertionError):
                        self.test_documents_do_not_mention_other_versions()

    def test_guides_do_not_cite_local_only_evidence(self):
        failures = []
        for source in markdown_sources():
            if not source.is_relative_to(ROOT / "docs"):
                continue
            for number, line in enumerate(source.read_text(encoding="utf-8").splitlines(), 1):
                if re.search(r"(?<![\w-])\.local/[\w{*]", line):
                    failures.append(f"{source.relative_to(ROOT)}:{number}")
        self.assertEqual([], failures, "Guides cite files that exist only on a development machine")


if __name__ == "__main__":
    unittest.main()
