//! Ordered, single-encoding output with byte leases covering queued and in-flight
//! frames. These are host resource limits, never game state or scheduling inputs.
use std::collections::VecDeque;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};
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
        EncodeError::TransferTooLarge { .. } => SendError::ClientBudget,
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

const MAX_CONNECTIONS: usize = 128;

impl OutboundLimits {
    /// Connections retain one maximum-frame guarantee for their entire output lifetime.
    pub fn connection_limit(self) -> usize {
        if self.is_valid() {
            MAX_CONNECTIONS.min(self.total_bytes / self.frame_bytes)
        } else {
            0
        }
    }

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

/// Reserved guarantees plus borrowing never exceed the host budget. Actual
/// leases are tracked separately: reservations do not allocate frame storage.
#[derive(Default)]
struct Accounting {
    reserved: usize,
    borrowed: usize,
    leased: usize,
}

struct Budget {
    total: usize,
    state: Mutex<Accounting>,
}

struct Allocation {
    budget: Arc<Budget>,
    reserve: usize,
    // All writes are protected by budget.state, including frame destruction.
    used: AtomicUsize,
}

impl Allocation {
    fn lease(self: &Arc<Self>, bytes: usize) -> Result<FrameLease, SendError> {
        let mut state = self
            .budget
            .state
            .lock()
            .expect("output accounting poisoned");
        let used = self.used.load(Ordering::Relaxed);
        let next = used.checked_add(bytes).ok_or(SendError::GlobalBudget)?;
        let additional = next.saturating_sub(self.reserve) - used.saturating_sub(self.reserve);
        if additional > self.budget.total - state.reserved - state.borrowed {
            return Err(SendError::GlobalBudget);
        }
        state.borrowed += additional;
        state.leased += bytes;
        self.used.store(next, Ordering::Relaxed);
        Ok(FrameLease {
            allocation: Arc::clone(self),
            bytes,
        })
    }
}

impl Drop for Allocation {
    fn drop(&mut self) {
        let mut state = self
            .budget
            .state
            .lock()
            .expect("output accounting poisoned");
        assert_eq!(self.used.load(Ordering::Relaxed), 0);
        state.reserved -= self.reserve;
    }
}

struct FrameLease {
    allocation: Arc<Allocation>,
    bytes: usize,
}

impl Drop for FrameLease {
    fn drop(&mut self) {
        let mut state = self
            .allocation
            .budget
            .state
            .lock()
            .expect("output accounting poisoned");
        let used = self.allocation.used.load(Ordering::Relaxed);
        let next = used - self.bytes;
        state.borrowed -= used.saturating_sub(self.allocation.reserve)
            - next.saturating_sub(self.allocation.reserve);
        state.leased -= self.bytes;
        self.allocation.used.store(next, Ordering::Relaxed);
    }
}

pub(crate) struct Pool {
    limits: OutboundLimits,
    budget: Arc<Budget>,
}

impl Pool {
    #[cfg(test)]
    pub(crate) fn available_bytes(&self) -> usize {
        self.budget.total - self.budget.state.lock().unwrap().leased
    }

    pub(crate) fn connection_limit(&self) -> usize {
        self.limits.connection_limit()
    }

    pub(crate) fn capabilities(&self) -> tor_protocol::ServerCapabilities {
        // Pool construction validates the frame ceiling; the host connection cap is 128.
        tor_protocol::ServerCapabilities::new(
            self.limits.frame_bytes as u32,
            self.connection_limit() as u32,
        )
    }

    pub(crate) fn new(limits: OutboundLimits) -> Self {
        assert!(limits.is_valid());
        Self {
            limits,
            budget: Arc::new(Budget {
                total: limits.total_bytes,
                state: Mutex::default(),
            }),
        }
    }

