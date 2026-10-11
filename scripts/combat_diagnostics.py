"""Collect protocol-validated headless pages into a complete retained window.

This adds cross-page consistency/integrity checks, not a second implementation of
combat or wire semantic validation. Inputs come from the real headless frontend.
Exports retain receipts verbatim; numerical comparison only normalizes the two
payment-owner identities, preserving whether each owner exists. Hashes detect
changes; they do not authenticate the author of a transcript.
"""
import argparse
import copy
import hashlib
import json
import os
import tempfile
from pathlib import Path
import re
import sys

FORMAT = "tor-combat-window-v1"
ROOT = Path(__file__).resolve().parents[1]
PROTOCOL = int(re.search(r"PROTOCOL_VERSION: u32 = (\d+);",
                        (ROOT / "crates/protocol/src/wire.rs").read_text(encoding="utf-8")).group(1))
FIELDS = {"enabled", "tick", "captured", "dropped", "retained", "through", "records"}
METADATA = ("enabled", "tick", "captured", "dropped", "retained")
MAX_LINE = 64 * 1024 * 1024


def _decimal(value):
    if not isinstance(value, str) or not re.fullmatch(r"0|[1-9][0-9]{0,19}", value):
        raise ValueError("Expected a canonical unsigned decimal string")
    result = int(value)
    if result >= 2**64:
        raise ValueError("Unsigned decimal exceeds u64")
    return result


def _encoded(value):
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False,
                      allow_nan=False).encode("utf-8")


def _digest(value):
    return hashlib.sha256(_encoded(value)).hexdigest()


def _numerical(payload):
    normalized = copy.deepcopy(payload)
    for record in normalized["records"]:
        for key in ("intention", "origin_intention"):
            record[key] = None if record[key] is None else "present"
    return normalized


def collect_window(pages):
    """Require all retained records from one unchanged window; never hide gaps."""
    if not isinstance(pages, list) or not 1 <= len(pages) <= 64:
        raise ValueError("Expected between one and 64 validated pages")
    metadata = None
    records = {}
    latest = False
    for page in pages:
        if not isinstance(page, dict) or set(page) != FIELDS:
            raise ValueError("Invalid diagnostic page fields")
        tick, captured, dropped, through = (_decimal(page[k]) for k in ("tick", "captured", "dropped", "through"))
        retained = page["retained"]
        if (type(page["enabled"]) is not bool or type(retained) is not int or not 0 <= retained <= 64
                or captured - dropped != retained or (not page["enabled"] and captured != 0)):
            raise ValueError("Inconsistent capture window")
        current = {key: page[key] for key in METADATA}
        if metadata is None:
            metadata = current
        elif metadata != current:
            raise ValueError("Capture window changed during pagination")
        values = page["records"]
        if not isinstance(values, list) or len(values) > 8:
            raise ValueError("Invalid page size")
        if not values:
            if captured != 0 or through != 0:
                raise ValueError("Missing page records")
        elif not dropped < through <= captured:
            raise ValueError("Page cursor outside retained window")
        latest |= through == captured
        start = through - len(values) + 1
        for offset, record in enumerate(values):
            if not isinstance(record, dict):
                raise ValueError("Invalid record")
            sequence = _decimal(record["sequence"])
            if sequence != start + offset or not dropped < sequence <= captured or _decimal(record["tick"]) > tick:
                raise ValueError("Invalid record sequence or tick")
            for key in ("intention", "origin_intention"):
                if record[key] is not None and _decimal(record[key]) == 0:
                    raise ValueError("Invalid payment owner")
            if sequence in records and records[sequence] != record:
                raise ValueError("Conflicting duplicate record")
            records[sequence] = record
    if not latest or len(records) != metadata["retained"]:
        raise ValueError("Missing retained records: query the latest page and all older pages")
    payload = {"format": FORMAT, "protocol": PROTOCOL, **metadata,
               "records": [copy.deepcopy(records[key]) for key in sorted(records)]}
    return {**payload, "sha256": _digest(payload), "numerical_sha256": _digest(_numerical(payload))}


