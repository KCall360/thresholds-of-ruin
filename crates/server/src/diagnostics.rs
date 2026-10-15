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
    incomplete: AtomicBool,
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
    Survival(serde_json::Value),
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

    pub(crate) fn survival_file(path: &std::path::Path) -> io::Result<Self> {
        let writer = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        Self::start(writer, 16).map(|(sink, _)| sink)
    }

    pub(crate) fn survival(&self, record: serde_json::Value) -> bool {
        if record["flows"]["incomplete"] == true {
            self.loss.incomplete.store(true, Ordering::Relaxed);
        }
        // Bounds the optional queue independently of gameplay or history size.
        if serde_json::to_vec(&record).map_or(true, |bytes| bytes.len() > 2 * 1024 * 1024) {
            self.loss.record(1);
            return false;
        }
        self.enqueue(Record::Survival(record))
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
                Record::Survival(record) => {
                    let mut record = record.clone();
                    record["diagnostic_dropped"] = loss.total.load(Ordering::Relaxed).into();
                    record["diagnostic_incomplete"] =
                        loss.incomplete.load(Ordering::Relaxed).into();
                    serde_json::to_writer(&mut writer, &record)?;
                }
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

    fn survival_record(event: &str) -> serde_json::Value {
        serde_json::json!({
            "survival_version": 1,
            "event": event,
            "flows": {"actors": {}, "incomplete": false},
        })
    }

    fn output_rows(output: &Output) -> Vec<serde_json::Value> {
        let bytes = output.0.lock().unwrap();
        let text = std::str::from_utf8(&bytes).unwrap();
        assert!(text.ends_with('\n'));
        text.lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }

    #[test]
    fn survival_queue_pressure_does_not_block_producers_and_marks_accepted_barriers() {
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
        assert!(sink.survival(survival_record("survival.begin")));
        waiting.recv_timeout(Duration::from_secs(5)).unwrap();
        let (finished, result) = mpsc::channel();
        let producer = std::thread::spawn(move || {
            let commit = sink.survival(survival_record("survival.commit"));
            let barrier = sink.survival(survival_record("survival.barrier"));
            let rejected = (0..10)
                .filter(|_| {
                    let mut record = survival_record("survival.commit");
                    // Even a rejected batch must poison later evidence.
                    record["flows"]["incomplete"] = true.into();
                    !sink.survival(record)
                })
                .count();
            // Dropping the final producer must not join the blocked writer.
            drop(sink);
            finished.send((commit, barrier, rejected)).unwrap();
        });
        let outcome = result.recv_timeout(Duration::from_secs(5));
        // Always release the writer, including when a blocking regression fails.
        resume.send(()).unwrap();
        producer.join().unwrap();
        worker.join().unwrap();
        assert_eq!(outcome.unwrap(), (true, true, 10));
        assert_eq!(loss.total.load(Ordering::Relaxed), 10);
        assert!(!loss.failed.load(Ordering::Relaxed));
        let rows = output_rows(&output);
        assert_eq!(rows.len(), 4);
        assert_eq!(rows[0]["event"], "survival.begin");
        assert_eq!(rows[0]["diagnostic_dropped"], 0);
        assert_eq!(rows[1]["event"], "server_diagnostics_dropped");
        assert_eq!(rows[1]["dropped"], 10);
        for row in &rows[2..] {
            assert_eq!(row["diagnostic_dropped"], 10);
            assert_eq!(row["diagnostic_incomplete"], true);
        }
        assert_eq!(rows[3]["event"], "survival.barrier");
    }

    #[test]
    fn oversized_survival_batch_is_rejected_and_taints_the_next_barrier() {
        let output = Output::default();
        let (sink, worker) = Diagnostics::start(output.clone(), 2).unwrap();
        let mut record = survival_record("survival.commit");
        record["padding"] = "é".repeat(1024 * 1024).into();
        record["flows"]["incomplete"] = true.into();
        assert!(!sink.survival(record));
        assert_eq!(sink.loss.total.load(Ordering::Relaxed), 1);
        assert!(sink.survival(survival_record("survival.barrier")));
        drop(sink);
        worker.join().unwrap();
        let rows = output_rows(&output);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["event"], "server_diagnostics_dropped");
        assert_eq!(rows[0]["dropped"], 1);
        assert_eq!(rows[1]["event"], "survival.barrier");
        assert_eq!(rows[1]["diagnostic_dropped"], 1);
        assert_eq!(rows[1]["diagnostic_incomplete"], true);
    }

    #[test]
    fn incomplete_survival_batch_remains_visible_after_a_complete_batch() {
        let output = Output::default();
        let (sink, worker) = Diagnostics::start(output.clone(), 3).unwrap();
        let mut incomplete = survival_record("survival.commit");
        incomplete["flows"]["incomplete"] = true.into();
        assert!(sink.survival(incomplete));
        assert!(sink.survival(survival_record("survival.commit")));
        assert!(sink.survival(survival_record("survival.barrier")));
        drop(sink);
        worker.join().unwrap();
        let rows = output_rows(&output);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0]["flows"]["incomplete"], true);
        assert_eq!(rows[1]["flows"]["incomplete"], false);
        for row in rows {
            assert_eq!(row["diagnostic_dropped"], 0);
            assert_eq!(row["diagnostic_incomplete"], true);
        }
    }

    #[test]
    fn partial_survival_write_counts_inflight_queued_and_future_records() {
        struct PartialFailure {
            blocked: Blocked,
            remaining: usize,
        }
        impl Write for PartialFailure {
            fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
                if self.remaining == 0 {
                    return Err(io::ErrorKind::BrokenPipe.into());
                }
                let written = self
                    .blocked
                    .write(&bytes[..bytes.len().min(self.remaining)])?;
                self.remaining -= written;
                Ok(written)
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let output = Output::default();
        let (entered, waiting) = mpsc::channel();
        let (resume, blocked) = mpsc::channel();
        let (sink, worker) = Diagnostics::start(
            PartialFailure {
                blocked: Blocked {
                    output: output.clone(),
                    entered: Some(entered),
                    resume: blocked,
                },
                remaining: 20,
            },
            2,
        )
        .unwrap();
        assert!(sink.survival(survival_record("survival.begin")));
        waiting.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(sink.survival(survival_record("survival.commit")));
        assert!(sink.survival(survival_record("survival.barrier")));
        resume.send(()).unwrap();
        worker.join().unwrap();
        assert!(sink.loss.failed.load(Ordering::Relaxed));
        assert_eq!(sink.loss.total.load(Ordering::Relaxed), 3);
        assert!(!sink.survival(survival_record("survival.barrier")));
        assert_eq!(sink.loss.total.load(Ordering::Relaxed), 4);
        let bytes = output.0.lock().unwrap();
        assert_eq!(bytes.len(), 20);
        assert!(!bytes.ends_with(b"\n"));
        assert!(serde_json::from_slice::<serde_json::Value>(&bytes).is_err());
    }
}
