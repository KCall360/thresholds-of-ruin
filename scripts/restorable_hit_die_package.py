"""Generic latest-HD operations through ordinary authored abilities and items."""
import copy
import subprocess
import tomllib

from arena_matrix import toml_source
from restorable_loss_package import prepare as prepare_loss


DRAIN = "fixture_drain_latest_hit_die"


def prepare(test, *, peer=False, choose_again=False):
    package = prepare_loss(test, peer=peer)
    path = package / "rules.toml"
    rules = tomllib.loads(path.read_text(encoding="utf-8"))
    ability = copy.deepcopy(next(row for row in rules["abilities"]
                                 if row["id"] == "fixture_lose_strength"))
    ability.update(id=DRAIN, name="Fixture drain latest hit die")
    ability["strikes"]["strikes"][0]["on_success"] = [{"kind": "drain_latest_hit_die"}]
    rules["abilities"].append(ability)
    grant = next(row for row in rules["talents"] if row["id"] == "fixture_loss_operations")
    grant["modifiers"].append({"scope": "always", "op": {"kind": "grant_ability", "ability": DRAIN}})
    path.write_text(toml_source(rules), encoding="utf-8", newline="\n")
    path = package / "scenario.toml"
    manifest = tomllib.loads(path.read_text(encoding="utf-8"))
    manifest["id"] = "restorable-hit-die-fixture"
    manifest["archetypes"]["restoration"]["consumable"]["effects"] = [{"kind": "restore_hit_die"}]
    creature = manifest["characters"][0]["creature"]
    creature["hit_dice"].append({"source": "racial", "purchases": [
        {"pool": "advancement", "selection": {"kind": "skill_rank", "skill": "lore"}},
    ]})
    if choose_again:
        creature["restoration"] = "choose_again"
        creature["future_growth"] = {"kind": "disabled"}
    path.write_text(toml_source(manifest), encoding="utf-8", newline="\n")
    compiler = test.bin / ("tor-scenario" + test.suffix)
    validated = subprocess.run([compiler, "validate", package], capture_output=True,
                               text=True, encoding="utf-8", timeout=30)
    test.assertEqual(validated.returncode, 0, validated.stderr)
    return package