    pub(crate) fn channel(&self, slots: usize) -> Result<(Sender, Receiver), SendError> {
        let reserve = self.limits.frame_bytes;
        {
            let mut state = self
                .budget
                .state
                .lock()
                .expect("output accounting poisoned");
            if state.reserved / reserve >= self.connection_limit()
                || reserve > self.budget.total - state.reserved - state.borrowed
            {
                return Err(SendError::GlobalBudget);
            }
            state.reserved += reserve;
        }
        let allocation = Arc::new(Allocation {
            budget: Arc::clone(&self.budget),
            reserve,
            used: AtomicUsize::new(0),
        });
        let (sender, receiver) = mpsc::channel(slots);
        Ok((
            Sender {
                sender,
                limits: self.limits,
                bytes: Arc::new(Semaphore::new(self.limits.client_bytes)),
                allocation: Arc::clone(&allocation),
            },
            Receiver {
                receiver,
                _allocation: allocation,
            },
        ))
    }
}

#[derive(Clone)]
pub(crate) struct Sender {
    sender: mpsc::Sender<Frame>,
    limits: OutboundLimits,
    bytes: Arc<Semaphore>,
    allocation: Arc<Allocation>,
}

/// Kept alive until flush completes, or until the failed socket is dropped.
/// No second full message object is retained alongside the encoded text.
pub(crate) struct Frame {
    pub text: String,
    // A logical recovery occupies one queue entry. Its following parts cannot
    // interleave with later updates, and all bytes share the admission lease.
    pub following: VecDeque<String>,
    pub ack_id: Option<String>,
    _allocation: FrameLease,
    _client: OwnedSemaphorePermit,
}

struct PreparedFrames {
    text: String,
    following: VecDeque<String>,
}

impl PreparedFrames {
    fn iter(&self) -> impl Iterator<Item = &String> {
        std::iter::once(&self.text).chain(self.following.iter())
    }
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
        self.try_send_batch_with(message, |message, limit| {
            prepare(message, limit).map(|text| PreparedFrames {
                text,
                following: VecDeque::new(),
            })
        })
    }

    pub(crate) fn try_send_snapshot(&self, message: &ServerMessage) -> Result<(), SendError> {
        self.try_send_batch_with(message, |message, limit| {
            let mut following: VecDeque<_> =
                tor_protocol::encode_snapshot(message, limit, self.bytes.available_permits())?
                    .into();
            Ok(PreparedFrames {
                text: following
                    .pop_front()
                    .expect("encoder returns a complete response"),
                following,
            })
        })
    }

