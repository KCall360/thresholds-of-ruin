//! Ordered, single-encoding output with byte leases covering queued and in-flight
//! frames. These are host resource limits, never game state or scheduling inputs.
use std::sync::Arc;
use tokio::sync::{mpsc, OwnedSemaphorePermit, Semaphore};
use tor_protocol::{EncodeError, ServerMessage, MAX_RESPONSE_BYTES};

/// Private admission failures. No capacity rejection is a simulation outcome.
#[derive(Debug)]
pub(crate) enum SendError {
    Closed,
    QueueFull,
    FrameLimit { limit: usize },
    Preparation(EncodeError),
    ClientBudget,
    GlobalBudget,
}

impl std::fmt::Display for SendError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Closed => formatter.write_str("output is closed"),
            Self::QueueFull => formatter.write_str("output queue is full"),
            Self::FrameLimit { limit } => write!(formatter, "output frame exceeds {limit} bytes"),
            Self::Preparation(error) => error.fmt(formatter),
            Self::ClientBudget => formatter.write_str("client output byte budget exhausted"),
            Self::GlobalBudget => formatter.write_str("aggregate output byte budget exhausted"),
        }
    }
}

impl std::error::Error for SendError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Preparation(error) => Some(error),
            _ => None,
        }
    }
}

fn preparation_error(error: EncodeError) -> SendError {
    match error {
        EncodeError::TooLarge { limit } => SendError::FrameLimit { limit },
        error => SendError::Preparation(error),
    }
}

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
            frame_bytes: MAX_RESPONSE_BYTES,
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
            && self.frame_bytes <= MAX_RESPONSE_BYTES
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
    #[cfg(test)]
    pub(crate) fn try_send(&self, message: ServerMessage) -> Result<(), SendError> {
        self.try_send_with(&message, tor_protocol::encode_bounded_json)
    }

    /// Reserve capacity before preparation and admit the chosen text once.
    /// The caller retains its message until successful admission so it can
    /// transfer owned disclosed state without making another full copy.
    pub(crate) fn try_send_with(
        &self,
        message: &ServerMessage,
        prepare: impl FnOnce(&ServerMessage, usize) -> Result<String, EncodeError>,
    ) -> Result<(), SendError> {
        let slot = self.sender.try_reserve().map_err(|error| match error {
            mpsc::error::TrySendError::Full(_) => SendError::QueueFull,
            mpsc::error::TrySendError::Closed(_) => SendError::Closed,
        })?;
        let text = prepare(message, self.limits.frame_bytes).map_err(preparation_error)?;
        if text.len() > self.limits.frame_bytes {
            return Err(SendError::FrameLimit {
                limit: self.limits.frame_bytes,
            });
        }
        let bytes = u32::try_from(text.len()).map_err(|_| SendError::FrameLimit {
            limit: self.limits.frame_bytes,
        })?;
        let client =
            self.bytes
                .clone()
                .try_acquire_many_owned(bytes)
                .map_err(|error| match error {
                    tokio::sync::TryAcquireError::Closed => SendError::Closed,
                    tokio::sync::TryAcquireError::NoPermits => SendError::ClientBudget,
                })?;
        let total =
            self.total
                .clone()
                .try_acquire_many_owned(bytes)
                .map_err(|error| match error {
                    tokio::sync::TryAcquireError::Closed => SendError::Closed,
                    tokio::sync::TryAcquireError::NoPermits => SendError::GlobalBudget,
                })?;
        let ack_id = match message {
            ServerMessage::Ack { request_id, .. } => Some(request_id.clone()),
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

#[cfg(test)]
mod tests {
    use super::*;
    use tor_protocol::ErrorCode;

    fn message(size: usize) -> ServerMessage {
        ServerMessage::Error {
            scope: tor_protocol::ErrorScope::Transport {},
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
    fn preparation_does_not_run_without_queue_capacity_and_cannot_bypass_frame_limit() {
        let value = message(1);
        let n = serde_json::to_string(&value).unwrap().len();
        let pool = Pool::new(limits(n, n * 2, n * 4));
        let (sender, receiver) = pool.channel(1);
        assert!(sender
            .try_send_with(&value, |_, _| Ok("x".repeat(n + 1)))
            .is_err());
        assert_eq!(pool.available_bytes(), n * 4);
        sender.try_send(value.clone()).unwrap();
        assert!(sender
            .try_send_with(&value, |_, _| panic!("full queue must not prepare"))
            .is_err());
        drop(receiver);
        assert_eq!(pool.available_bytes(), n * 4);
        assert!(sender
            .try_send_with(&value, |_, _| panic!("closed queue must not prepare"))
            .is_err());
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
    fn typed_admission_failures_preserve_queue_and_byte_capacity() {
        let value = message(1);
        let n = serde_json::to_string(&value).unwrap().len();
        let pool = Pool::new(limits(n, n * 2, n * 3));
        let (sender, receiver) = pool.channel(1);
        assert!(
            matches!(sender.try_send_with(&value, |_, _| Ok("x".repeat(n + 1))),
            Err(SendError::FrameLimit { limit }) if limit == n)
        );
        assert!(
            matches!(sender.try_send_with(&value, |_, _| Err(tor_protocol::EncodeError::RetainedStateTooLarge { limit: n })),
            Err(SendError::Preparation(tor_protocol::EncodeError::RetainedStateTooLarge { limit })) if limit == n)
        );
        assert_eq!(sender.sender.capacity(), 1);
        assert_eq!(pool.available_bytes(), n * 3);
        sender.try_send(value.clone()).unwrap();
        assert!(matches!(
            sender.try_send(value.clone()),
            Err(SendError::QueueFull)
        ));
        drop(receiver);
        assert!(matches!(sender.try_send(value), Err(SendError::Closed)));
        assert_eq!(pool.available_bytes(), n * 3);
    }

    #[test]
    fn client_and_aggregate_pressure_have_distinct_failures_without_leaked_leases() {
        let value = message(1);
        let n = serde_json::to_string(&value).unwrap().len();
        let pool = Pool::new(limits(n, n * 2, n * 3));
        let (sender, receiver) = pool.channel(4);
        let (other, other_receiver) = pool.channel(4);
        sender.try_send(value.clone()).unwrap();
        sender.try_send(value.clone()).unwrap();
        assert!(matches!(
            sender.try_send(value.clone()),
            Err(SendError::ClientBudget)
        ));
        assert_eq!(sender.sender.capacity(), 2);
        other.try_send(value.clone()).unwrap();
        assert!(matches!(
            other.try_send(value),
            Err(SendError::GlobalBudget)
        ));
        assert_eq!(other.sender.capacity(), 3);
        assert_eq!(other.bytes.available_permits(), n);
        assert_eq!(pool.available_bytes(), 0);
        drop(receiver);
        drop(other_receiver);
        assert_eq!(pool.available_bytes(), n * 3);
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
