import re
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
DOC_INDEX = ROOT / "docs" / "README.md"
MARKDOWN_LINK = re.compile(r"(?<!!)\[[^]]+\]\(([^)]+)\)")


def local_target(source: Path, raw_target: str) -> Path | None:
    target = raw_target.split("#", 1)[0].strip()
    if not target or "://" in target or target.startswith("mailto:"):
        return None
    return (source.parent / target).resolve()


class DocumentationTests(unittest.TestCase):
    def test_local_markdown_links_resolve(self):
        failures = []
        sources = sorted(ROOT.glob("*.md")) + sorted((ROOT / "docs").glob("*.md"))
        for source in sources:
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


if __name__ == "__main__":
    unittest.main()
