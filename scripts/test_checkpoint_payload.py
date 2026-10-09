"""The diagnostic decoder preserves the production envelope's resource bounds."""
import unittest
from unittest import mock
import zlib

from checkpoint_payload import HEADER, LIMIT, decode_checkpoint_payload


def envelope(raw, declared=None):
    return HEADER + zlib.compress(raw, 1) + (len(raw) if declared is None else declared).to_bytes(4, "little")


class CheckpointPayload(unittest.TestCase):
    def test_valid_empty_and_large_streams_round_trip(self):
        for raw in (b"", b'{}', b'"remembered cell"' * 10000):
            self.assertEqual(decode_checkpoint_payload(envelope(raw)), raw)

    def test_older_corrupt_truncated_and_trailing_streams_fail(self):
        valid = envelope(b'{"game":{},"shared":{}}')
        malformed = [b'{"version":24}', valid[:7], valid[:-1]]
        for index in (0, 4, 6, 8, len(valid) - 5):
            damaged = bytearray(valid)
            damaged[index] ^= 0xff
            malformed.append(bytes(damaged))
        malformed.extend(valid[:-4-cut] + valid[-4:] for cut in (1, 4, 8))
        malformed.extend((valid[:-4] + b'x' + valid[-4:], valid[:-4] + valid[8:],
                          envelope(b'{}', 1), envelope(b'{}', 3)))
        for payload in malformed:
            with self.subTest(length=len(payload)), self.assertRaises(ValueError):
                decode_checkpoint_payload(payload)

    def test_oversized_declaration_is_rejected_before_decompression(self):
        with mock.patch('checkpoint_payload.zlib.decompressobj', side_effect=AssertionError('must reject first')):
            with self.assertRaises(ValueError):
                decode_checkpoint_payload(envelope(b'{}', LIMIT + 1))

    def test_compression_bomb_is_limited_by_the_declared_size(self):
        with self.assertRaises(ValueError):
            decode_checkpoint_payload(envelope(b'A' * (1024 * 1024), 1))


if __name__ == '__main__':
    unittest.main()
