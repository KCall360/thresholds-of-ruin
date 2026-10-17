//! Recover presentation translation from uniquely disclosed cell identities.
use std::collections::BTreeMap;
use tor_protocol::{BranchId, CellView, Position, StreamContext};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameChange {
    pub offset: Position,
    pub discontinuous: bool,
}

#[derive(Default)]
pub struct SceneFrame {
    previous: BTreeMap<String, Position>,
    offset: [i32; 3],
    identity: Option<(StreamContext, BranchId)>,
}

impl SceneFrame {
    pub fn observe(
        &mut self,
        cells: &[CellView],
        context: &StreamContext,
        branch: &BranchId,
    ) -> FrameChange {
        let mut counts = BTreeMap::<&str, (Position, usize)>::new();
        for cell in cells {
            let entry = counts.entry(&cell.key).or_insert((cell.position, 0));
            entry.1 += 1;
        }
        let mut shift = None;
        let mut conflict = false;
        for (key, (now, count)) in &counts {
            if *count != 1 {
                continue;
            }
            if let Some(before) = self.previous.get(*key) {
                let delta = [
                    i64::from(now.x) - i64::from(before.x),
                    i64::from(now.y) - i64::from(before.y),
                    i64::from(now.z) - i64::from(before.z),
                ];
                conflict |= shift.is_some_and(|old| old != delta);
                shift = Some(delta);
            }
        }
        let same_identity = self
            .identity
            .as_ref()
            .is_some_and(|(old_context, old_branch)| {
                old_context == context && old_branch == branch
            });
        let translated = shift
            .filter(|_| same_identity && !conflict)
            .and_then(|delta| {
                Some([
                    i32::try_from(i64::from(self.offset[0]) - delta[0]).ok()?,
                    i32::try_from(i64::from(self.offset[1]) - delta[1]).ok()?,
                    i32::try_from(i64::from(self.offset[2]) - delta[2]).ok()?,
                ])
            });
        self.offset = translated.unwrap_or([0; 3]);
        self.identity = Some((context.clone(), branch.clone()));
        self.previous = counts
            .into_iter()
            .filter(|(_, (_, n))| *n == 1)
            .map(|(key, (position, _))| (key.to_owned(), position))
            .collect();
        FrameChange {
            offset: Position {
                x: self.offset[0],
                y: self.offset[1],
                z: self.offset[2],
            },
            discontinuous: translated.is_none(),
        }
    }
}
