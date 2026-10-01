//! At most 64 local intentions. A repeat is one of them.
//!
//! `Repeat` and pickup permission are client bookkeeping. They are not protocol types.

use std::collections::VecDeque;
use tor_protocol::{Action, Command, Direction, EntryId};

pub const QUEUE_CAP: usize = 64;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Intent {
    Act {
        action: Action,
        pickup: bool,
    },
    /// One adjacent step. Bump resolution happens at send time.
    Step {
        direction: Direction,
        pickup: bool,
        suppress_attack: bool,
    },
    Repeat {
        direction: Direction,
        remaining: u16,
        pickup: bool,
        suppress_attack: bool,
    },
    /// A counted `.` or numpad 5. `remaining` includes the step about to be sent.
    RepeatWait {
        remaining: u16,
    },
    Travel {
        destination: String,
        pickup: bool,
    },
    CancelTravel,
    Continue,
    Rename {
        key: String,
        name: String,
    },
    Annotate(Command),
    History {
        before: Option<EntryId>,
        limit: u16,
    },
    Acquire,
    Release,
}

#[derive(Clone, Debug, Default)]
pub struct IntentQueue {
    items: VecDeque<Intent>,
}

impl IntentQueue {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }

    pub fn front(&self) -> Option<&Intent> {
        self.items.front()
    }

    pub fn front_mut(&mut self) -> Option<&mut Intent> {
        self.items.front_mut()
    }

    pub fn pop_front(&mut self) -> Option<Intent> {
        self.items.pop_front()
    }

    /// `false` means the 64-intent cap was already full.
    pub fn push_back(&mut self, intent: Intent) -> bool {
        self.push(intent, false)
    }

    /// `false` means the 64-intent cap was already full.
    pub fn push_front(&mut self, intent: Intent) -> bool {
        self.push(intent, true)
    }

    pub fn has_cancel(&self) -> bool {
        self.items
            .iter()
            .any(|item| matches!(item, Intent::CancelTravel))
    }

    fn push(&mut self, intent: Intent, front: bool) -> bool {
        if self.items.len() >= QUEUE_CAP {
            return false;
        }
        if front {
            self.items.push_front(intent);
        } else {
            self.items.push_back(intent);
        }
        true
    }
}

/// What `try_send` did with the encoded front intent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrySend {
    Accepted,
    Full,
    Closed,
}

/// `Full` keeps the session. `Closed` is the worker dying.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SendFate {
    InFlight,
    Retry,
    Disconnect,
}

pub fn classify_try_send(result: TrySend) -> SendFate {
    match result {
        TrySend::Accepted => SendFate::InFlight,
        TrySend::Full => SendFate::Retry,
        TrySend::Closed => SendFate::Disconnect,
    }
}
