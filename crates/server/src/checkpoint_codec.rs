//! Bounded checkpoint storage envelope. Streaming JSON compression keeps large
//! perception histories out of SQLite's page/journal traffic without allocating
//! a second JSON payload. Both encoded and decoded sizes retain the 64 MiB cap.
use flate2::{write::ZlibEncoder, Compression, Decompress, FlushDecompress, Status};
use serde::Serialize;
use std::io::{self, Write};

pub(crate) const LIMIT: usize = 64 * 1024 * 1024;
const HEADER: &[u8; 8] = b"TORC\x01\x00\x00\x00";
const OVERHEAD: usize = 12;

struct Bounded<W> {
    inner: W,
    bytes: usize,
    limit: usize,
}
impl<W: Write> Write for Bounded<W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > self.limit.saturating_sub(self.bytes) {
            return Err(invalid());
        }
        let written = self.inner.write(bytes)?;
        self.bytes += written;
        Ok(written)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}
fn invalid() -> io::Error {
    io::Error::other("invalid or oversized checkpoint payload")
}

/// The counting diagnostic and storage worker use precisely the same encoder.
pub(crate) fn write_json<T: Serialize, W: Write>(value: &T, output: W) -> io::Result<W> {
    write_limited(value, output, LIMIT)
}
fn write_limited<T: Serialize, W: Write>(value: &T, output: W, limit: usize) -> io::Result<W> {
    write_encoding(value, output, limit, Compression::fast())
}

#[cfg(test)]
pub(super) fn write_uncompressed_json<T: Serialize>(value: &T) -> io::Result<Vec<u8>> {
    write_encoding(value, Vec::new(), LIMIT, Compression::none())
}

fn write_encoding<T: Serialize, W: Write>(
    value: &T,
    output: W,
    limit: usize,
    compression: Compression,
) -> io::Result<W> {
    let mut output = Bounded {
        inner: output,
        bytes: 0,
        limit,
    };
    output.write_all(HEADER)?;
    let mut encoder = ZlibEncoder::new(output, compression);
    let raw_bytes = write_json_input(value, &mut encoder, limit)?;
    let mut output = encoder.finish()?;
    output.write_all(&(raw_bytes as u32).to_le_bytes())?;
    Ok(output.inner)
}

fn write_json_input<T: Serialize, W: Write>(
    value: &T,
    output: &mut W,
    limit: usize,
) -> io::Result<usize> {
    // Serde emits small punctuation/number tokens. Batch compression input
    // in fixed memory rather than making a compression call for every token.
    let mut json = Bounded {
        inner: io::BufWriter::with_capacity(64 * 1024, output),
        bytes: 0,
        limit,
    };
    serde_json::to_writer(&mut json, value).map_err(io::Error::other)?;
    let bytes = json.bytes;
    // Flush the input buffer without a zlib sync flush, and propagate failures.
    json.inner
        .into_inner()
        .map_err(|error| error.into_error())?;
    Ok(bytes)
}

