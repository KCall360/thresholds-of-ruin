//! Bounded checkpoint storage envelope. Streaming JSON compression keeps large
//! perception histories out of SQLite's page/journal traffic without allocating
//! a second JSON payload. Both encoded and decoded sizes retain the 64 MiB cap.
use flate2::{write::ZlibEncoder, Compression, Decompress, FlushDecompress, Status};
use serde::Serialize;
use std::cell::Cell;
use std::io::{self, Write};

pub(crate) const LIMIT: usize = tor_simulation::checkpoint::ARCHIVE_BYTE_LIMIT;
const HEADER: &[u8; 8] = b"TORC\x01\x00\x00\x00";
const OVERHEAD: usize = 12;

/// Only the encoder's own size bounds establish a permanent capacity failure.
/// Writer, serialization and resource errors remain unclassified/retryable.
#[derive(Debug)]
pub(crate) enum EncodeError {
    CapacityExceeded,
    Other(io::Error),
}
impl EncodeError {
    fn into_io(self) -> io::Error {
        match self {
            Self::CapacityExceeded => invalid(),
            Self::Other(error) => error,
        }
    }
}

#[derive(Clone, Copy)]
enum EncodeFailure {
    Capacity,
    Other,
}
fn retain_first_failure(failure: &Cell<Option<EncodeFailure>>, kind: EncodeFailure) {
    if failure.get().is_none() {
        failure.set(Some(kind));
    }
}

struct Bounded<'a, W> {
    inner: W,
    bytes: usize,
    limit: usize,
    failure: &'a Cell<Option<EncodeFailure>>,
}
impl<W: Write> Write for Bounded<'_, W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > self.limit.saturating_sub(self.bytes) {
            retain_first_failure(self.failure, EncodeFailure::Capacity);
            return Err(invalid());
        }
        let written = self.inner.write(bytes).inspect_err(|error| {
            // write_all retries Interrupted; a recovered interruption must not
            // mask a later real capacity failure.
            if error.kind() != io::ErrorKind::Interrupted {
                retain_first_failure(self.failure, EncodeFailure::Other);
            }
        })?;
        self.bytes += written;
        Ok(written)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush().inspect_err(|_| {
            retain_first_failure(self.failure, EncodeFailure::Other);
        })
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

pub(crate) fn write_json_classified<T: Serialize, W: Write>(
    value: &T,
    output: W,
) -> Result<W, EncodeError> {
    write_encoding_classified(value, output, LIMIT, Compression::fast())
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
    write_encoding_classified(value, output, limit, compression).map_err(EncodeError::into_io)
}

fn write_encoding_classified<T: Serialize, W: Write>(
    value: &T,
    output: W,
    limit: usize,
    compression: Compression,
) -> Result<W, EncodeError> {
    let failure = Cell::new(None);
    let result = (|| {
        let mut output = Bounded {
            inner: output,
            bytes: 0,
            limit,
            failure: &failure,
        };
        output.write_all(HEADER)?;
        let mut encoder = ZlibEncoder::new(output, compression);
        let raw_bytes = write_json_input(value, &mut encoder, limit, &failure)?;
        let mut output = encoder.finish()?;
        output.write_all(&(raw_bytes as u32).to_le_bytes())?;
        Ok(output.inner)
    })();
    result.map_err(|error| {
        if matches!(failure.get(), Some(EncodeFailure::Capacity)) {
            EncodeError::CapacityExceeded
        } else {
            EncodeError::Other(error)
        }
    })
}

