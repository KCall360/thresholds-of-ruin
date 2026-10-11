"""Resolve the built-in general rules import for authoring and arena tooling.

The compiler remains authoritative for RulesetDocument/content validation. This
helper mirrors host import/override semantics and returns a standalone document;
imports never resolve arbitrary paths or access the network.
"""
import copy
import hashlib
from pathlib import Path
import tomllib

DEFAULT_RULESET = Path(__file__).resolve().parents[1] / "crates/simulation/data/default-ruleset.toml"
MAX_BYTES = 8 * 1024 * 1024
MAX_OVERRIDES = 4096


def _document(source):
    if isinstance(source, Path):
        if source.stat().st_size > MAX_BYTES:
            raise ValueError("Ruleset exceeds source limit")
        source = source.read_text(encoding="utf-8")
    if isinstance(source, str):
        if len(source.encode("utf-8")) > MAX_BYTES:
            raise ValueError("Ruleset exceeds source limit")
        source = tomllib.loads(source)
    if not isinstance(source, dict):
        raise ValueError("Rules source must be TOML text, a Path or a document")
    return copy.deepcopy(source)


def _identity(value):
    if not isinstance(value, dict):
        return None
    value = value["id"] if "id" in value else value.get("resource")
    return value if isinstance(value, str) else None


def _apply(current, path, operation):
    catalog_member = False
    for segment in path[:-1]:
        if isinstance(current, dict) and segment in current:
            current = current[segment]
            catalog_member = False
        elif isinstance(current, list):
            current = next((value for value in current if _identity(value) == segment), None)
            if current is None:
                raise ValueError("Unknown rules override path")
            catalog_member = True
        else:
            raise ValueError("Unknown rules override path")
    segment, op = path[-1], operation["op"]
    if isinstance(current, dict):
        if catalog_member and segment in {"id", "resource"}:
            raise ValueError("Catalog identity changes require explicit remove/add")
        exists = segment in current
        if exists != (op != "add"):
            raise ValueError("Add requires a new field; set/remove require an existing field")
        if op == "remove":
            del current[segment]
        else:
            current[segment] = copy.deepcopy(operation["value"])
    elif isinstance(current, list):
        index = next((i for i, value in enumerate(current) if _identity(value) == segment), None)
        if (index is not None) != (op != "add"):
            raise ValueError("Add requires a new content ID; set/remove require an existing content ID")
        if op != "remove" and _identity(operation["value"]) != segment:
            raise ValueError("Replacement/added definition must match content ID")
        if op == "remove":
            del current[index]
        elif op == "set":
            current[index] = copy.deepcopy(operation["value"])
        else:
            current.append(copy.deepcopy(operation["value"]))
    else:
        raise ValueError("Rules override parent is not a table or catalog")


def resolve_with_dependencies(source):
    """Return the detached document and exact normalized general-source hashes."""
    document = _document(source)
    if "import" not in document:
        return document, {}
    if set(document) - {"import", "overrides"}:
        raise ValueError("Unknown shared rules import field")
    canonical_source = DEFAULT_RULESET.read_text(encoding="utf-8")
    canonical = _document(canonical_source)
    if document["import"] != canonical["id"]:
        raise ValueError("Unknown shared rules import")
    changes = document.get("overrides", [])
    if not isinstance(changes, list) or len(changes) > MAX_OVERRIDES:
        raise ValueError("Supply at most 4096 rules overrides")
    paths = []
    for change in changes:
        if (not isinstance(change, dict) or not isinstance(change.get("op"), str)
                or change["op"] not in {"set", "add", "remove"}):
            raise ValueError("Unknown rules override operation")
        expected = {"op", "path"} | ({"value"} if change["op"] != "remove" else set())
        if set(change) != expected:
            raise ValueError("Invalid rules override fields")
        path = change["path"]
        if (not isinstance(path, list) or not 1 <= len(path) <= 32 or path[0] == "id"
                or any(not isinstance(value, str) or not 1 <= len(value.encode("utf-8")) <= 128 for value in path)):
            raise ValueError("Invalid rules override path")
        if any(previous[:len(path)] == path or path[:len(previous)] == previous for previous in paths):
            raise ValueError("Overlapping rules overrides")
        paths.append(path)
    for change in changes:
        _apply(canonical, change["path"], change)
    fingerprint = hashlib.sha256(canonical_source.replace("\r\n", "\n").encode("utf-8")).hexdigest()
    return canonical, {document["import"]: fingerprint}


def resolve_document(source):
    """Resolve TOML text, a Path or a parsed source without mutating its inputs."""
    return resolve_with_dependencies(source)[0]


def load_rules(path):
    """Read a rules source by explicit filesystem path and resolve its import."""
    return resolve_document(Path(path))
