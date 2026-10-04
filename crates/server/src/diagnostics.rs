//! Best-effort host diagnostics. No console write runs on the simulation or
//! socket task; queue pressure and writer failure never become game effects.
use serde::Serialize;
use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

const CAPACITY: usize = 256;
const MAX_DETAIL_BYTES: usize = 16 * 1024;

#[derive(Default)]
struct Loss {
    total: AtomicU64,
    unreported: AtomicU64,
    failed: AtomicBool,
}

impl Loss {
    fn record(&self, count: u64) {
        self.total.fetch_add(count, Ordering::Relaxed);
        self.unreported.fetch_add(count, Ordering::Relaxed);
    }
}

#[derive(Clone)]
pub(crate) struct Diagnostics {
    sender: mpsc::SyncSender<Pending>,
    loss: Arc<Loss>,
}

pub(crate) struct RequestTiming {
    pub started: Instant,
    pub diagnostics: Diagnostics,
}

#[derive(Serialize)]
struct Timing {
    timing_version: u8,
    event: &'static str,
    client: u64,
    request_id: String,
    lock_ms: f64,
    duration_ms: f64,
    unix_ns: u128,
}

enum Record {
    Timing(Timing),
    Warning(String),
}

/// Account for every accepted record even if the receiver disappears while
/// another producer is sending. Counting a drained queue alone misses that race.
struct Pending {
    record: Record,
    loss: Arc<Loss>,
    delivered: bool,
}

impl Drop for Pending {
    fn drop(&mut self) {
        if !self.delivered {
            self.loss.record(1);
        }
    }
}

fn unix_ns() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}

impl Diagnostics {
    pub(crate) fn stderr() -> Option<Self> {
        Self::start(io::stderr(), CAPACITY)
            .ok()
            .map(|(sink, _)| sink)
    }

    fn start(
        writer: impl Write + Send + 'static,
        capacity: usize,
    ) -> io::Result<(Self, std::thread::JoinHandle<()>)> {
        let (sender, receiver) = mpsc::sync_channel(capacity);
        let loss = Arc::new(Loss::default());
        let worker_loss = loss.clone();
        // The worker owns no sender. Dropping the final host handle closes the
        // queue. Its join handle is detached in production: blocked stderr must
        // not delay server shutdown, and diagnostics have no durability promise.
        let worker = std::thread::Builder::new()
            .name("tor-diagnostics".into())
            .spawn(move || drain(writer, receiver, worker_loss))?;
        Ok((Self { sender, loss }, worker))
    }

    fn enqueue(&self, record: Record) -> bool {
        let pending = Pending {
            record,
            loss: self.loss.clone(),
            delivered: false,
        };
        !self.loss.failed.load(Ordering::Relaxed) && self.sender.try_send(pending).is_ok()
    }

    pub(crate) fn timing(
        &self,
        event: &'static str,
        client: u64,
        request_id: &str,
        lock_ms: f64,
        duration_ms: f64,
    ) -> bool {
        if request_id.len() > MAX_DETAIL_BYTES
            || !lock_ms.is_finite()
            || lock_ms < 0.
            || !duration_ms.is_finite()
            || duration_ms < 0.
        {
            self.loss.record(1);
            return false;
        }
        self.enqueue(Record::Timing(Timing {
            timing_version: 1,
            event,
            client,
            request_id: request_id.into(),
            lock_ms,
            duration_ms,
            // Capture at the producer, not after an arbitrary writer delay.
            unix_ns: unix_ns(),
        }))
    }

    pub(crate) fn warning(&self, message: &str) -> bool {
        if message.len() > MAX_DETAIL_BYTES {
            self.loss.record(1);
            return false;
        }
        self.enqueue(Record::Warning(message.into()))
    }
}

fn report_loss(writer: &mut impl Write, loss: &Loss) -> io::Result<()> {
    let dropped = loss.unreported.swap(0, Ordering::Relaxed);
    if dropped > 0 {
        serde_json::to_writer(
            &mut *writer,
            &serde_json::json!({"timing_version": 1, "event": "server_diagnostics_dropped",
                "client": 0, "request_id": "", "lock_ms": 0., "duration_ms": 0.,
                "unix_ns": unix_ns(), "dropped": dropped}),
        )?;
        writer.write_all(b"\n")?;
    }
    Ok(())
}

