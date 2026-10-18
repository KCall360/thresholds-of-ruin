//! Private evidence for the one human-input action translation. Receipt inputs
//! stay unchanged, and neither simulation work nor a later retry reselects a hold.
use super::*;
use std::borrow::Cow;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum ActionResolution {
    Unchanged,
    MoveEscape { hold: tor_simulation::HoldId },
}

impl<'de> Deserialize<'de> for ActionResolution {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            Unchanged {},
            MoveEscape { hold: tor_simulation::HoldId },
        }
        Ok(match crate::storage::schema::named_map(d)? {
            Fields::Unchanged {} => Self::Unchanged,
            Fields::MoveEscape { hold } => Self::MoveEscape { hold },
        })
    }
}

impl ActionResolution {
    pub(super) fn resolve<'a>(
        game: &Game,
        actor: ActorId,
        requested: &'a Action,
    ) -> (Cow<'a, Action>, Self) {
        if matches!(requested, Action::Move { .. }) {
            // Incoming links alone are active holds; protection slots are not
            // candidates. Identity order is creation order, independent of the
            // holder's identity, visibility, or the incoming slot order.
            if let Some(hold) = game
                .linked_holds(SimActor(actor.0))
                .iter()
                .map(|link| link.id)
                .min()
            {
                return (
                    Cow::Owned(Action::Escape { hold }),
                    Self::MoveEscape { hold },
                );
            }
        }
        (Cow::Borrowed(requested), Self::Unchanged)
    }

    fn matches(self, requested: &Action, resolved: &Action) -> bool {
        match self {
            Self::Unchanged => requested == resolved,
            Self::MoveEscape { hold } => {
                hold.0 > 0
                    && hold.0 < u64::MAX
                    && matches!(requested, Action::Move { .. })
                    && matches!(resolved, Action::Escape { hold: actual } if *actual == hold)
            }
        }
    }
}

impl Record {
    /// Structural linkage remains verifiable after a historical boundary expires.
    /// It grants only the named Move-to-Escape translation, never equivalence
    /// between arbitrary request and executed actions.
    pub(super) fn valid_action_resolution(&self) -> bool {
        let Some(receipt) = &self.receipt else {
            return self.action_resolution == ActionResolution::Unchanged;
        };
        let (requested, resolved) = match (&receipt.command, &self.entry.content) {
            (
                Command::AdmitIntention {
                    action: requested, ..
                },
                JournalContent::IntentionAdmitted {
                    action: resolved, ..
                },
            ) => (requested, resolved),
            (
                Command::Act {
                    action: requested, ..
                },
                JournalContent::Action {
                    action: resolved,
                    event,
                },
            ) => {
                if let ActionResolution::MoveEscape { hold } = self.action_resolution {
                    if !matches!(event, crate::journal::Event::EscapeStarted { hold: actual } if *actual == hold)
                    {
                        return false;
                    }
                }
                (requested, resolved)
            }
            (Command::Act { .. } | Command::AdmitIntention { .. }, _) => return false,
            _ => return self.action_resolution == ActionResolution::Unchanged,
        };
        if !self.action_resolution.matches(requested, resolved) {
            return false;
        }
        if matches!(self.action_resolution, ActionResolution::MoveEscape { .. }) {
            receipt.actor == self.entry.actor
                && receipt.branch == self.entry.branch
                && self.entry.author
                    == Author::User {
                        user: receipt.user.clone(),
                    }
                && self.entry.audience
                    == if matches!(receipt.command, Command::AdmitIntention { .. }) {
                        Audience::Private
                    } else {
                        Audience::Actor
                    }
        } else {
            true
        }
    }

    /// Verify the choice against the exact preceding authoritative state. Never
    /// use a checkpoint's final state to reinterpret an old accepted request.
    pub(super) fn valid_action_resolution_at(&self, before: &Game) -> bool {
        if !self.valid_action_resolution() {
            return false;
        }
        let Some(receipt) = &self.receipt else {
            return true;
        };
        let requested = match &receipt.command {
            Command::Act { action, .. } | Command::AdmitIntention { action, .. } => action,
            _ => return true,
        };
        let (_, expected) = ActionResolution::resolve(before, receipt.actor, requested);
        self.action_resolution == expected
    }
}
