//! Bounded logical snapshot transfer, independent of individual response frames.
//! Transport owners retain assembly across canceled reads; consumers see only a
//! complete snapshot and still perform their ordinary semantic validation.
use crate::{DecodeError, EncodeError, ServerMessage, StreamContext};
use serde::{Deserialize, Serialize};

/// Independent ceiling for a complete recovery snapshot, including its history
/// and authority metadata. The retained StateView keeps its separate 16 MiB cap.
/// This raises no individual frame limit and reserves no space within a state.
pub const MAX_SNAPSHOT_BYTES: usize = 32 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SnapshotPart {
    pub request_id: String,
    pub context: StreamContext,
    /// UTF-8 byte offsets in the complete encoded snapshot response, not frame bytes.
    pub offset: u32,
    pub total_bytes: u32,
    pub data: String,
}

/// Encode one logical recovery atomically. Ordinary snapshots retain their
/// original envelope; oversized ones use ordered parts with exact frame sizing.
pub fn encode_snapshot(
    message: &ServerMessage,
    limit: usize,
    byte_budget: usize,
) -> Result<Vec<String>, EncodeError> {
    let limit = limit.min(crate::MAX_RESPONSE_BYTES);
    let ServerMessage::Snapshot {
        request_id,
        snapshot,
    } = message
    else {
        return crate::encode_bounded_json(message, limit).and_then(|text| {
            if text.len() > byte_budget {
                Err(EncodeError::TransferTooLarge { limit: byte_budget })
            } else {
                Ok(vec![text])
            }
        });
    };
    match crate::encode_bounded_json(message, limit) {
        Ok(text) => {
            return if text.len() > byte_budget {
                Err(EncodeError::TransferTooLarge { limit: byte_budget })
            } else {
                Ok(vec![text])
            };
        }
        Err(EncodeError::TooLarge { .. }) => {}
        Err(error) => return Err(error),
    }
    if crate::encoded_length(snapshot.state.as_ref(), crate::MAX_STATE_BYTES)?.is_none() {
        return Err(EncodeError::RetainedStateTooLarge {
            limit: crate::MAX_STATE_BYTES,
        });
    }
    let text = crate::encode_bounded_json(message, MAX_SNAPSHOT_BYTES)?;
    let mut part = SnapshotPart {
        request_id: request_id.clone(),
        context: snapshot.context.clone(),
        offset: 0,
        total_bytes: text.len() as u32,
        data: String::new(),
    };
    let mut frames = Vec::new();
    let mut encoded_bytes = 0usize;
    while (part.offset as usize) < text.len() {
        let envelope = ServerMessage::SnapshotPart {
            part: Box::new(part.clone()),
        };
        let header =
            crate::encoded_length(&envelope, limit)?.ok_or(EncodeError::TooLarge { limit })?;
        let mut remaining = limit - header;
        let start = part.offset as usize;
        let mut end = start;
        for character in text[start..].chars() {
            // The source is serialized JSON: controls are already escaped.
            // Quotes and backslashes gain one byte inside a JSON string.
            let size = character.len_utf8() + usize::from(matches!(character, '"' | '\\'));
            if size > remaining {
                break;
            }
            remaining -= size;
            end += character.len_utf8();
        }
        if end == start {
            return Err(EncodeError::TooLarge { limit });
        }
        // Count exact escaped frame bytes before allocating the next payload.
        encoded_bytes = encoded_bytes
            .checked_add(limit - remaining)
            .filter(|total| *total <= byte_budget)
            .ok_or(EncodeError::TransferTooLarge { limit: byte_budget })?;
        part.data.push_str(&text[start..end]);
        let envelope = ServerMessage::SnapshotPart {
            part: Box::new(part),
        };
        frames.push(crate::encode_bounded_json(&envelope, limit)?);
        let ServerMessage::SnapshotPart { part: returned } = envelope else {
            unreachable!()
        };
        part = *returned;
        part.data.clear();
        part.offset = end as u32;
    }
    Ok(frames)
}

#[derive(Debug)]
pub enum SnapshotAssemblyError {
    InvalidPart,
    Interrupted,
    Allocation,
    Decode(DecodeError),
}

impl std::fmt::Display for SnapshotAssemblyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidPart => f.write_str("Invalid snapshot transfer identity, order or size"),
            Self::Interrupted => f.write_str("Snapshot transfer interrupted by another response"),
            Self::Allocation => f.write_str("Snapshot assembly allocation failed"),
            Self::Decode(error) => error.fmt(f),
        }
    }
}
impl std::error::Error for SnapshotAssemblyError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Decode(error) => Some(error),
            _ => None,
        }
    }
}

struct PartialSnapshot {
    request_id: String,
    context: StreamContext,
    total_bytes: usize,
    text: String,
}