fn drain(mut writer: impl Write, receiver: mpsc::Receiver<Pending>, loss: Arc<Loss>) {
    while let Ok(mut pending) = receiver.recv() {
        let written = (|| -> io::Result<()> {
            report_loss(&mut writer, &loss)?;
            match &pending.record {
                Record::Timing(record) => serde_json::to_writer(&mut writer, record)?,
                Record::Warning(message) => writer.write_all(message.as_bytes())?,
            }
            writer.write_all(b"\n")?;
            writer.flush()
        })();
        if written.is_err() {
            loss.failed.store(true, Ordering::Relaxed);
            return;
        }
        pending.delivered = true;
    }
    if report_loss(&mut writer, &loss)
        .and_then(|()| writer.flush())
        .is_err()
    {
        loss.failed.store(true, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use std::time::Duration;

    #[test]
    fn lost_queued_and_inflight_records_are_counted_on_drop() {
        let loss = Arc::new(Loss::default());
        let (sender, receiver) = mpsc::sync_channel(2);
        for message in ["queued", "in flight"] {
            assert!(sender
                .try_send(Pending {
                    record: Record::Warning(message.into()),
                    loss: loss.clone(),
                    delivered: false,
                })
                .is_ok());
        }
        let inflight = receiver.try_recv().unwrap();
        drop(receiver);
        assert_eq!(loss.total.load(Ordering::Relaxed), 1);
        drop(inflight);
        assert_eq!(loss.total.load(Ordering::Relaxed), 2);
        assert!(sender
            .try_send(Pending {
                record: Record::Warning("after close".into()),
                loss: loss.clone(),
                delivered: false,
            })
            .is_err());
        assert_eq!(loss.total.load(Ordering::Relaxed), 3);
    }

    #[derive(Clone, Default)]
    struct Output(Arc<Mutex<Vec<u8>>>);
    impl Write for Output {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    struct Blocked {
        output: Output,
        entered: Option<mpsc::Sender<()>>,
        resume: mpsc::Receiver<()>,
    }
    impl Write for Blocked {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if let Some(entered) = self.entered.take() {
                entered.send(()).unwrap();
                self.resume.recv().unwrap();
            }
            self.output.write(bytes)
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn blocked_writer_bounds_queue_counts_loss_and_does_not_hold_final_sender() {
        let output = Output::default();
        let (entered, waiting) = mpsc::channel();
        let (resume, blocked) = mpsc::channel();
        let (sink, worker) = Diagnostics::start(
            Blocked {
                output: output.clone(),
                entered: Some(entered),
                resume: blocked,
            },
            2,
        )
        .unwrap();
        let loss = sink.loss.clone();
        assert!(sink.timing("server_handled", 7, "first", 1., 2.));
        waiting.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(sink.warning("Saving is behind schedule."));
        assert!(sink.timing("server_ack_sent", 7, "second", 0., 3.));
        for _ in 0..10 {
            assert!(!sink.warning("overflow"));
        }
        assert_eq!(loss.total.load(Ordering::Relaxed), 10);
        // Drop does not join the still-blocked writer.
        drop(sink);
        resume.send(()).unwrap();
        worker.join().unwrap();
        let bytes = output.0.lock().unwrap();
        let lines: Vec<_> = std::str::from_utf8(&bytes).unwrap().lines().collect();
        assert_eq!(lines.len(), 4);
        let first: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(first["event"], "server_handled");
        assert_eq!(first["client"], 7);
        assert_eq!(first["request_id"], "first");
        assert_eq!(first["lock_ms"], 1.);
        assert_eq!(first["duration_ms"], 2.);
        assert!(first["unix_ns"].as_u64().unwrap() > 0);
        let dropped: serde_json::Value = serde_json::from_str(lines[1]).unwrap();
        assert_eq!(dropped["event"], "server_diagnostics_dropped");
        assert_eq!(dropped["dropped"], 10);
        assert_eq!(lines[2], "Saving is behind schedule.");
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(lines[3]).unwrap()["request_id"],
            "second"
        );
    }

    #[test]
    fn oversized_and_invalid_details_are_rejected_before_queueing() {
        let output = Output::default();
        let (sink, worker) = Diagnostics::start(output.clone(), 2).unwrap();
        assert!(!sink.warning(&"x".repeat(MAX_DETAIL_BYTES + 1)));
        assert!(!sink.warning(&"é".repeat(MAX_DETAIL_BYTES / 2 + 1)));
        assert!(!sink.timing(
            "server_handled",
            1,
            &"x".repeat(MAX_DETAIL_BYTES + 1),
            0.,
            0.
        ));
        assert!(!sink.timing("server_handled", 1, "r", f64::NAN, 0.));
        assert!(!sink.timing("server_handled", 1, "r", 0., -1.));
        assert_eq!(sink.loss.total.load(Ordering::Relaxed), 5);
        drop(sink);
        worker.join().unwrap();
        let row: serde_json::Value = serde_json::from_slice(&output.0.lock().unwrap()).unwrap();
        assert_eq!(row["dropped"], 5);
    }

    #[test]
    fn failed_writer_stops_and_future_records_are_counted_without_waiting() {
        struct Failed;
        impl Write for Failed {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                Err(io::ErrorKind::BrokenPipe.into())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let (sink, worker) = Diagnostics::start(Failed, 2).unwrap();
        assert!(sink.warning("first"));
        worker.join().unwrap();
        assert!(sink.loss.failed.load(Ordering::Relaxed));
        assert!(!sink.warning("after close"));
        assert_eq!(sink.loss.total.load(Ordering::Relaxed), 2);
    }
}
