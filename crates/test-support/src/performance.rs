use serde::Deserialize;
use tor_protocol::{Action, Direction, StateView};

pub const SPEC: &str = include_str!("../../server/fixtures/performance-v1.json");

#[derive(Deserialize)]
pub struct Trace {
    pub version: u32,
    pub seed: u64,
    steps: Vec<Step>,
    pub secondary: Vec<Step>,
    pub traversal: Vec<Step>,
}
#[derive(Clone, Deserialize)]
pub struct Step {
    pub label: String,
    pub action: TraceAction,
    pub expected: String,
    #[serde(default = "one")]
    pub min_regions: u64,
}
fn one() -> u64 {
    1
}
#[derive(Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TraceAction {
    Move { direction: Direction },
    Door { open: bool },
    Wait,
}

impl Trace {
    pub fn load() -> Self {
        serde_json::from_str(SPEC).expect("versioned trace")
    }
    pub fn steps(&self, regions: u64) -> impl Iterator<Item = &Step> {
        self.steps.iter().filter(move |s| s.min_regions <= regions)
    }
}
impl Step {
    /// Resolve entity identities exclusively through the actor's disclosure.
    pub fn resolve(&self, view: &StateView) -> Action {
        assert!(view.observation.ready, "{}: actor not ready", self.label);
        match self.action {
            TraceAction::Move { direction } => Action::Move { direction },
            TraceAction::Wait => Action::Wait,
            TraceAction::Door { open } => {
                let mut doors = view
                    .observation
                    .visible_cells
                    .iter()
                    .filter_map(|c| c.door.as_ref())
                    .filter(|d| d.reachable && d.open != open)
                    .map(|d| d.id)
                    .collect::<Vec<_>>();
                doors.sort();
                doors.dedup();
                assert_eq!(
                    doors.len(),
                    1,
                    "{}: expected one disclosed reachable door",
                    self.label
                );
                Action::SetDoor {
                    door: doors[0],
                    open,
                }
            }
        }
    }
    /// Observable assertions also used by server harness tests. Internal region
    /// coordinates belong only in authoritative tests, never in client driving.
    pub fn verify(&self, before: &StateView, after: &StateView, accepted: bool) {
        if self.expected == "blocked" {
            assert!(!accepted, "{} unexpectedly accepted", self.label);
            assert_eq!(before, after, "{} advanced state", self.label);
            return;
        }
        assert!(accepted, "{} unexpectedly rejected", self.label);
        assert!(
            after.revision > before.revision,
            "{} failed to advance",
            self.label
        );
        let center = |s: &StateView| {
            s.observation
                .visible_cells
                .iter()
                .find(|c| c.position.x == 0 && c.position.y == 0 && c.position.z == 0)
                .unwrap()
                .key
                .clone()
        };
        match self.action {
            TraceAction::Move { direction } => {
                assert_ne!(center(before), center(after), "{} did not move", self.label);
                let (x, y, z) = match direction {
                    Direction::North => (0, -1, 0),
                    Direction::East => (1, 0, 0),
                    Direction::South => (0, 1, 0),
                    Direction::West => (-1, 0, 0),
                    Direction::NorthEast => (1, -1, 0),
                    Direction::SouthEast => (1, 1, 0),
                    Direction::SouthWest => (-1, 1, 0),
                    Direction::NorthWest => (-1, -1, 0),
                    Direction::Up => (0, 0, 1),
                    Direction::Down => (0, 0, -1),
                };
                let destination = before
                    .observation
                    .visible_cells
                    .iter()
                    .find(|c| c.position == tor_protocol::Position { x, y, z })
                    .expect("trace destination must be disclosed");
                assert_eq!(
                    destination.key,
                    center(after),
                    "{} disclosed destination offset",
                    self.label
                );
            }
            TraceAction::Wait => assert_eq!(
                before.observation.visible_cells,
                after.observation.visible_cells
            ),
            TraceAction::Door { open } => {
                let Action::SetDoor { door, .. } = self.resolve(before) else {
                    unreachable!()
                };
                assert!(after
                    .observation
                    .visible_cells
                    .iter()
                    .filter_map(|c| c.door.as_ref())
                    .any(|d| d.id == door && d.open == open));
                assert_ne!(
                    before.observation.visible_cells,
                    after.observation.visible_cells
                );
            }
        }
    }
}
