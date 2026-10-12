"""Thin host adapter for the authoritative Rust rules-source resolver.

Imports, overrides and dependency identities are owned exclusively by
``tor-scenario resolve-rules``. This module never builds the compiler implicitly.
"""
import json
import os
from pathlib import Path
import subprocess

MAX_BYTES = 8 * 1024 * 1024

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_RULESET = ROOT / "crates/simulation/data/default-ruleset.toml"
DEFAULT_COMPILER = Path(os.environ.get("TOR_SCENARIO_COMPILER",
    str(Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target")) / os.environ.get("TOR_TEST_PROFILE", "debug")
        / ("tor-scenario.exe" if os.name == "nt" else "tor-scenario"))))


def resolve_with_dependencies(source, compiler=None):
    """Resolve TOML text, a Path or a parsed document using the chosen compiler."""
    if isinstance(source, Path):
        with source.open("rb") as file:
            raw = file.read(MAX_BYTES + 1)
        if len(raw) > MAX_BYTES:
            raise ValueError("Ruleset exceeds source limit")
        source = raw.decode("utf-8")
    elif isinstance(source, dict):
        from arena_matrix import toml_source
        source = toml_source(source)
    if not isinstance(source, str):
        raise ValueError("Rules source must be TOML text, a Path or a document")
    payload = source.encode("utf-8")
    if len(payload) > MAX_BYTES:
        raise ValueError("Ruleset exceeds source limit")
    executable = Path(compiler) if compiler is not None else DEFAULT_COMPILER
    try:
        # Binary pipes preserve authored CRLF on Windows. Text-mode stdin would
        # expand its LF again; normalizing the source would mask malformed TOML.
        result = subprocess.run([str(executable.resolve(strict=True)), "resolve-rules"], input=payload,
                                capture_output=True, timeout=30)
    except (OSError, subprocess.TimeoutExpired) as error:
        raise ValueError(f"Cannot run rules resolver {executable}: {error}") from error
    if result.returncode:
        diagnostic = (result.stderr or result.stdout).decode("utf-8", errors="replace").strip()
        raise ValueError(diagnostic or "Rules resolution failed")
    try:
        resolved = json.loads(result.stdout)
        document, dependencies = resolved["document"], resolved["dependencies"]
        if not isinstance(document, dict) or not isinstance(dependencies, dict):
            raise ValueError("Invalid resolver result types")
        return document, dependencies
    except (KeyError, TypeError, ValueError) as error:
        raise ValueError("Invalid response from rules resolver") from error


def resolve_document(source, compiler=None):
    """Resolve a detached document without mutating the supplied input."""
    return resolve_with_dependencies(source, compiler)[0]


def load_rules(path, compiler=None):
    """Read a rules source by explicit filesystem path and resolve its import."""
    return resolve_document(Path(path), compiler)
