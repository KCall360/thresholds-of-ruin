//! Generic, bounded privileged diagnostics. These fields report facts; this
//! protocol does not re-run formulas from the selected ruleset.
use crate::{valid_content_id, valid_label, ContentRef};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
pub const MAX_COMBAT_TRACE_STEPS: usize = 128;
pub const MAX_DIAGNOSTIC_FIELDS: usize = 32;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidCombatDiagnostics;
impl std::fmt::Display for InvalidCombatDiagnostics {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Invalid combat diagnostics")
    }
}
impl std::error::Error for InvalidCombatDiagnostics {}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum DiagnosticValue {
    Text {
        value: String,
    },
    Signed {
        #[serde(with = "crate::integers::signed")]
        value: i64,
    },
    Unsigned {
        #[serde(with = "crate::integers::unsigned")]
        value: u64,
    },
    Bool {
        value: bool,
    },
    Dice {
        values: Vec<u16>,
    },
    Content {
        value: ContentRef,
    },
}
impl DiagnosticValue {
    pub(crate) fn valid(&self) -> bool {
        match self {
            Self::Text { value } => value.len() <= 4096 && !value.chars().any(char::is_control),
            Self::Dice { values } => values.len() <= 128,
            Self::Content { value } => value.valid(),
            _ => true,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiagnosticField {
    pub id: String,
    pub name: String,
    pub value: DiagnosticValue,
}
impl DiagnosticField {
    pub(crate) fn valid(&self) -> bool {
        valid_content_id(&self.id) && valid_label(&self.name, 256) && self.value.valid()
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CombatTraceStep {
    pub id: String,
    pub name: String,
    pub fields: Vec<DiagnosticField>,
}
impl CombatTraceStep {
    pub(crate) fn valid(&self) -> bool {
        let mut seen = BTreeSet::new();
        valid_content_id(&self.id)
            && valid_label(&self.name, 256)
            && self.fields.len() <= MAX_DIAGNOSTIC_FIELDS
            && self
                .fields
                .iter()
                .all(|field| field.valid() && seen.insert(&field.id))
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CombatTraceView {
    pub steps: Vec<CombatTraceStep>,
    pub truncated: bool,
}
impl CombatTraceView {
    pub fn validate(&self) -> Result<(), InvalidCombatDiagnostics> {
        if self.steps.len() <= MAX_COMBAT_TRACE_STEPS
            && self.steps.iter().all(CombatTraceStep::valid)
            && matches!(
                crate::codec::encoded_length(self, crate::MAX_RESPONSE_BYTES),
                Ok(Some(_))
            )
        {
            Ok(())
        } else {
            Err(InvalidCombatDiagnostics)
        }
    }
}