/// One ordered connection's assembly. Partial contents never escape this owner.
/// A failed transfer clears all partial data; callers must treat it as a failed
/// connection, not silently resume an interrupted stream.
pub struct ResponseAssembler {
    pending: Option<PartialSnapshot>,
    limit: usize,
}

impl Default for ResponseAssembler {
    fn default() -> Self {
        Self {
            pending: None,
            limit: MAX_SNAPSHOT_BYTES,
        }
    }
}

impl ResponseAssembler {
    pub fn with_limit(limit: usize) -> Result<Self, SnapshotAssemblyError> {
        if limit == 0 || limit > MAX_SNAPSHOT_BYTES {
            return Err(SnapshotAssemblyError::InvalidPart);
        }
        Ok(Self {
            pending: None,
            limit,
        })
    }
    pub fn is_pending(&self) -> bool {
        self.pending.is_some()
    }
    pub fn push(
        &mut self,
        message: ServerMessage,
    ) -> Result<Option<ServerMessage>, SnapshotAssemblyError> {
        let result = self.push_inner(message);
        if result.is_err() {
            self.pending = None;
        }
        result
    }

    fn push_inner(
        &mut self,
        message: ServerMessage,
    ) -> Result<Option<ServerMessage>, SnapshotAssemblyError> {
        let ServerMessage::SnapshotPart { part } = message else {
            return if self.pending.is_some() {
                Err(SnapshotAssemblyError::Interrupted)
            } else {
                Ok(Some(message))
            };
        };
        let total = part.total_bytes as usize;
        if total == 0
            || total > self.limit
            || part.data.is_empty()
            || !part.context.is_valid()
            || part.request_id.len() > 128
            || part.request_id.chars().any(char::is_control)
        {
            return Err(SnapshotAssemblyError::InvalidPart);
        }
        if self.pending.is_none() {
            if part.offset != 0 {
                return Err(SnapshotAssemblyError::InvalidPart);
            }
            self.pending = Some(PartialSnapshot {
                request_id: part.request_id.clone(),
                context: part.context.clone(),
                total_bytes: total,
                text: String::new(),
            });
        }
        let pending = self.pending.as_mut().unwrap();
        if pending.request_id != part.request_id
            || pending.context != part.context
            || pending.total_bytes != total
            || pending.text.len() != part.offset as usize
            || part.data.len() > total - pending.text.len()
        {
            return Err(SnapshotAssemblyError::InvalidPart);
        }
        let required = pending.text.len() + part.data.len();
        if required > pending.text.capacity() {
            let capacity = required
                .max(pending.text.capacity().saturating_mul(2))
                .min(total);
            pending
                .text
                .try_reserve_exact(capacity - pending.text.len())
                .map_err(|_| SnapshotAssemblyError::Allocation)?;
        }
        pending.text.push_str(&part.data);
        if pending.text.len() != total {
            return Ok(None);
        }
        let pending = self.pending.take().unwrap();
        let message = crate::codec::decode_snapshot_response(&pending.text)
            .map_err(SnapshotAssemblyError::Decode)?;
        let ServerMessage::Snapshot {
            request_id,
            snapshot,
        } = &message
        else {
            return Err(SnapshotAssemblyError::InvalidPart);
        };
        if snapshot.context != pending.context || request_id != &pending.request_id {
            return Err(SnapshotAssemblyError::InvalidPart);
        }
        Ok(Some(message))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const TEST_BUDGET: usize = MAX_SNAPSHOT_BYTES * 2;

    fn snapshot() -> ServerMessage {
        let samples: serde_json::Value =
            serde_json::from_str(include_str!("../tests/fixtures/wire-v30.json")).unwrap();
        serde_json::from_value(
            samples["server"]
                .as_array()
                .unwrap()
                .iter()
                .find(|value| value["type"] == "snapshot")
                .unwrap()
                .clone(),
        )
        .unwrap()
    }

    #[test]
    fn small_frames_assemble_exactly_and_never_publish_partial_snapshot() {
        let mut message = snapshot();
        let ServerMessage::Snapshot { snapshot, .. } = &mut message else {
            unreachable!()
        };
        std::sync::Arc::make_mut(&mut snapshot.state)
            .observation
            .visible_cells[0]
            .key = "Unicode é and escaped \\\" content".repeat(200);
        let frames = encode_snapshot(&message, 512, TEST_BUDGET).unwrap();
        assert!(frames.len() > 2);
        let mut assembler = ResponseAssembler::default();
        let count = frames.len();
        for (index, frame) in frames.into_iter().enumerate() {
            assert!(frame.len() <= 512);
            let result = assembler
                .push(crate::decode_response(&frame).unwrap())
                .unwrap();
            if index + 1 == count {
                assert_eq!(result, Some(message.clone()));
            } else {
                assert_eq!(result, None);
            }
        }
    }

    #[test]
    fn fitting_snapshot_retains_its_single_response() {
        let message = snapshot();
        let frames = encode_snapshot(&message, crate::MAX_RESPONSE_BYTES, TEST_BUDGET).unwrap();
        assert_eq!(frames.len(), 1);
        assert_eq!(
            ResponseAssembler::default()
                .push(crate::decode_response(&frames[0]).unwrap())
                .unwrap(),
            Some(message)
        );
    }

    #[test]
    fn wrong_offsets_identity_size_and_interleaved_responses_fail_closed() {
        let frames = encode_snapshot(&snapshot(), 512, TEST_BUDGET).unwrap();
        for case in 0..6 {
            let mut assembler = ResponseAssembler::default();
            assert!(assembler
                .push(crate::decode_response(&frames[0]).unwrap())
                .unwrap()
                .is_none());
            let mut next = crate::decode_response(&frames[1]).unwrap();
            let ServerMessage::SnapshotPart { part } = &mut next else {
                unreachable!()
            };
            match case {
                0 => part.offset += 1,
                1 => part.context.epoch += 1,
                2 => part.request_id.push('x'),
                3 => part.total_bytes = MAX_SNAPSHOT_BYTES as u32 + 1,
                4 => part.data.clear(),
                _ => next = snapshot(),
            }
            assert!(assembler.push(next).is_err());
            assert_eq!(assembler.push(snapshot()).unwrap(), Some(snapshot()));
        }
    }

    #[test]
    fn later_part_cannot_start_a_transfer_and_tiny_frame_cannot_bypass_limit() {
        let frames = encode_snapshot(&snapshot(), 512, TEST_BUDGET).unwrap();
        assert!(ResponseAssembler::default()
            .push(crate::decode_response(&frames[1]).unwrap())
            .is_err());
        assert!(encode_snapshot(&snapshot(), 1, TEST_BUDGET).is_err());
    }

    #[test]
    fn a_state_at_its_ceiling_can_recover_with_the_complete_snapshot_envelope() {
        let mut message = snapshot();
        let ServerMessage::Snapshot { snapshot, .. } = &mut message else {
            unreachable!()
        };
        let state = std::sync::Arc::make_mut(&mut snapshot.state);
        state.observation.visible_cells[0].key.clear();
        let overhead = crate::encoded_length(state, crate::MAX_STATE_BYTES)
            .unwrap()
            .unwrap();
        state.observation.visible_cells[0].key = "k".repeat(crate::MAX_STATE_BYTES - overhead);
        assert_eq!(
            crate::encoded_length(state, crate::MAX_STATE_BYTES).unwrap(),
            Some(crate::MAX_STATE_BYTES)
        );
        state.validate().unwrap();
        assert!(crate::encoded_length(&message, crate::MAX_RESPONSE_BYTES)
            .unwrap()
            .is_none());
        let frames = encode_snapshot(&message, crate::MAX_RESPONSE_BYTES, TEST_BUDGET).unwrap();
        assert!(frames.len() > 1);
        let mut assembler = ResponseAssembler::default();
        let mut result = None;
        for frame in frames {
            assert!(frame.len() <= crate::MAX_RESPONSE_BYTES);
            result = assembler
                .push(crate::decode_response(&frame).unwrap())
                .unwrap();
        }
        assert_eq!(result, Some(message));
    }

    #[test]
    fn assembly_honors_lower_limits_and_checks_completed_context() {
        let frames = encode_snapshot(&snapshot(), 512, TEST_BUDGET).unwrap();
        assert!(ResponseAssembler::with_limit(512)
            .unwrap()
            .push(crate::decode_response(&frames[0]).unwrap())
            .is_err());
        let mut assembler = ResponseAssembler::default();
        let mut result = Ok(None);
        for frame in frames {
            let mut message = crate::decode_response(&frame).unwrap();
            let ServerMessage::SnapshotPart { part } = &mut message else {
                unreachable!()
            };
            part.context.epoch += 1;
            result = assembler.push(message);
        }
        assert!(matches!(result, Err(SnapshotAssemblyError::InvalidPart)));
        assert!(!assembler.is_pending());
    }

    #[test]
    fn snapshot_preparation_stops_at_the_encoded_transfer_budget() {
        assert!(matches!(
            encode_snapshot(&snapshot(), 512, 512),
            Err(EncodeError::TransferTooLarge { limit: 512 })
        ));
        let message = snapshot();
        let frames = encode_snapshot(&message, 512, TEST_BUDGET).unwrap();
        let exact: usize = frames.iter().map(String::len).sum();
        assert_eq!(encode_snapshot(&message, 512, exact).unwrap(), frames);
        assert!(matches!(
            encode_snapshot(&message, 512, exact - 1),
            Err(EncodeError::TransferTooLarge { .. })
        ));
    }
}
