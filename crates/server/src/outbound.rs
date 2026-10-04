//! Ordered, single-encoding output with byte leases covering queued and in-flight
//! frames. These are host resource limits, never game state or scheduling inputs.
use serde::Serialize;
use std::io::{self, Write};
use std::sync::Arc;
use tokio::sync::{mpsc, OwnedSemaphorePermit, Semaphore};
use tor_protocol::ServerMessage;

/// Host output limits. The default frame ceiling matches the existing native
/// clients' WebSocket frame limit; aggregate limits also include in-flight writes.
#[derive(Clone, Copy, Debug)]
pub struct OutboundLimits {
    pub frame_bytes: usize,
    pub client_bytes: usize,
    pub total_bytes: usize,
}

impl Default for OutboundLimits {
    fn default() -> Self {
        Self {
            frame_bytes: 16 * 1024 * 1024,
            client_bytes: 64 * 1024 * 1024,
            total_bytes: 256 * 1024 * 1024,
        }
    }
}

impl OutboundLimits {
    /// Whether the limits fit native frame and permit bounds, with room for at
    /// least one maximum frame in each connection and in the shared pool.
    pub fn is_valid(self) -> bool {
        self.frame_bytes > 0
            && self.frame_bytes <= 16 * 1024 * 1024
            && self.frame_bytes <= self.client_bytes
            && self.client_bytes <= self.total_bytes
            && self.total_bytes <= u32::MAX as usize
            && self.total_bytes <= Semaphore::MAX_PERMITS
    }
}

pub(crate) struct Pool {
    limits: OutboundLimits,
    bytes: Arc<Semaphore>,
}

impl Pool {
    #[cfg(test)]
    pub(crate) fn available_bytes(&self) -> usize {
        self.bytes.available_permits()
    }

    pub(crate) fn new(limits: OutboundLimits) -> Self {
        assert!(limits.is_valid());
        Self {
            limits,
            bytes: Arc::new(Semaphore::new(limits.total_bytes)),
        }
    }

    pub(crate) fn channel(&self, slots: usize) -> (Sender, Receiver) {
        let (sender, receiver) = mpsc::channel(slots);
        (
            Sender {
                sender,
                limits: self.limits,
                bytes: Arc::new(Semaphore::new(self.limits.client_bytes)),
                total: self.bytes.clone(),
            },
            Receiver { receiver },
        )
    }
}

#[derive(Clone)]
pub(crate) struct Sender {
    sender: mpsc::Sender<Frame>,
    limits: OutboundLimits,
    bytes: Arc<Semaphore>,
    total: Arc<Semaphore>,
}

/// Kept alive until flush completes, or until the failed socket is dropped.
/// No second full message object is retained alongside the encoded text.
pub(crate) struct Frame {
    pub text: String,
    pub ack_id: Option<String>,
    _client: OwnedSemaphorePermit,
    _total: OwnedSemaphorePermit,
}

impl Sender {
    pub(crate) fn try_send(&self, message: ServerMessage) -> Result<(), ()> {
        let slot = self.sender.try_reserve().map_err(|_| ())?;
        let text = encode(&message, self.limits.frame_bytes).map_err(|_| ())?;
        let bytes = u32::try_from(text.len()).map_err(|_| ())?;
        let client = self
            .bytes
            .clone()
            .try_acquire_many_owned(bytes)
            .map_err(|_| ())?;
        let total = self
            .total
            .clone()
            .try_acquire_many_owned(bytes)
            .map_err(|_| ())?;
        let ack_id = match message {
            ServerMessage::Ack { request_id, .. } => Some(request_id),
            _ => None,
        };
        slot.send(Frame {
            text,
            ack_id,
            _client: client,
            _total: total,
        });
        Ok(())
    }

    pub(crate) fn has_headroom(&self, slots: usize) -> bool {
        self.sender.capacity() >= slots && self.bytes.available_permits() >= self.limits.frame_bytes
    }

    pub(crate) async fn wait_for_headroom(&self, slots: usize) {
        // The global pool is an admission bound, not a reason to stall an
        // unrelated actor. Each client's own backlog determines its stall timer.
        drop(
            self.bytes
                .clone()
                .acquire_many_owned(self.limits.frame_bytes as u32)
                .await,
        );
        drop(self.sender.reserve_many(slots).await);
    }
}

pub(crate) struct Receiver {
    receiver: mpsc::Receiver<Frame>,
}

impl Receiver {
    pub(crate) async fn recv_frame(&mut self) -> Option<Frame> {
        self.receiver.recv().await
    }

    pub(crate) fn try_recv_frame(&mut self) -> Result<Frame, mpsc::error::TryRecvError> {
        self.receiver.try_recv()
    }

