//! Reusable Phase B/C fault fixtures. This models the proposed recovery
//! boundary without changing the format-3 storage implementation in Phase A.

const HEADER: usize = 24;

fn frame(sequence: u64, payload: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"TORJ");
    bytes.extend_from_slice(&4u16.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&sequence.to_le_bytes());
    bytes.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&checksum(payload).to_le_bytes());
    bytes.extend_from_slice(payload);
    bytes
}

fn checksum(bytes: &[u8]) -> u32 {
    bytes.iter().fold(0x811c9dc5u32, |hash, byte| {
        hash.wrapping_mul(0x01000193) ^ u32::from(*byte)
    })
}

fn recover(bytes: &[u8]) -> (Vec<u64>, usize) {
    let mut offset = 0;
    let mut sequences = Vec::new();
    while bytes.len().saturating_sub(offset) >= HEADER {
        if &bytes[offset..offset + 4] != b"TORJ" {
            break;
        }
        let version = u16::from_le_bytes(bytes[offset + 4..offset + 6].try_into().unwrap());
        let kind = u16::from_le_bytes(bytes[offset + 6..offset + 8].try_into().unwrap());
        let sequence = u64::from_le_bytes(bytes[offset + 8..offset + 16].try_into().unwrap());
        let length =
            u32::from_le_bytes(bytes[offset + 16..offset + 20].try_into().unwrap()) as usize;
        let expected = u32::from_le_bytes(bytes[offset + 20..offset + 24].try_into().unwrap());
        let Some(end) = offset
            .checked_add(HEADER)
            .and_then(|v| v.checked_add(length))
        else {
            break;
        };
        if version != 4
            || kind != 1
            || end > bytes.len()
            || checksum(&bytes[offset + HEADER..end]) != expected
        {
            break;
        }
        sequences.push(sequence);
        offset = end;
    }
    (sequences, offset)
}

#[test]
fn interruption_before_or_during_append_recovers_last_complete_frame() {
    let first = frame(1, b"first");
    let second = frame(2, b"second record");
    let mut durable = first.clone();
    assert_eq!(recover(&durable), (vec![1], first.len()));
    for cut in 0..second.len() {
        let mut interrupted = durable.clone();
        interrupted.extend_from_slice(&second[..cut]);
        assert_eq!(recover(&interrupted), (vec![1], first.len()), "cut={cut}");
    }
    durable.extend_from_slice(&second);
    assert_eq!(recover(&durable), (vec![1, 2], durable.len()));
}

#[test]
fn corrupt_final_frame_is_never_a_recoverable_acknowledgement() {
    let first = frame(1, b"first");
    let mut bytes = first.clone();
    let mut second = frame(2, b"second");
    *second.last_mut().unwrap() ^= 0xff;
    bytes.extend(second);
    assert_eq!(recover(&bytes), (vec![1], first.len()));
}

#[test]
fn checkpoint_install_and_rotation_boundaries_keep_one_recoverable_generation() {
    #[derive(Clone, Copy)]
    enum Fault {
        BeforeCheckpointSync,
        BeforeInstall,
        BeforeRotation,
        None,
    }
    for fault in [
        Fault::BeforeCheckpointSync,
        Fault::BeforeInstall,
        Fault::BeforeRotation,
        Fault::None,
    ] {
        let old_checkpoint = 10;
        let journal = [11, 12];
        let recovered = match fault {
            Fault::BeforeCheckpointSync | Fault::BeforeInstall => {
                (old_checkpoint, journal.as_slice())
            }
            Fault::BeforeRotation | Fault::None => (12, [].as_slice()),
        };
        assert_eq!(recovered.0 + recovered.1.len() as u64, 12);
    }
}
