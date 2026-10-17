//! Publication of the last genuinely presented diagnostic framebuffer.
use std::{
    io::{self, Write},
    path::Path,
};
use tor_client_ascii::render::{Canvas, HEIGHT, WIDTH};

pub(crate) fn save_frame(path: &Path, canvas: &Canvas) -> io::Result<()> {
    publish_frame(path, |output| {
        write!(output, "P6\n{WIDTH} {HEIGHT}\n255\n")?;
        for pixel in &canvas.pixels {
            output.write_all(&[(pixel >> 16) as u8, (pixel >> 8) as u8, *pixel as u8])?;
        }
        Ok(())
    })
}

fn publish_frame(
    path: &Path,
    write_frame: impl FnOnce(&mut dyn Write) -> io::Result<()>,
) -> io::Result<()> {
    let directory = path
        .parent()
        .filter(|directory| !directory.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    // Keep the previously published frame intact until every byte of its
    // replacement is available. A later presentation may race an earlier
    // report's reader; that reader must see one complete frame, never a prefix.
    let mut staged = tempfile::NamedTempFile::new_in(directory)?;
    {
        let mut output = io::BufWriter::new(staged.as_file_mut());
        write_frame(&mut output)?;
        output.flush()?;
    }
    // std's Windows replacement supports an existing delete-sharing reader;
    // diagnostic readers must opt into that sharing mode. Do not remove the
    // destination first or turn a failed replacement into a successful report.
    let staged_path = staged.into_temp_path();
    std::fs::rename(&staged_path, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    const FIRST: &[u8] = b"P6\n1 1\n255\n\x12\x34\x56";
    const SECOND: &[u8] = b"P6\n1 1\n255\n\x78\x9a\xbc";

    fn entries(directory: &Path) -> Vec<std::ffi::OsString> {
        let mut names: Vec<_> = fs::read_dir(directory)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn a_capture_keeps_the_previous_frame_visible_until_replacement_is_complete() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("capture.ppm");
        fs::write(&path, FIRST).unwrap();

        publish_frame(&path, |output| {
            assert_eq!(fs::read(&path).unwrap(), FIRST);
            output.write_all(&SECOND[..7])?;
            output.flush()?;
            assert_eq!(fs::read(&path).unwrap(), FIRST);
            output.write_all(&SECOND[7..])?;
            Ok(())
        })
        .unwrap();

        assert_eq!(fs::read(&path).unwrap(), SECOND);
        assert_eq!(entries(directory.path()), ["capture.ppm"]);
    }

    #[test]
    fn a_failed_frame_preserves_the_previous_capture_and_cleans_staging() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("capture.ppm");
        fs::write(&path, FIRST).unwrap();

        let error = publish_frame(&path, |output| {
            output.write_all(&SECOND[..7])?;
            output.flush()?;
            Err(io::Error::other("injected capture write failure"))
        })
        .unwrap_err();

        assert_eq!(error.to_string(), "injected capture write failure");
        assert_eq!(fs::read(&path).unwrap(), FIRST);
        assert_eq!(entries(directory.path()), ["capture.ppm"]);
    }

    #[test]
    fn the_first_capture_is_absent_until_complete_and_failed_writes_leave_no_file() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("capture.ppm");

        let error = publish_frame(&path, |output| {
            output.write_all(&FIRST[..7])?;
            output.flush()?;
            assert!(!path.exists(), "partial first capture became visible");
            Err(io::Error::other("injected first capture failure"))
        })
        .unwrap_err();

        assert_eq!(error.to_string(), "injected first capture failure");
        assert!(entries(directory.path()).is_empty());
        publish_frame(&path, |output| output.write_all(FIRST)).unwrap();
        assert_eq!(fs::read(&path).unwrap(), FIRST);
        assert_eq!(entries(directory.path()), ["capture.ppm"]);
    }

    #[test]
    fn replacement_failure_preserves_the_destination_and_cleans_staging() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("capture.ppm");
        fs::create_dir(&path).unwrap();
        fs::write(path.join("keep"), FIRST).unwrap();
        let mut encoded = false;

        let result = publish_frame(&path, |output| {
            output.write_all(SECOND)?;
            encoded = true;
            Ok(())
        });

        assert!(result.is_err());
        assert!(
            encoded,
            "the failure must occur at publication, after encoding"
        );
        assert_eq!(fs::read(path.join("keep")).unwrap(), FIRST);
        assert_eq!(entries(directory.path()), ["capture.ppm"]);
        assert_eq!(entries(&path), ["keep"]);
    }

    #[test]
    fn saved_frame_has_the_complete_ppm_header_dimensions_and_rgb_payload() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("capture.ppm");
        let mut canvas = Canvas::default();
        canvas.pixels.fill(0x010203);
        canvas.pixels[0] = 0xa1b2c3;
        *canvas.pixels.last_mut().unwrap() = 0xdeadbe;

        save_frame(&path, &canvas).unwrap();

        let bytes = fs::read(&path).unwrap();
        let header = b"P6\n1200 800\n255\n";
        assert!(bytes.starts_with(header));
        let pixels = &bytes[header.len()..];
        assert_eq!(pixels.len(), 1200 * 800 * 3);
        assert_eq!(&pixels[..3], &[0xa1, 0xb2, 0xc3]);
        assert_eq!(&pixels[pixels.len() - 3..], &[0xde, 0xad, 0xbe]);
        assert!(pixels[3..pixels.len() - 3]
            .as_chunks::<3>()
            .0
            .iter()
            .all(|pixel| *pixel == [1, 2, 3]));
        assert_eq!(entries(directory.path()), ["capture.ppm"]);
    }

    #[cfg(windows)]
    #[test]
    fn replacement_preserves_an_open_reader_that_shares_delete_access() {
        use std::{io::Read, os::windows::fs::OpenOptionsExt};

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("capture.ppm");
        fs::write(&path, FIRST).unwrap();
        // Capture readers on Windows explicitly permit atomic replacement.
        let mut reader = fs::OpenOptions::new()
            .read(true)
            .share_mode(0x0000_0001 | 0x0000_0002 | 0x0000_0004)
            .open(&path)
            .unwrap();

        publish_frame(&path, |output| output.write_all(SECOND)).unwrap();

        let mut held_bytes = Vec::new();
        reader.read_to_end(&mut held_bytes).unwrap();
        assert_eq!(held_bytes, FIRST);
        assert_eq!(fs::read(&path).unwrap(), SECOND);
        assert_eq!(entries(directory.path()), ["capture.ppm"]);
    }

    #[cfg(windows)]
    #[test]
    fn a_reader_denying_delete_returns_an_error_without_losing_the_old_capture() {
        use std::{io::Read, os::windows::fs::OpenOptionsExt};

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("capture.ppm");
        fs::write(&path, FIRST).unwrap();
        let mut reader = fs::OpenOptions::new()
            .read(true)
            .share_mode(0x0000_0001 | 0x0000_0002)
            .open(&path)
            .unwrap();

        let result = publish_frame(&path, |output| output.write_all(SECOND));

        assert!(result.is_err(), "a reader denied replacement permission");
        let mut held_bytes = Vec::new();
        reader.read_to_end(&mut held_bytes).unwrap();
        assert_eq!(held_bytes, FIRST);
        assert_eq!(fs::read(&path).unwrap(), FIRST);
        assert_eq!(entries(directory.path()), ["capture.ppm"]);
    }
}
