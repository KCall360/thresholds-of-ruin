//! Shared wire byte/depth limits, typed decoding and bounded JSON encoding. No filesystem, network or host policy.
use serde::{de::DeserializeOwned, Serialize};
use std::io::{self, Write};

/// Maximum UTF-8 bytes in one complete client message, including its envelope.
pub const MAX_REQUEST_BYTES: usize = 16 * 1024;
/// Maximum UTF-8 bytes in one complete server message, including its envelope.
pub const MAX_RESPONSE_BYTES: usize = 16 * 1024 * 1024;
/// Ceiling for the canonical encoded full StateView retained by a stream,
/// independently of whether an incoming observation is a full view or a delta.
pub const MAX_STATE_BYTES: usize = MAX_RESPONSE_BYTES;

struct Limited {
    bytes: Vec<u8>,
    limit: usize,
    exceeded: bool,
}

impl Write for Limited {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let Some(length) = self
            .bytes
            .len()
            .checked_add(bytes.len())
            .filter(|&length| length <= self.limit)
        else {
            self.exceeded = true;
            return Err(io::Error::other("wire message exceeds byte limit"));
        };
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

/// Encoding distinguishes capacity rejection from serializer failure without
/// inspecting diagnostic strings. Retained-state and envelope limits differ.
#[derive(Debug)]
pub enum EncodeError {
    TooLarge { limit: usize },
    RetainedStateTooLarge { limit: usize },
    Json(serde_json::Error),
}

impl std::fmt::Display for EncodeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooLarge { limit } => {
                write!(formatter, "wire message exceeds byte limit ({limit} bytes)")
            }
            Self::RetainedStateTooLarge { limit } => write!(
                formatter,
                "observation exceeds retained-state byte limit ({limit} bytes)"
            ),
            Self::Json(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for EncodeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Json(error) => Some(error),
            _ => None,
        }
    }
}

impl From<serde_json::Error> for EncodeError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

/// Maximum object/array nesting in a complete wire message. The root container
/// counts as one; delimiters in JSON strings do not contribute to nesting.
pub const MAX_JSON_DEPTH: usize = 64;

/// A resource-policy rejection or a typed JSON decoding failure.
#[derive(Debug)]
pub enum DecodeError {
    TooLarge { limit: usize },
    TooDeep { limit: usize },
    Json(serde_json::Error),
}

impl std::fmt::Display for DecodeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooLarge { limit } => write!(formatter, "wire message exceeds {limit} bytes"),
            Self::TooDeep { limit } => {
                write!(formatter, "wire message exceeds {limit} nesting levels")
            }
            Self::Json(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for DecodeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Json(error) => Some(error),
            _ => None,
        }
    }
}

fn decode<T: DeserializeOwned>(text: &str, limit: usize) -> Result<T, DecodeError> {
    if text.len() > limit {
        return Err(DecodeError::TooLarge { limit });
    }
    // This allocation-free scan enforces resource depth, including ignored fields.
    // Serde remains responsible for syntax, escaping, schema and trailing content.
    let (mut depth, mut string, mut escaped) = (0usize, false, false);
    for byte in text.bytes() {
        if string {
            if escaped {
                escaped = false;
            } else {
                match byte {
                    b'\\' => escaped = true,
                    b'"' => string = false,
                    _ => {}
                }
            }
        } else {
            match byte {
                b'"' => string = true,
                b'{' | b'[' => {
                    depth += 1;
                    if depth > MAX_JSON_DEPTH {
                        return Err(DecodeError::TooDeep {
                            limit: MAX_JSON_DEPTH,
                        });
                    }
                }
                b'}' | b']' => depth = depth.saturating_sub(1),
                _ => {}
            }
        }
    }
    serde_json::from_str(text).map_err(DecodeError::Json)
}

/// Decode a complete client envelope after byte and nesting checks.
pub fn decode_request(text: &str) -> Result<crate::ClientMessage, DecodeError> {
    decode(text, MAX_REQUEST_BYTES)
}

/// Decode a complete server envelope after byte and nesting checks.
pub fn decode_response(text: &str) -> Result<crate::ServerMessage, DecodeError> {
    decode(text, MAX_RESPONSE_BYTES)
}

