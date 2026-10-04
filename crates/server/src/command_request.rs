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
}

impl<'a> CheckedRequest<'a> {
    pub(super) fn into_parts(self) -> (&'a Receipt, u64) {
        (self.receipt, self.revision)
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
        if &receipt.branch != self.branch() {
            return Err(Failure::new(
                ErrorCode::WrongBranch,
                "Reconnect to the current branch",
            ));
        }
        let revision = self.revision(receipt.actor)?;
        let expected = match &receipt.command {
            Command::Act {
                expected_revision, ..
            } => Some((*expected_revision, "Refresh the observation before acting")),
            Command::Travel {
                expected_revision, ..
            } => Some((*expected_revision, "Refresh before travelling")),
            Command::RenamePlace {
                expected_revision, ..
            } => Some((*expected_revision, "Refresh before naming a place")),
            Command::Wizard {
                expected_revision, ..
            } => Some((*expected_revision, "Refresh before a wizard operation")),
            Command::Annotate { .. } | Command::PausePreparation => None,
        };
        if let Some((expected, message)) = expected {
            if expected != revision {
                return Err(Failure::new(ErrorCode::StaleRevision, message));
            }
        }
        Ok(CheckedRequest { receipt, revision })
    }
}
