"""Bounded decoding of the save-owned TORC checkpoint envelope for diagnostics."""
import copy
import zlib

LIMIT = 64 * 1024 * 1024
HEADER = b"TORC\x01\x00\x00\x00"


def crc32c(payload):
    """Independent Castagnoli checksum for diagnostic and corruption fixtures."""
    crc = 0xffffffff
    for byte in payload:
        crc ^= byte
        for _ in range(8):
            crc = (crc >> 1) ^ (0x82f63b78 if crc & 1 else 0)
    return crc ^ 0xffffffff


def decode_checkpoint_payload(payload):
    if not isinstance(payload, bytes) or not 12 <= len(payload) <= LIMIT or payload[:8] != HEADER:
        raise ValueError("invalid checkpoint envelope")
    declared = int.from_bytes(payload[-4:], "little")
    if declared > LIMIT:
        raise ValueError("oversized checkpoint")
    decoder = zlib.decompressobj()
    try:
        decoded = decoder.decompress(payload[8:-4], declared + 1)
    except zlib.error as error:
        raise ValueError("invalid checkpoint stream") from error
    if (len(decoded) != declared or not decoder.eof
            or decoder.unused_data or decoder.unconsumed_tail):
        raise ValueError("truncated, trailing or oversized checkpoint stream")
    return decoded


def authoritative_floor_activation(reference, shared):
    """Reconstruct the strict boundary clock overlay without discarding state."""
    if reference is None:
        return None
    if set(reference) != {"state", "wandering"}:
        raise ValueError("invalid floor activation checkpoint reference")
    base = copy.deepcopy(shared["floor_activations"][reference["state"]])
    if base["wandering"] is not None:
        raise ValueError("pooled floor activation must have canonical null wandering")
    base["wandering"] = copy.deepcopy(reference["wandering"])
    return base