/// Serialize JSON while bounding allocation and encoded UTF-8 bytes.
/// Escaping counts toward the limit; failure returns no partial message.
pub fn encode_bounded_json(message: &impl Serialize, limit: usize) -> Result<String, EncodeError> {
    let mut writer = Limited {
        bytes: Vec::new(),
        limit,
        exceeded: false,
    };
    let result = serde_json::to_writer(&mut writer, message);
    if writer.exceeded {
        return Err(EncodeError::TooLarge { limit });
    }
    result?;
    Ok(String::from_utf8(writer.bytes).expect("JSON serializer writes UTF-8"))
}

/// The selected observation representation; ordinary replies have no selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObservationEncoding {
    Full,
    Delta,
}

/// One bounded complete server envelope, ready for host output admission.
/// No second encoded candidate buffer is retained.
pub struct EncodedResponse {
    pub text: String,
    pub observation: Option<ObservationEncoding>,
}

struct Count {
    length: usize,
    limit: usize,
    exceeded: bool,
}

impl Write for Count {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let Some(length) = self
            .length
            .checked_add(bytes.len())
            .filter(|n| *n <= self.limit)
        else {
            self.exceeded = true;
            return Err(io::Error::other("wire message exceeds byte limit"));
        };
        self.length = length;
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

// Overflow is an ineligible candidate; other serialization failures propagate.
pub(crate) fn encoded_length(
    message: &impl Serialize,
    limit: usize,
) -> Result<Option<usize>, serde_json::Error> {
    let mut writer = Count {
        length: 0,
        limit,
        exceeded: false,
    };
    match serde_json::to_writer(&mut writer, message) {
        Ok(()) => Ok(Some(writer.length)),
        Err(_) if writer.exceeded => Ok(None),
        Err(error) => Err(error),
    }
}

/// Select using complete encoded bytes, then allocate only the chosen text.
/// `base` must be the host's exact previous disclosure for this attachment and
/// branch. This codec does not grant authority or replace semantic validation.
/// Equal sizes prefer a full observation; an oversized full may still use a
/// fitting delta. The host may lower but never raise the protocol byte ceiling.
/// Non-observation responses use ordinary bounded encoding.
pub fn encode_response(
    message: &crate::ServerMessage,
    base: Option<(crate::ObservationBase, &crate::StateView)>,
    limit: usize,
) -> Result<EncodedResponse, EncodeError> {
    use crate::{ServerMessage, StateDelta, StreamUpdate, UpdateBody};
    let limit = limit.min(MAX_RESPONSE_BYTES);
    let observation = match message {
        ServerMessage::Update { update } => match &update.body {
            UpdateBody::Observation { state, event } => Some((update, state, event)),
            _ => None,
        },
        _ => None,
    };
    let Some((update, state, event)) = observation else {
        return Ok(EncodedResponse {
            text: encode_bounded_json(message, limit)?,
            observation: None,
        });
    };
    let full_length = encoded_length(message, limit)?;
    // A stream must not grow retained state without bound through small deltas.
    // A fitting complete envelope already proves the state fits this ceiling.
    if full_length.is_none() && encoded_length(state.as_ref(), MAX_STATE_BYTES)?.is_none() {
        return Err(EncodeError::RetainedStateTooLarge {
            limit: MAX_STATE_BYTES,
        });
    }

    let delta = base.and_then(|(base, previous)| {
        if previous.revision != base.revision
            || previous.observation.tick != base.cursor.tick
            || previous.observation.actor != state.observation.actor
            || update.actor != state.observation.actor
        {
            return None;
        }
        StateDelta::between(previous, state).map(|state| ServerMessage::Update {
            update: Box::new(StreamUpdate {
                context: update.context.clone(),
                actor: update.actor,
                branch: update.branch.clone(),
                cursor: update.cursor,
                body: UpdateBody::ObservationDelta {
                    base,
                    state: Box::new(state),
                    event: event.clone(),
                },
            }),
        })
    });
    if let Some(delta) = delta {
        let smaller_limit = full_length.map_or(limit, |length| length.saturating_sub(1));
        if encoded_length(&delta, smaller_limit)?.is_some() {
            return Ok(EncodedResponse {
                text: encode_bounded_json(&delta, limit)?,
                observation: Some(ObservationEncoding::Delta),
            });
        }
    }
    Ok(EncodedResponse {
        text: encode_bounded_json(message, limit)?,
        observation: Some(ObservationEncoding::Full),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn encoding_failures_distinguish_limits_from_serializer_errors() {
        struct Rejected;
        impl Serialize for Rejected {
            fn serialize<S: serde::Serializer>(&self, _: S) -> Result<S::Ok, S::Error> {
                Err(serde::ser::Error::custom("wire message exceeds byte limit"))
            }
        }
        assert!(matches!(
            encode_bounded_json(&"é", 3),
            Err(EncodeError::TooLarge { limit: 3 })
        ));
        assert!(
            matches!(encode_bounded_json(&Rejected, 100), Err(EncodeError::Json(error)) if error.is_data())
        );
        assert_eq!(encode_bounded_json(&"é", 4).unwrap(), "\"é\"");
    }

    #[test]
    fn typed_response_encoding_cannot_raise_the_declared_wire_ceiling() {
        let message = crate::ServerMessage::Error {
            scope: crate::ErrorScope::Transport {},
            request_id: None,
            code: crate::ErrorCode::InvalidRequest,
            message: "x".repeat(MAX_RESPONSE_BYTES),
        };
        assert!(
            encode_response(&message, None, MAX_RESPONSE_BYTES * 2).is_err(),
            "a caller's host limit cannot enlarge the protocol message ceiling"
        );
    }

    #[test]
    fn counting_propagates_serialization_errors_and_checks_arithmetic() {
        struct Fails;
        impl Serialize for Fails {
            fn serialize<S: serde::Serializer>(&self, _: S) -> Result<S::Ok, S::Error> {
                Err(serde::ser::Error::custom("intentional serializer failure"))
            }
        }
        let error = encoded_length(&Fails, 1024).unwrap_err();
        assert!(error.to_string().contains("intentional serializer failure"));
        let mut writer = Count {
            length: usize::MAX,
            limit: usize::MAX,
            exceeded: false,
        };
        assert!(writer.write(b"x").is_err());
        assert!(writer.exceeded);
        assert_eq!(writer.length, usize::MAX);
    }

    #[test]
    fn response_decode_rejects_overdeep_ignored_fields() {
        let samples: serde_json::Value =
            serde_json::from_str(include_str!("../tests/fixtures/wire-v27.json")).unwrap();
        let mut snapshot = samples["server"]
            .as_array()
            .unwrap()
            .iter()
            .find(|message| message["type"] == "snapshot")
            .unwrap()
            .clone();
        let mut ignored = serde_json::Value::Null;
        for _ in 0..MAX_JSON_DEPTH + 1 {
            ignored = serde_json::Value::Array(vec![ignored]);
        }
        snapshot["snapshot"]["state"]["observation"]["ignored"] = ignored;
        let text = serde_json::to_string(&snapshot).unwrap();
        assert!(text.len() < MAX_RESPONSE_BYTES);
        assert!(
            matches!(
                decode_response(&text),
                Err(DecodeError::TooDeep {
                    limit: MAX_JSON_DEPTH
                })
            ),
            "ignored fields must obey the declared nesting ceiling"
        );
    }

    #[test]
    fn resource_limits_reject_before_entering_typed_deserialization() {
        struct NeverDeserialize;
        impl<'de> serde::Deserialize<'de> for NeverDeserialize {
            fn deserialize<D: serde::Deserializer<'de>>(_: D) -> Result<Self, D::Error> {
                panic!("resource limits must reject before typed construction");
            }
        }
        assert!(matches!(
            decode::<NeverDeserialize>("é", 1),
            Err(DecodeError::TooLarge { limit: 1 })
        ));
        let nested = format!(
            "{}null{}",
            "[".repeat(MAX_JSON_DEPTH + 1),
            "]".repeat(MAX_JSON_DEPTH + 1)
        );
        assert!(matches!(
            decode::<NeverDeserialize>(&nested, nested.len()),
            Err(DecodeError::TooDeep {
                limit: MAX_JSON_DEPTH
            })
        ));
    }

    #[test]
    fn nesting_scan_counts_containers_and_honors_string_escaping() {
        let nested = format!(
            "{}null{}",
            "[".repeat(MAX_JSON_DEPTH),
            "]".repeat(MAX_JSON_DEPTH)
        );
        assert!(decode::<serde_json::Value>(&nested, nested.len()).is_ok());
        let strings = [
            "{[]}".repeat(100),
            "\\\"[{}]".repeat(100),
            "é\n\\[\"}".to_owned(),
        ];
        for value in strings {
            let json = serde_json::to_string(&value).unwrap();
            assert_eq!(decode::<String>(&json, json.len()).unwrap(), value);
        }
        for malformed in ["{} {}", "[}", "\"unterminated", "\"\\q\"", "]"] {
            assert!(
                matches!(
                    decode::<serde_json::Value>(malformed, malformed.len()),
                    Err(DecodeError::Json(_))
                ),
                "{malformed}"
            );
        }
    }

    #[test]
    fn response_byte_ceiling_accepts_exactly_and_rejects_valid_json_one_byte_over() {
        let make = |message| crate::ServerMessage::Error {
            scope: crate::ErrorScope::Transport {},
            request_id: None,
            code: crate::ErrorCode::InvalidRequest,
            message,
        };
        let overhead = serde_json::to_string(&make(String::new())).unwrap().len();
        let message = make("x".repeat(MAX_RESPONSE_BYTES - overhead));
        let mut text = encode_bounded_json(&message, MAX_RESPONSE_BYTES).unwrap();
        assert_eq!(text.len(), MAX_RESPONSE_BYTES);
        assert_eq!(decode_response(&text).unwrap(), message);
        text.push(' '); // Still valid JSON, including the complete trailing whitespace.
        assert!(matches!(
            decode_response(&text),
            Err(DecodeError::TooLarge {
                limit: MAX_RESPONSE_BYTES
            })
        ));
    }

    #[test]
    fn request_depth_preflight_does_not_relax_strict_schema_or_syntax() {
        let hello = crate::ClientMessage::Hello {
            protocol: crate::PROTOCOL_VERSION,
            token: "test".into(),
            frontend: "test".into(),
        };
        let text = serde_json::to_string(&hello).unwrap();
        for malformed in [
            format!("{text} {{}}"),
            text[..text.len() - 1].to_owned(),
            text.replace(
                "\"token\":\"test\"",
                "\"token\":\"test\",\"token\":\"duplicate\"",
            ),
        ] {
            assert!(matches!(
                decode_request(&malformed),
                Err(DecodeError::Json(_))
            ));
        }
        let mut wire = serde_json::to_value(&hello).unwrap();
        wire["unknown"] = serde_json::Value::Null;
        assert!(matches!(
            decode_request(&serde_json::to_string(&wire).unwrap()),
            Err(DecodeError::Json(_))
        ));
        let mut nested = serde_json::Value::Null;
        for _ in 0..MAX_JSON_DEPTH + 1 {
            nested = serde_json::Value::Array(vec![nested]);
        }
        wire["unknown"] = nested;
        assert!(matches!(
            decode_request(&serde_json::to_string(&wire).unwrap()),
            Err(DecodeError::TooDeep {
                limit: MAX_JSON_DEPTH
            })
        ));
    }

    #[test]
    fn typed_decoders_preserve_all_recorded_message_kinds() {
        let samples: serde_json::Value =
            serde_json::from_str(include_str!("../tests/fixtures/wire-v27.json")).unwrap();
        for sample in samples["client"].as_array().unwrap() {
            let text = serde_json::to_string(sample).unwrap();
            assert_eq!(
                decode_request(&text).unwrap(),
                serde_json::from_str::<crate::ClientMessage>(&text).unwrap()
            );
        }
        for sample in samples["server"].as_array().unwrap() {
            let text = serde_json::to_string(sample).unwrap();
            assert_eq!(
                decode_response(&text).unwrap(),
                serde_json::from_str::<crate::ServerMessage>(&text).unwrap()
            );
        }
    }

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
        assert_eq!(decode_request(&encoded).unwrap(), exact);
        let oversized =
            serde_json::to_string(&make("x".repeat(MAX_REQUEST_BYTES - overhead + 1))).unwrap();
        assert!(matches!(
            decode_request(&oversized),
            Err(DecodeError::TooLarge {
                limit: MAX_REQUEST_BYTES
            })
        ));
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
            exceeded: false,
        };
        writer.write_all(&[1; 16]).unwrap();
        assert!(writer.write_all(&[2; 2]).is_err());
        assert_eq!(writer.bytes, vec![1; 16]);
        assert!(writer.bytes.capacity() <= 17);
    }
}
