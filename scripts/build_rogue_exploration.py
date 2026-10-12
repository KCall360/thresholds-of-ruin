"""Author the exploration package; tor-scenario validation supplies certificates."""
from pathlib import Path
from grid_scenario_authoring import build_explicit


def build(root: Path) -> None:
    build_explicit(root, "rogue-exploration", 26)


if __name__ == "__main__":
    build(Path(__file__).resolve().parents[1] / "scenarios" / "rogue-exploration")