    fn try_send_batch_with(
        &self,
        message: &ServerMessage,
        prepare: impl FnOnce(&ServerMessage, usize) -> Result<PreparedFrames, EncodeError>,
    ) -> Result<(), SendError> {
        let slot = self.sender.try_reserve().map_err(|error| match error {
            mpsc::error::TrySendError::Full(_) => SendError::QueueFull,
            mpsc::error::TrySendError::Closed(_) => SendError::Closed,
        })?;
        let texts = prepare(message, self.limits.frame_bytes).map_err(preparation_error)?;
        if texts
            .iter()
            .any(|text| text.len() > self.limits.frame_bytes)
        {
            return Err(SendError::FrameLimit {
                limit: self.limits.frame_bytes,
            });
        }
        let bytes = texts
            .iter()
            .try_fold(0u32, |total, text| {
                total.checked_add(u32::try_from(text.len()).ok()?)
            })
            .ok_or(SendError::ClientBudget)?;
        let client =
            self.bytes
                .clone()
                .try_acquire_many_owned(bytes)
                .map_err(|error| match error {
                    tokio::sync::TryAcquireError::Closed => SendError::Closed,
                    tokio::sync::TryAcquireError::NoPermits => SendError::ClientBudget,
                })?;
        let allocation = self.allocation.lease(bytes as usize)?;
        let ack_id = match message {
            ServerMessage::Ack { request_id, .. } => Some(request_id.clone()),
            _ => None,
        };
        slot.send(Frame {
            text: texts.text,
            following: texts.following,
            ack_id,
            _allocation: allocation,
            _client: client,
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
    _allocation: Arc<Allocation>,
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
        self.recv_frame().await.map(decode_logical_frame)
    }

    #[cfg(test)]
    pub(crate) fn try_recv(&mut self) -> Result<ServerMessage, mpsc::error::TryRecvError> {
        self.try_recv_frame().map(decode_logical_frame)
    }
}

#[cfg(test)]
fn decode_logical_frame(frame: Frame) -> ServerMessage {
    let mut assembler = tor_protocol::ResponseAssembler::default();
    let mut result = None;
    for text in std::iter::once(&frame.text).chain(frame.following.iter()) {
        let message = tor_protocol::decode_response(text).expect("encoded server part");
        let complete = assembler.push(message).expect("encoded logical response");
        if complete.is_some() {
            assert!(result.is_none(), "one logical response per queue entry");
            result = complete;
        }
    }
    result.expect("complete admitted response")
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
        let (sender, receiver) = pool.channel(1).unwrap();
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
    fn snapshot_batch_admission_is_atomic_and_leases_every_part() {
        let samples: serde_json::Value =
            serde_json::from_str(include_str!("../../protocol/tests/fixtures/wire-v30.json"))
                .unwrap();
        let message: ServerMessage = serde_json::from_value(
            samples["server"]
                .as_array()
                .unwrap()
                .iter()
                .find(|value| value["type"] == "snapshot")
                .unwrap()
                .clone(),
        )
        .unwrap();
        let parts =
            tor_protocol::encode_snapshot(&message, 512, tor_protocol::MAX_SNAPSHOT_BYTES * 2)
                .unwrap();
        assert!(parts.len() > 1);
        let bytes: usize = parts.iter().map(String::len).sum();
        for client_bytes in [bytes - 1, bytes] {
            let pool = Pool::new(limits(512, client_bytes, bytes * 2));
            let (sender, mut receiver) = pool.channel(1).unwrap();
            if client_bytes < bytes {
                assert!(matches!(
                    sender.try_send_snapshot(&message),
                    Err(SendError::ClientBudget)
                ));
                assert_eq!(sender.sender.capacity(), 1);
                assert!(receiver.try_recv_frame().is_err());
                assert_eq!(pool.available_bytes(), bytes * 2);
            } else {
                sender.try_send_snapshot(&message).unwrap();
                assert_eq!(
                    sender.sender.capacity(),
                    0,
                    "one logical output occupies one slot"
                );
                assert_eq!(pool.available_bytes(), bytes);
                let frame = receiver.try_recv_frame().unwrap();
                assert_eq!(frame.following.len() + 1, parts.len());
                assert!(frame.following.iter().all(|text| text.len() <= 512));
                assert_eq!(
                    pool.available_bytes(),
                    bytes,
                    "in-flight parts remain leased"
                );
                drop(frame);
                assert_eq!(pool.available_bytes(), bytes * 2);
            }
        }
    }

    #[test]
    fn encoded_bytes_are_charged_through_queue_and_inflight_ownership() {
        for slots in [16, 256, 4096] {
            let first = message(128);
            let expected = serde_json::to_string(&first).unwrap();
            let n = expected.len();
            let pool = Pool::new(limits(n, n * 2, n * 3));
            let (sender, mut receiver) = pool.channel(slots).unwrap();
            let (other, mut other_receiver) = pool.channel(slots).unwrap();
            sender.try_send(first).unwrap();
            let frame = receiver.try_recv_frame().unwrap();
            assert_eq!(frame.text, expected);
            sender.try_send(message(128)).unwrap();
            assert!(sender.try_send(message(128)).is_err());
            assert_eq!(sender.bytes.available_permits(), 0);
            assert_eq!(pool.available_bytes(), n);
            other.try_send(message(128)).unwrap();
            assert!(other.try_send(message(128)).is_err());
            assert_eq!(pool.available_bytes(), 0);
            drop(frame);
            assert_eq!(pool.available_bytes(), n);
            drop(receiver);
            assert_eq!(pool.available_bytes(), n * 2);
            drop(other_receiver.try_recv_frame().unwrap());
            assert_eq!(pool.available_bytes(), n * 3);
        }
    }

    #[test]
    fn failed_encoding_admission_and_closed_receivers_release_slots_and_bytes() {
        let n = serde_json::to_string(&message(1)).unwrap().len();
        let pool = Pool::new(limits(n, n * 2, n * 3));
        let (sender, receiver) = pool.channel(1).unwrap();
        assert!(sender.try_send(message(2)).is_err());
        assert_eq!(sender.sender.capacity(), 1);
        assert_eq!(pool.available_bytes(), n * 3);
        sender.try_send(message(1)).unwrap();
        assert!(sender.try_send(message(1)).is_err());
        assert_eq!(pool.available_bytes(), n * 2);
        drop(receiver);
        assert_eq!(pool.available_bytes(), n * 3);
        assert!(sender.try_send(message(1)).is_err());
        assert_eq!(sender.bytes.available_permits(), n * 2);
    }

    #[tokio::test]
    async fn byte_headroom_waits_for_inflight_release_and_closed_queue_wakes() {
        let n = serde_json::to_string(&message(1)).unwrap().len();
        let pool = Pool::new(limits(n, n, n * 2));
        let (sender, mut receiver) = pool.channel(16).unwrap();
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
        let (sender, receiver) = pool.channel(1).unwrap();
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
        let (sender, receiver) = pool.channel(4).unwrap();
        let (other, other_receiver) = pool.channel(4).unwrap();
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
    fn borrower_cannot_consume_another_connections_maximum_frame_guarantee() {
        let value = message(1);
        let n = serde_json::to_string(&value).unwrap().len();
        let pool = Pool::new(limits(n, n * 4, n * 4));
        let (borrower, _borrowed_output) = pool.channel(8).unwrap();
        let (healthy, mut healthy_output) = pool.channel(8).unwrap();
        for _ in 0..3 {
            borrower.try_send(value.clone()).unwrap();
        }
        assert!(matches!(
            borrower.try_send(value.clone()),
            Err(SendError::GlobalBudget)
        ));
        healthy.try_send(value.clone()).unwrap();
        assert_eq!(
            healthy_output.try_recv_frame().unwrap().text,
            serde_json::to_string(&value).unwrap()
        );
    }

    #[test]
    fn empty_connections_reserve_capacity_without_leasing_bytes() {
        let pool = Pool::new(limits(100, 100, 200));
        let first = pool.channel(1).unwrap();
        let second = pool.channel(1).unwrap();
        assert_eq!(pool.available_bytes(), 200);
        assert_eq!(pool.budget.state.lock().unwrap().reserved, 200);
        assert!(matches!(pool.channel(1), Err(SendError::GlobalBudget)));
        drop(first);
        assert!(pool.channel(1).is_ok());
        drop(second);
        assert_eq!(pool.budget.state.lock().unwrap().reserved, 0);
    }

    #[test]
    fn reservation_outlives_channel_until_inflight_frame_is_released() {
        let value = message(1);
        let n = serde_json::to_string(&value).unwrap().len();
        let pool = Pool::new(limits(n, n, n));
        let (sender, mut receiver) = pool.channel(1).unwrap();
        sender.try_send(value).unwrap();
        let frame = receiver.try_recv_frame().unwrap();
        drop(sender);
        drop(receiver);
        assert!(matches!(pool.channel(1), Err(SendError::GlobalBudget)));
        drop(frame);
        assert_eq!(pool.available_bytes(), n);
        assert!(pool.channel(1).is_ok());
    }

    #[test]
    fn dropping_queued_frames_recalculates_borrowing_around_inflight_frame() {
        let value = message(1);
        let n = serde_json::to_string(&value).unwrap().len();
        let pool = Pool::new(limits(n, n * 3, n * 3));
        let (sender, mut receiver) = pool.channel(3).unwrap();
        for _ in 0..3 {
            sender.try_send(value.clone()).unwrap();
        }
        let frame = receiver.try_recv_frame().unwrap();
        assert!(matches!(pool.channel(1), Err(SendError::GlobalBudget)));
        drop(receiver);
        assert_eq!(pool.budget.state.lock().unwrap().borrowed, 0);
        assert_eq!(pool.available_bytes(), n * 2);
        let replacement = pool.channel(1).unwrap();
        drop(sender);
        assert_eq!(pool.budget.state.lock().unwrap().reserved, n * 2);
        drop(frame);
        assert_eq!(pool.budget.state.lock().unwrap().reserved, n);
        drop(replacement);
        assert_eq!(pool.budget.state.lock().unwrap().reserved, 0);
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
