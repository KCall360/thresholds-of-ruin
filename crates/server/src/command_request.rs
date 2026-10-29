//! Backend request metadata checked before private candidate capture.
//! Attachment and controller authority remain in the session. Operation-specific
//! targets and timing are still validated against candidate state.
use super::{valid_label, Command, CommandResult, Engine, Failure, Receipt};
use tor_protocol::ErrorCode;

/// Transient metadata proof for the uninterrupted private command pipeline.
/// This is neither an admitted intention nor an execution-time queue check.
pub(super) struct CheckedRequest<'a> {
    receipt: &'a Receipt,
    revision: u64,
    annotation: Option<crate::journal::RecordedAnchor>,
}

impl<'a> CheckedRequest<'a> {
    pub(super) fn into_parts(self) -> (&'a Receipt, u64, Option<crate::journal::RecordedAnchor>) {
        (self.receipt, self.revision, self.annotation)
    }
    pub(super) fn with_annotation_anchor(
        mut self,
        anchor: crate::journal::RecordedAnchor,
    ) -> Result<Self, Failure> {
        if !matches!(&self.receipt.command, Command::Annotate { anchor: raw, .. } if *raw == anchor.to_public())
        {
            return Err(super::invalid_anchor());
        }
        self.annotation = Some(anchor);
        Ok(self)
    }
}

impl Engine {
    /// Wizard authorization and identity validation precede receipt resolution.
    /// A matching receipt precedes branch/revision checks, preserving retries
    /// after the accepted request has advanced the game.
    pub(super) fn resolve_receipt(
        &self,
        receipt: &Receipt,
    ) -> Result<Option<CommandResult>, Failure> {
        if matches!(receipt.command, Command::Wizard { .. }) && !self.wizard_enabled {
            return Err(Failure::new(
                ErrorCode::Unauthorized,
                "Wizard operations are disabled",
            ));
        }
        if !valid_label(&receipt.user)
            || !valid_label(&receipt.frontend)
            || !valid_label(&receipt.request_id)
        {
            return Err(Failure::new(
                ErrorCode::InvalidRequest,
                "Invalid identity or request ID",
            ));
        }
        self.retry(
            &receipt.user,
            receipt.actor,
            &receipt.request_id,
            &receipt.branch,
            &receipt.command,
        )
    }

    pub(super) fn check_request<'a>(
        &self,
        receipt: &'a Receipt,
    ) -> Result<CheckedRequest<'a>, Failure> {
        let revision = self.check_metadata(
            receipt.actor,
            &receipt.branch,
            receipt.command.boundary_requirement(),
        )?;
        if let Command::AdmitIntention { action, .. } | Command::Act { action, .. } =
            &receipt.command
        {
            if action.advancement_basis().is_some_and(|basis| {
                !basis.valid()
                    || basis.actor.0 != receipt.actor.0
                    || basis.branch != receipt.branch.0
            }) {
                return Err(Failure::new(
                    ErrorCode::StaleContext,
                    "Advancement changed; reopen its preview",
                ));
            }
        }
        let annotation = match &receipt.command {
            Command::Annotate {
                anchor: tor_protocol::Anchor::Entry { id },
                ..
            } => Some(crate::journal::RecordedAnchor::Entry { id: id.clone() }),
            _ => None,
        };
        Ok(CheckedRequest {
            receipt,
            revision,
            annotation,
        })
    }
}

impl Command {
    pub(crate) fn boundary_requirement(&self) -> Option<(u64, &'static str)> {
        match self {
            Command::AdmitIntention {
                expected_boundary, ..
            }
            | Command::ResumeIntention {
                expected_boundary, ..
            }
            | Command::CancelIntention {
                expected_boundary, ..
            }
            | Command::Act {
                expected_boundary, ..
            } => Some((*expected_boundary, "Refresh the observation before acting")),
            Command::Travel {
                expected_boundary, ..
            } => Some((*expected_boundary, "Refresh before travelling")),
            Command::RenamePlace {
                expected_boundary, ..
            } => Some((*expected_boundary, "Refresh before naming a place")),
            Command::Wizard {
                expected_boundary, ..
            } => Some((*expected_boundary, "Refresh before a wizard operation")),
            Command::Annotate { .. } | Command::PausePreparation => None,
        }
    }
}

impl Engine {
    pub(super) fn check_metadata(
        &self,
        actor: tor_protocol::ActorId,
        branch: &tor_protocol::BranchId,
        expected: Option<(u64, &'static str)>,
    ) -> Result<u64, Failure> {
        if branch != self.branch() {
            return Err(Failure::new(
                ErrorCode::WrongBranch,
                "Reconnect to the current branch",
            ));
        }
        let revision = self.causal_boundary(actor)?;
        if let Some((expected, message)) = expected {
            if expected != revision {
                return Err(Failure::new(ErrorCode::StaleRevision, message));
            }
        }
        Ok(revision)
    }
}