fn write_json_input<T: Serialize, W: Write>(
    value: &T,
    output: &mut W,
    limit: usize,
    failure: &Cell<Option<EncodeFailure>>,
) -> io::Result<usize> {
    // Serde emits small punctuation/number tokens. Batch compression input
    // in fixed memory rather than making a compression call for every token.
    let mut json = Bounded {
        inner: io::BufWriter::with_capacity(64 * 1024, output),
        bytes: 0,
        limit,
        failure,
    };
    serde_json::to_writer(&mut json, value).map_err(|error| {
        // Record a serialization error before buffered writers are dropped;
        // a later best-effort drain must not relabel the original failure.
        retain_first_failure(failure, EncodeFailure::Other);
        io::Error::other(error)
    })?;
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
pub(super) fn write_json_with_test_limit<T: Serialize, W: Write>(
    value: &T,
    output: W,
    limit: usize,
) -> io::Result<W> {
    write_limited(value, output, limit)
}

#[cfg(test)]
pub(super) fn write_json_classified_with_test_limit<T: Serialize, W: Write>(
    value: &T,
    output: W,
    limit: usize,
) -> Result<W, EncodeError> {
    write_encoding_classified(value, output, limit, Compression::fast())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classified_capacity_distinguishes_raw_and_encoded_bounds() {
        let value = vec![0_u8; 2_048];
        let raw = serde_json::to_vec(&value).unwrap();
        let encoded = write_json_classified(&value, Vec::new()).unwrap();
        assert_eq!(decode(&encoded).unwrap(), raw);
        assert!(encoded.len() < raw.len() - 1);
        assert!(matches!(
            write_json_classified_with_test_limit(&value, Vec::new(), raw.len() - 1),
            Err(EncodeError::CapacityExceeded)
        ));
        let value = "a";
        let raw = serde_json::to_vec(value).unwrap();
        let encoded = write_json_classified(&value, Vec::new()).unwrap();
        assert!(raw.len() < encoded.len() - 1);
        assert!(matches!(
            write_json_classified_with_test_limit(&value, Vec::new(), encoded.len() - 1),
            Err(EncodeError::CapacityExceeded)
        ));
        assert_eq!(
            write_json_classified_with_test_limit(&value, Vec::new(), encoded.len()).unwrap(),
            encoded
        );
    }

    #[test]
    fn classified_writer_and_serialization_errors_are_not_capacity_failures() {
        struct ResourceFailure;
        impl Write for ResourceFailure {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                Err(io::Error::new(
                    io::ErrorKind::OutOfMemory,
                    "invalid or oversized checkpoint payload",
                ))
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        match write_json_classified(&"valid", ResourceFailure) {
            Err(EncodeError::Other(error)) => assert_eq!(error.kind(), io::ErrorKind::OutOfMemory),
            _ => panic!("writer/resource failure must remain unclassified"),
        }
        struct InvalidValue;
        impl Serialize for InvalidValue {
            fn serialize<S: serde::Serializer>(&self, _: S) -> Result<S::Ok, S::Error> {
                Err(serde::ser::Error::custom(
                    "invalid or oversized checkpoint payload",
                ))
            }
        }
        assert!(matches!(
            write_json_classified(&InvalidValue, Vec::new()),
            Err(EncodeError::Other(_))
        ));
    }

    #[test]
    fn recovered_writer_interruption_does_not_mask_later_capacity() {
        #[derive(Default)]
        struct InterruptedOnce {
            interrupted: bool,
            bytes: Vec<u8>,
        }
        impl Write for InterruptedOnce {
            fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
                if !self.interrupted {
                    self.interrupted = true;
                    return Err(io::Error::from(io::ErrorKind::Interrupted));
                }
                self.bytes.extend_from_slice(bytes);
                Ok(bytes.len())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let value = vec![0_u8; 2_048];
        let raw = serde_json::to_vec(&value).unwrap();
        let control = write_json_classified(&value, InterruptedOnce::default()).unwrap();
        assert!(control.interrupted);
        assert_eq!(decode(&control.bytes).unwrap(), raw);
        assert!(matches!(
            write_json_classified_with_test_limit(
                &value,
                InterruptedOnce::default(),
                raw.len() - 1
            ),
            Err(EncodeError::CapacityExceeded)
        ));
    }

    #[test]
    fn serialization_failure_is_not_relabelled_by_buffered_cleanup_capacity() {
        use serde::ser::SerializeSeq;
        struct InvalidAfterData;
        impl Serialize for InvalidAfterData {
            fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                let mut sequence = serializer.serialize_seq(Some(2))?;
                sequence.serialize_element("abcdefghij")?;
                Err(serde::ser::Error::custom("fixture serialization failure"))
            }
        }
        let control = vec!["abcdefghij"];
        assert!(serde_json::to_vec(&control).unwrap().len() <= 20);
        assert!(matches!(
            write_encoding_classified(&control, Vec::new(), 20, Compression::none()),
            Err(EncodeError::CapacityExceeded)
        ));
        match write_encoding_classified(&InvalidAfterData, Vec::new(), 20, Compression::none()) {
            Err(EncodeError::Other(error)) => {
                assert!(error.to_string().contains("fixture serialization failure"))
            }
            _ => panic!("buffer cleanup must not replace the original serialization failure"),
        }
    }

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
        let count = write_json_input(&value, &mut input, LIMIT, &Cell::new(None)).unwrap();
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
            write_json_input(&value, &mut output, expected.len(), &Cell::new(None)).unwrap(),
            expected.len()
        );
        assert_eq!(output, expected);
        assert!(write_json_input(
            &value,
            &mut Vec::new(),
            expected.len() - 1,
            &Cell::new(None)
        )
        .is_err());
        assert_eq!(
            write_json_input(&value, &mut Failing, LIMIT, &Cell::new(None))
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