def verify_export(value):
    """Check integrity and complete window shape; wire validation is upstream."""
    try:
        if (not isinstance(value, dict) or set(value) != {*METADATA, "format", "protocol", "records", "sha256", "numerical_sha256"}
                or value["format"] != FORMAT or type(value["protocol"]) is not int or value["protocol"] != PROTOCOL):
            return False
        records = value["records"]
        if not isinstance(records, list) or len(records) > 64:
            return False
        chunks = [records[i:i+8] for i in range(0, len(records), 8)] or [[]]
        pages = [{**{key: value[key] for key in METADATA}, "records": chunk,
                  "through": chunk[-1]["sequence"] if chunk else "0"} for chunk in chunks]
        return collect_window(pages) == value
    except (ValueError, KeyError, TypeError, OverflowError):
        return False


def numerical_equal(left, right):
    if not verify_export(left) or not verify_export(right):
        raise ValueError("Invalid export integrity or window")
    return left["numerical_sha256"] == right["numerical_sha256"]


def window_from_transcript(lines):
    """Select the last consecutive unchanged private reply window/context."""
    pages = []
    previous = None
    for line in lines:
        if len(line.encode("utf-8")) > MAX_LINE:
            raise ValueError("Headless frame exceeds bounded input size")
        if not line.lstrip().startswith("{"):
            continue
        frame = json.loads(line)
        message = frame.get("message") or {}
        if message.get("type") != "combat_diagnostics":
            continue
        if frame.get("role") != "wizard" or frame.get("synchronized") is not True:
            raise ValueError("Expected a synchronized wizard headless report")
        page = message["report"]
        key = _encoded([message["context"], {name: page[name] for name in METADATA}])
        if key != previous:
            pages = []
            previous = key
        pages.append(page)
        if len(pages) > 64:
            raise ValueError("Too many pages in one window; use a bounded query transcript")
    return collect_window(pages)


def _lines(stream):
    while line := stream.readline(MAX_LINE + 1):
        if len(line) > MAX_LINE:
            raise ValueError("Headless frame exceeds bounded input size")
        yield line


def _write_export(path, value):
    temporary = None
    try:
        with tempfile.NamedTemporaryFile(mode="w", encoding="utf-8", newline="\n", dir=path.parent,
                                         prefix=".combat-export-", delete=False) as stream:
            temporary = Path(stream.name)
            json.dump(value, stream, indent=2, sort_keys=True, ensure_ascii=False, allow_nan=False)
            stream.write("\n")
            stream.flush()
            os.fsync(stream.fileno())
        temporary.replace(path)
    finally:
        if temporary is not None:
            temporary.unlink(missing_ok=True)


def _read_export(path):
    if path.stat().st_size > 128 * 1024 * 1024:
        raise ValueError("Export exceeds bounded input size")
    with path.open(encoding="utf-8") as stream:
        return json.load(stream)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="operation", required=True)
    export = sub.add_parser("export", help="Export the last complete retained window from headless JSONL")
    export.add_argument("transcript", type=Path)
    export.add_argument("output", type=Path)
    compare = sub.add_parser("compare", help="Compare numerical outcomes, preserving raw receipts in exports")
    compare.add_argument("baseline", type=Path)
    compare.add_argument("replay", type=Path)
    args = parser.parse_args(argv)
    try:
        if args.operation == "export":
            if args.output.resolve() == args.transcript.resolve():
                raise ValueError("Export output must differ from its source transcript")
            with args.transcript.open(encoding="utf-8") as stream:
                result = window_from_transcript(_lines(stream))
            _write_export(args.output, result)
            truncated = sum(bool(record["trace"]["truncated"]) for record in result["records"])
            print(f"Exported {len(result['records'])} retained records; dropped {result['dropped']}; truncated traces {truncated}")
            return 0
        left = _read_export(args.baseline)
        right = _read_export(args.replay)
        equal = numerical_equal(left, right)
        print("Numerical outcomes match" if equal else "Numerical outcomes differ")
        return 0 if equal else 1
    except (ValueError, KeyError, TypeError, OSError) as error:
        print(f"Combat diagnostics: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
