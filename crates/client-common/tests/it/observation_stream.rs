use tor_client_common::{ObservationStream, StreamError};
use tor_protocol::{ActorId, StreamCursor};

fn cursor(sequence: u64, tick: u64) -> StreamCursor {
    StreamCursor { sequence, tick }
}

#[test]
fn pushed_updates_can_unfold_before_time_advances_again() {
    let mut stream = ObservationStream::from_snapshot(ActorId(7), cursor(0, 0));
    stream.accept(ActorId(7), cursor(1, 10)).unwrap();
    stream.accept(ActorId(7), cursor(2, 10)).unwrap();
    stream.accept(ActorId(7), cursor(3, 15)).unwrap();
    assert_eq!(stream.cursor(), cursor(3, 15));
}

#[test]
fn a_gap_requires_resynchronization_without_mutating_the_cursor() {
    let mut stream = ObservationStream::from_snapshot(ActorId(7), cursor(3, 15));
    assert_eq!(
        stream.accept(ActorId(7), cursor(5, 20)),
        Err(StreamError::SequenceMismatch)
    );
    assert_eq!(stream.cursor(), cursor(3, 15));
    let mut resumed = ObservationStream::from_snapshot(ActorId(7), cursor(5, 20));
    resumed.accept(ActorId(7), cursor(6, 21)).unwrap();
}

#[test]
fn actor_mismatch_duplicate_and_time_reversal_are_rejected_atomically() {
    let mut stream = ObservationStream::from_snapshot(ActorId(7), cursor(3, 15));
    for (actor, update, error) in [
        (ActorId(8), cursor(4, 16), StreamError::WrongActor),
        (ActorId(7), cursor(3, 15), StreamError::SequenceMismatch),
        (ActorId(7), cursor(4, 14), StreamError::TimeReversed),
    ] {
        assert_eq!(stream.accept(actor, update), Err(error));
        assert_eq!(stream.cursor(), cursor(3, 15));
    }
}

#[test]
fn a_stream_at_the_sequence_limit_must_start_a_new_snapshot() {
    let mut stream = ObservationStream::from_snapshot(ActorId(7), cursor(u64::MAX, 15));
    assert_eq!(
        stream.accept(ActorId(7), cursor(0, 16)),
        Err(StreamError::SequenceMismatch)
    );
    assert_eq!(stream.cursor(), cursor(u64::MAX, 15));
}
