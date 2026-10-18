"""Ordinary authored two-holder content shared by Service and process witnesses."""
import copy
from pathlib import Path
import shutil
import subprocess
import tomllib

from arena_matrix import toml_source

ROOT = Path(__file__).resolve().parent.parent
LINK = "fixture_linked_hold"
NAMES = ("Escape subject", "First private holder", "Second private holder")


def fixture(*, hidden=True, success=True):
    manifest = tomllib.loads((ROOT / "scenarios/two-room/scenario.toml").read_text(encoding="utf-8"))
    rules = tomllib.loads((ROOT / "crates/simulation/data/default-ruleset.toml").read_text(encoding="utf-8"))
    policy = {"sustaining": "clutch", "preparation": 100, "recovery": 100, "protection": 400}
    strike = {"offset": 0, "on_success": [{"kind": "damage", "damage": {"kind": "components",
        "components": [{"category": "impact", "amount": {"kind": "fixed", "amount": 0}}]}}],
        "on_failure": [], "descriptors": [], "riders": [{"trigger": "success",
            "effects": [{"kind": "apply_linked_hold", "policy": policy}], "descriptors": []}]}
    rules["abilities"].append({"id": LINK, "name": "Fixture Contact", "tags": ["attack"],
        "basic_attack": False, "invocation": "actor_target", "interaction": "hostile_to_target",
        "timing": {"source": {"kind": "fixed", "prepare": 25, "recover": 25}},
        "reach": {"kind": "melee"}, "strikes": {"kind": "authored", "strikes": [strike]},
        "ai": {"priority": 0, "hostile": True, "minimum_resource_percent": 0, "range_preference": "any"}})
    attributes = dict.fromkeys(("strength", "speed", "intellect", "willpower", "awareness", "presence"), 0)
    species = {"kind": "humanoid", "attributes": attributes,
        "melee": {"skill": "heavy_weaponry", "bonus": 0, "wind_up": 100, "recovery": 100,
            "strikes": [{"offset": 0, "on_success": [], "on_failure": [], "descriptors": [], "riders": []}]},
        "modifiers": [{"scope": "always", "op": {"kind": "flag", "flag": "clutch"}},
            {"scope": "always", "op": {"kind": "grant_ability", "ability": LINK}},
            {"scope": "always", "op": {"kind": "add", "stat": {"kind": "health"}, "value": 200}}]}
    subject, holder = copy.deepcopy(species), copy.deepcopy(species)
    (subject if success else holder)["modifiers"].append({"scope": "always",
        "op": {"kind": "add", "stat": {"kind": "attribute", "attribute": "strength"}, "value": 100}})
    manifest["creatures"] = {"species": {"escape_subject": subject, "escape_holder": holder}}

    def creature(index):
        return {"species": "escape_subject" if index == 0 else "escape_holder", "name": NAMES[index],
            "faction": "neutral", "binding": "intellect", "starting_skills": ["heavy_weaponry", "athletics"],
            "restoration": "authored", "nutrition": {"kind": "exempt"}, "hit_dice": [{"source": "racial"}]}

    manifest["characters"][0]["creature"] = creature(0)
    body = copy.deepcopy(manifest["characters"][0]["body"])
    positions = ((1, [5, 1, 0]), (2, [5, 3, 0])) if hidden else ((1, [2, 1, 0]), (2, [1, 2, 0]))
    lit_columns = ((1, 1), (5, 2)) if hidden else ((1, 1), (2, 2))
    region = {"id": 1, "name": "Linked hold room", "size": [6, 4, 2], "chamber": True,
        "gravity": [0, 0, -1], "anchors": {"start": [1, 1, 0]}, "lit": not hidden,
        "lighting": [{"at": [x, y, z], "lit": True} for x, y in lit_columns for z in (0, 1)],
        "actors": [{"id": index + 1, "at": position, "turn_ticks": 100, "controller": "external",
            "body": body, "creature": creature(index)} for index, position in positions]}
    if hidden:
        # Body-local awareness discloses all 26 neighbours even in darkness.
        # Directed physical portals give each remote holder ordinary melee
        # contact without giving the subject a reverse view of the holder.
        # Moving the first holder south enters light but preserves that contact;
        # moving north hides the same endpoint again, without replacing its link.
        region["portals"] = [{"at": [5, y, 0], "direction": "east", "to": "1/start",
            "kind": "portal", "height": 2} for y in (1, 2, 3)]
    return {"manifest": manifest, "regions": [region], "rules": toml_source(rules), "seed": 42,
        "wizard": True, "frame_bytes": 65536, "client_bytes": 262144, "total_bytes": 524288}


def prepare(test, *, hidden=False, success=True):
    data = fixture(hidden=hidden, success=success)
    package = test.directory / "linked-holds"
    shutil.copytree(ROOT / "scenarios/two-room", package)
    (package / "scenario.toml").write_text(toml_source(data["manifest"]), encoding="utf-8", newline="\n")
    for path in (package / "regions").glob("*.toml"):
        path.unlink()
    for region in data["regions"]:
        (package / "regions" / f"{region['id']}.toml").write_text(toml_source(region), encoding="utf-8", newline="\n")
    (package / "rules.toml").write_text(data["rules"], encoding="utf-8", newline="\n")
    result = subprocess.run([test.bin / ("tor-scenario" + test.suffix), "validate", package],
        capture_output=True, text=True, encoding="utf-8", timeout=30)
    test.assertEqual(result.returncode, 0, result.stderr)
    return package
