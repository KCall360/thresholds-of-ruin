"""Author the small explicit-group regression fixture, independently of Rogue."""
from pathlib import Path
from grid_scenario_authoring import build_explicit


def build(root: Path) -> None:
    build_explicit(root, "explicit-grid-groups", 3)


if __name__ == "__main__":
    build(Path(__file__).resolve().parents[1] / "scenarios/tests/explicit-grid-groups")