pub(crate) fn decode(bytes: &[u8]) -> io::Result<Vec<u8>> {
    if bytes.len() < OVERHEAD || bytes.len() > LIMIT || &bytes[..HEADER.len()] != HEADER {
        return Err(invalid());
    }
    let declared =
        u32::from_le_bytes(bytes[bytes.len() - 4..].try_into().map_err(|_| invalid())?) as usize;
    if declared > LIMIT {
        return Err(invalid());
    }
    let mut input = &bytes[HEADER.len()..bytes.len() - 4];
    let mut decoder = Decompress::new(true);
    let mut decoded = Vec::with_capacity(declared.min(8192));
    loop {
        // One excess byte detects expansion beyond the declaration without
        // permitting the compressed stream to allocate an unbounded payload.
        let remaining = (declared + 1).saturating_sub(decoded.len());
        if remaining == 0 {
            return Err(invalid());
        }
        let mut buffer = [0; 8192];
        let room = remaining.min(buffer.len());
        let before_in = decoder.total_in();
        let before_out = decoder.total_out();
        let status = decoder
            .decompress(input, &mut buffer[..room], FlushDecompress::None)
            .map_err(|_| invalid())?;
        let consumed = (decoder.total_in() - before_in) as usize;
        let produced = (decoder.total_out() - before_out) as usize;
        input = &input[consumed..];
        if produced > declared.saturating_sub(decoded.len()) {
            return Err(invalid());
        }
        let needed = decoded.len() + produced;
        if needed > decoded.capacity() {
            let capacity = decoded
                .capacity()
                .saturating_mul(2)
                .max(needed)
                .min(declared);
            decoded
                .try_reserve_exact(capacity - decoded.len())
                .map_err(|_| invalid())?;
        }
        decoded.extend_from_slice(&buffer[..produced]);
        if status == Status::StreamEnd {
            return if input.is_empty() && decoded.len() == declared {
                Ok(decoded)
            } else {
                Err(invalid())
            };
        }
        if consumed == 0 && produced == 0 {
            return Err(invalid());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiny_json_tokens_do_not_make_a_compression_call_per_token() {
        #[derive(Default)]
        struct CompressionInput {
            writes: usize,
            bytes: Vec<u8>,
        }
        impl Write for CompressionInput {
            fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
                self.writes += 1;
                self.bytes.extend_from_slice(bytes);
                Ok(bytes.len())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let value = vec![[1, 2, 3, 4]; 10_000];
        let mut input = CompressionInput::default();
        let count = write_json_input(&value, &mut input, LIMIT).unwrap();
        assert_eq!(input.bytes, serde_json::to_vec(&value).unwrap());
        assert_eq!(count, input.bytes.len());
        assert!(input.writes <= 4, "{} compression writes", input.writes);
    }

    #[test]
    fn draining_buffered_json_preserves_bounds_and_reports_write_failure() {
        struct Failing;
        impl Write for Failing {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                Err(io::Error::new(io::ErrorKind::BrokenPipe, "fixture failure"))
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let value = vec![[1, 2, 3, 4]; 100];
        let expected = serde_json::to_vec(&value).unwrap();
        let mut output = Vec::new();
        assert_eq!(
            write_json_input(&value, &mut output, expected.len()).unwrap(),
            expected.len()
        );
        assert_eq!(output, expected);
        assert!(write_json_input(&value, &mut Vec::new(), expected.len() - 1).is_err());
        assert_eq!(
            write_json_input(&value, &mut Failing, LIMIT)
                .unwrap_err()
                .kind(),
            io::ErrorKind::BrokenPipe
        );
    }

    #[test]
    fn streaming_storage_and_counting_round_trip_identically() {
        struct Counter(usize);
        impl Write for Counter {
            fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
                self.0 += bytes.len();
                Ok(bytes.len())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let value = serde_json::json!({"navigation": vec![[1,2,3,4]; 10_000], "name": "lit room"});
        let json = serde_json::to_vec(&value).unwrap();
        let encoded = write_json(&value, Vec::new()).unwrap();
        assert_eq!(decode(&encoded).unwrap(), json);
        assert_eq!(write_json(&value, Counter(0)).unwrap().0, encoded.len());
        assert!(encoded.len() * 2 < json.len());
        assert!(write_limited(&value, Vec::new(), 1024).is_err());
        // Output accounting remains correct even for short writes.
        struct Short(Vec<u8>);
        impl Write for Short {
            fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
                let n = bytes.len().min(7);
                self.0.extend_from_slice(&bytes[..n]);
                Ok(n)
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        assert_eq!(write_json(&value, Short(Vec::new())).unwrap().0, encoded);
    }

    #[test]
    fn corrupt_truncated_trailing_and_expanding_payloads_are_rejected() {
        let encoded = write_json(&vec!["remembered cell"; 10_000], Vec::new()).unwrap();
        let mut malformed = Vec::new();
        for index in [0, 4, 6, 8, encoded.len() - 5] {
            let mut b = encoded.clone();
            b[index] ^= 0xff;
            malformed.push(b);
        }
        for declared in [0, 1, (LIMIT + 1) as u32, u32::MAX] {
            let mut b = encoded.clone();
            let n = b.len();
            b[n - 4..].copy_from_slice(&declared.to_le_bytes());
            malformed.push(b);
        }
        for cut in [1, 4, 8, 16] {
            let mut b = encoded.clone();
            let footer = b.split_off(b.len() - 4);
            b.truncate(b.len() - cut);
            b.extend(footer);
            malformed.push(b);
        }
        let mut trailing = encoded.clone();
        trailing.insert(trailing.len() - 4, 0);
        malformed.push(trailing);
        let mut second = encoded[..encoded.len() - 4].to_vec();
        second.extend_from_slice(&encoded[8..]);
        malformed.push(second);
        malformed.push(b"{\"version\":24}".to_vec());
        malformed.push(vec![0; OVERHEAD - 1]);
        for bytes in malformed {
            assert!(decode(&bytes).is_err());
        }
    }
}
