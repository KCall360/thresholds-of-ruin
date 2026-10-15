"""Process acceptance modules share helpers without importing other test modules."""
import ast
from pathlib import Path
import unittest


class ProcessModuleBoundaries(unittest.TestCase):
    def test_process_modules_do_not_import_other_test_modules(self):
        violations = []
        for path in sorted(Path(__file__).parent.glob("test_*_process.py")):
            for node in ast.walk(ast.parse(path.read_text(encoding="utf-8"))):
                names = ([alias.name for alias in node.names] if isinstance(node, ast.Import)
                         else [node.module or ""] if isinstance(node, ast.ImportFrom) else [])
                for name in names:
                    if any(part.startswith("test_") for part in name.split(".")):
                        violations.append(f"{path.name}:{node.lineno}: {name}")
        self.assertEqual(violations, [], "Move shared acceptance helpers into a non-test module")
