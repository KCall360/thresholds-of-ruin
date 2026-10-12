//! Opt-in actual-process test diagnostics. Absent these environment variables,
//! startup emits no files and the save worker never waits on a test barrier.
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Best-effort startup breadcrumbs, separate from the public stdout protocol.
#[doc(hidden)]
pub fn startup(stage: &str) {
    let Some(path) = std::env::var_os("TOR_TEST_STARTUP_LOG") else {
        return;
    };
    static STARTED: OnceLock<Instant> = OnceLock::new();
    let elapsed_ms = STARTED.get_or_init(Instant::now).elapsed().as_secs_f64() * 1000.;
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let record = serde_json::json!({
            "stage": stage,
            "elapsed_ms": elapsed_ms,
            "pid": std::process::id(),
            "unix_ms": SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis(),
        });
        let _ = writeln!(file, "{record}");
    }
}

fn marker(base: &Path, suffix: &str) -> PathBuf {
    let mut path = base.as_os_str().to_os_string();
    path.push(suffix);
    path.into()
}

/// Claim each explicitly armed barrier once. The marker is published after the
/// writer releases its state lock, before it writes the batch. A missing release
/// fails that batch after a bounded watchdog instead of hanging shutdown forever.
pub(crate) fn save_barrier(base: Option<&Path>, watchdog: Duration) -> std::io::Result<()> {
    let Some(base) = base else { return Ok(()) };
    match std::fs::rename(marker(base, ".armed"), marker(base, ".entered")) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    }
    let start = Instant::now();
    let outcome = loop {
        match std::fs::remove_file(marker(base, ".release")) {
            Ok(()) => break Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => break Err(error),
        }
        if start.elapsed() >= watchdog {
            break Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "test save barrier was not released",
            ));
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    // Retain entry evidence, but distinguish a currently held writer from one
    // that escaped through the watchdog or an I/O error.
    let status = match &outcome {
        Ok(()) => "released",
        Err(error) if error.kind() == std::io::ErrorKind::TimedOut => "timed_out",
        Err(_) => "error",
    };
    if let Err(error) = &outcome {
        // Best effort: diagnostic I/O must not replace the original failure.
        let detail = serde_json::json!({
            "operation": if error.kind() == std::io::ErrorKind::TimedOut {
                "wait_for_release"
            } else {
                "remove_release_marker"
            },
            "kind": format!("{:?}", error.kind()),
            "raw_os_error": error.raw_os_error(),
            "message": error.to_string(),
        });
        let _ = std::fs::write(marker(base, ".error.json"), detail.to_string());
    }
    std::fs::write(marker(base, ".finished"), status)?;
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unarmed_or_disabled_barrier_never_waits() {
        let dir = tempfile::tempdir().unwrap();
        save_barrier(None, Duration::ZERO).unwrap();
        save_barrier(Some(&dir.path().join("barrier")), Duration::ZERO).unwrap();
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    #[test]
    fn armed_barriers_publish_entry_consume_release_and_are_one_shot() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().join("barrier");
        std::fs::write(marker(&base, ".armed"), "").unwrap();
        std::fs::write(marker(&base, ".release"), "").unwrap();
        save_barrier(Some(&base), Duration::ZERO).unwrap();
        assert!(marker(&base, ".entered").exists());
        assert!(!marker(&base, ".armed").exists());
        assert!(!marker(&base, ".release").exists());
        assert_eq!(
            std::fs::read_to_string(marker(&base, ".finished")).unwrap(),
            "released"
        );
        save_barrier(Some(&base), Duration::ZERO).unwrap();
        std::fs::remove_file(marker(&base, ".entered")).unwrap();
        std::fs::remove_file(marker(&base, ".finished")).unwrap();
        std::fs::write(marker(&base, ".armed"), "").unwrap();
        assert_eq!(
            save_barrier(Some(&base), Duration::ZERO)
                .unwrap_err()
                .kind(),
            std::io::ErrorKind::TimedOut
        );
        assert_eq!(
            std::fs::read_to_string(marker(&base, ".finished")).unwrap(),
            "timed_out"
        );
    }

    #[test]
    fn barrier_io_failure_preserves_original_error_details() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().join("barrier");
        std::fs::write(marker(&base, ".armed"), "").unwrap();
        // Removing a directory as a file fails on both Windows and Unix.
        std::fs::create_dir(marker(&base, ".release")).unwrap();
        let error = save_barrier(Some(&base), Duration::ZERO).unwrap_err();
        assert_ne!(error.kind(), std::io::ErrorKind::TimedOut);
        assert_eq!(
            std::fs::read_to_string(marker(&base, ".finished")).unwrap(),
            "error"
        );
        let detail: serde_json::Value =
            serde_json::from_slice(&std::fs::read(marker(&base, ".error.json")).unwrap()).unwrap();
        assert_eq!(detail["operation"], "remove_release_marker");
        assert_eq!(detail["kind"], format!("{:?}", error.kind()));
        assert_eq!(detail["raw_os_error"], error.raw_os_error().unwrap());
        assert_eq!(detail["message"], error.to_string());
    }

    #[test]
    fn failed_error_diagnostic_write_does_not_replace_original_failure() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().join("barrier");
        std::fs::write(marker(&base, ".armed"), "").unwrap();
        std::fs::create_dir(marker(&base, ".error.json")).unwrap();
        let error = save_barrier(Some(&base), Duration::ZERO).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
        assert_eq!(error.to_string(), "test save barrier was not released");
        assert_eq!(
            std::fs::read_to_string(marker(&base, ".finished")).unwrap(),
            "timed_out"
        );
    }
}