    // Semantic tests consume the same encoded queue as the actual transport.
    #[cfg(test)]
    pub(crate) async fn recv(&mut self) -> Option<ServerMessage> {
        self.recv_frame()
            .await
            .map(|frame| serde_json::from_str(&frame.text).expect("encoded server message"))
    }

    #[cfg(test)]
    pub(crate) fn try_recv(&mut self) -> Result<ServerMessage, mpsc::error::TryRecvError> {
        self.try_recv_frame()
            .map(|frame| serde_json::from_str(&frame.text).expect("encoded server message"))
    }
}

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
            .ok_or_else(|| io::Error::other("outbound frame exceeds host byte limit"))?;
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

fn encode(message: &impl Serialize, limit: usize) -> Result<String, serde_json::Error> {
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
    use tor_protocol::ErrorCode;

    fn message(size: usize) -> ServerMessage {
        ServerMessage::Error {
            request_id: None,
            code: ErrorCode::InvalidRequest,
            message: "é".repeat(size),
        }
    }

    fn limits(frame: usize, client: usize, total: usize) -> OutboundLimits {
        OutboundLimits {
            frame_bytes: frame,
            client_bytes: client,
            total_bytes: total,
        }
    }

    #[test]
    fn encoded_bytes_are_charged_through_queue_and_inflight_ownership() {
        for slots in [16, 256, 4096] {
            let first = message(128);
            let expected = serde_json::to_string(&first).unwrap();
            let n = expected.len();
            let pool = Pool::new(limits(n, n * 2, n * 3));
            let (sender, mut receiver) = pool.channel(slots);
            let (other, mut other_receiver) = pool.channel(slots);
            sender.try_send(first).unwrap();
            let frame = receiver.try_recv_frame().unwrap();
            assert_eq!(frame.text, expected);
            sender.try_send(message(128)).unwrap();
            assert!(sender.try_send(message(128)).is_err());
            assert_eq!(sender.bytes.available_permits(), 0);
            assert_eq!(pool.bytes.available_permits(), n);
            other.try_send(message(128)).unwrap();
            assert!(other.try_send(message(128)).is_err());
            assert_eq!(pool.bytes.available_permits(), 0);
            drop(frame);
            assert_eq!(pool.bytes.available_permits(), n);
            drop(receiver);
            assert_eq!(pool.bytes.available_permits(), n * 2);
            drop(other_receiver.try_recv_frame().unwrap());
            assert_eq!(pool.bytes.available_permits(), n * 3);
        }
    }

    #[test]
    fn failed_encoding_admission_and_closed_receivers_release_slots_and_bytes() {
        let n = serde_json::to_string(&message(1)).unwrap().len();
        let pool = Pool::new(limits(n, n * 2, n * 3));
        let (sender, receiver) = pool.channel(1);
        assert!(sender.try_send(message(2)).is_err());
        assert_eq!(sender.sender.capacity(), 1);
        assert_eq!(pool.bytes.available_permits(), n * 3);
        sender.try_send(message(1)).unwrap();
        assert!(sender.try_send(message(1)).is_err());
        assert_eq!(pool.bytes.available_permits(), n * 2);
        drop(receiver);
        assert_eq!(pool.bytes.available_permits(), n * 3);
        assert!(sender.try_send(message(1)).is_err());
        assert_eq!(sender.bytes.available_permits(), n * 2);
    }

    #[test]
    fn bounded_encoding_counts_escaping_and_utf8_and_never_writes_past_limit() {
        for text in ["é\"\\\n", "plain"] {
            let bytes = serde_json::to_vec(text).unwrap();
            assert_eq!(encode(&text, bytes.len()).unwrap().as_bytes(), bytes);
            assert!(encode(&text, bytes.len() - 1).is_err());
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

    #[tokio::test]
    async fn byte_headroom_waits_for_inflight_release_and_closed_queue_wakes() {
        let n = serde_json::to_string(&message(1)).unwrap().len();
        let pool = Pool::new(limits(n, n, n * 2));
        let (sender, mut receiver) = pool.channel(16);
        sender.try_send(message(1)).unwrap();
        let frame = receiver.try_recv_frame().unwrap();
        assert!(!sender.has_headroom(16));
        assert!(tokio::time::timeout(
            std::time::Duration::from_millis(10),
            sender.wait_for_headroom(16)
        )
        .await
        .is_err());
        drop(frame);
        assert!(sender.has_headroom(16));
        drop(receiver);
        tokio::time::timeout(
            std::time::Duration::from_secs(1),
            sender.wait_for_headroom(16),
        )
        .await
        .unwrap();
    }

    #[test]
    fn invalid_host_limits_are_rejected() {
        assert!(OutboundLimits::default().is_valid());
        for limits in [
            limits(0, 1, 2),
            limits(2, 1, 2),
            limits(1, 3, 2),
            limits(1, 1, usize::MAX),
        ] {
            assert!(!limits.is_valid());
        }
    }
}
