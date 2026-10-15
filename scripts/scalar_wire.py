"""Exact scalar decoding for current decimal-string wire/report fields.

Saved simulation JSON and bounded per-HD/resource operands use their own numeric
schemas. Call this only at an explicitly string-encoded scalar boundary.
"""
import re


def u64(value):
    """Require canonical ASCII decimal text in the complete unsigned u64 range."""
    if not isinstance(value, str) or not re.fullmatch(r"0|[1-9][0-9]{0,19}", value):
        raise ValueError("Expected a canonical unsigned decimal string")
    result = int(value)
    if result >= 2**64:
        raise ValueError("Unsigned decimal exceeds u64")
    return result
