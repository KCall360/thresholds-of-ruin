//! Shared wire byte limits and bounded JSON encoding. No filesystem, network or host policy.
use serde::Serialize;
use std::io::{self, Write};

/// Maximum UTF-8 bytes in one complete client message, including its envelope.
pub const MAX_REQUEST_BYTES: usize = 16 * 1024;
/// Maximum UTF-8 bytes in one complete server message, including its envelope.
pub const MAX_RESPONSE_BYTES: usize = 16 * 1024 * 1024;

struct Limited {
    bytes: Vec<u8>,
    limit: usize,
}

impl Write for Limited {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let length = self
            .bytes
            .len()
            .checked_add(bytes.len())
            .filter(|&length| length <= self.limit)
            .ok_or_else(|| io::Error::other("wire message exceeds byte limit"))?;
        if length > self.bytes.capacity() {
            let capacity = length
                .max(self.bytes.capacity().saturating_mul(2))
                .min(self.limit);
            self.bytes
                .try_reserve_exact(capacity - self.bytes.len())
                .map_err(io::Error::other)?;
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Serialize JSON while bounding allocation and encoded UTF-8 bytes.
/// Escaping counts toward the limit; failure returns no partial message.
pub fn encode_bounded_json(
    message: &impl Serialize,
    limit: usize,
) -> Result<String, serde_json::Error> {
    let mut writer = Limited {
        bytes: Vec::new(),
        limit,
    };
    serde_json::to_writer(&mut writer, message)?;
    Ok(String::from_utf8(writer.bytes).expect("JSON serializer writes UTF-8"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn client_envelope_fits_exactly_at_the_byte_limit_and_rejects_one_byte_over() {
        use crate::{ClientMessage, PROTOCOL_VERSION};
        let make = |token| ClientMessage::Hello {
            protocol: PROTOCOL_VERSION,
            token,
            frontend: "test".into(),
        };
        let overhead = serde_json::to_string(&make(String::new())).unwrap().len();
        let exact = make("x".repeat(MAX_REQUEST_BYTES - overhead));
        let encoded = encode_bounded_json(&exact, MAX_REQUEST_BYTES).unwrap();
        assert_eq!(encoded.len(), MAX_REQUEST_BYTES);
        assert_eq!(
            serde_json::from_str::<ClientMessage>(&encoded).unwrap(),
            exact
        );
        assert!(encode_bounded_json(
            &make("x".repeat(MAX_REQUEST_BYTES - overhead + 1)),
            MAX_REQUEST_BYTES
        )
        .is_err());
    }

    #[test]
    fn bounded_encoding_counts_escaping_and_utf8_and_never_writes_past_limit() {
        for text in ["é\"\\\n", "plain"] {
            let bytes = serde_json::to_vec(text).unwrap();
            assert_eq!(
                encode_bounded_json(&text, bytes.len()).unwrap().as_bytes(),
                bytes
            );
            assert!(encode_bounded_json(&text, bytes.len() - 1).is_err());
        }
        let mut writer = Limited {
            bytes: Vec::new(),
            limit: 17,
        };
        writer.write_all(&[1; 16]).unwrap();
        assert!(writer.write_all(&[2; 2]).is_err());
        assert_eq!(writer.bytes, vec![1; 16]);
        assert!(writer.bytes.capacity() <= 17);
    }
}
