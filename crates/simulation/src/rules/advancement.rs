//! Authored ordinary advancement timing, independent of character speed.
use super::RulesetError;
use serde::{Deserialize, Serialize};

/// Candidate balance values live in the rules document, never in the clock.
/// A zero purchase/replacement recovery still uses the ordinary action boundary.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct AdvancementPolicy {
    pub purchase_recovery: u64,
    pub respec_prepare: u64,
    pub respec_recovery: u64,
    pub received_loss_interval: u64,
    pub respec_ration: super::SupplyCost,
}

impl<'de> Deserialize<'de> for AdvancementPolicy {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            purchase_recovery: u64,
            respec_prepare: u64,
            respec_recovery: u64,
            received_loss_interval: u64,
            respec_ration: super::SupplyCost,
        }
        let fields: Fields = crate::bounded::named_map(deserializer)?;
        Ok(Self {
            purchase_recovery: fields.purchase_recovery,
            respec_prepare: fields.respec_prepare,
            respec_recovery: fields.respec_recovery,
            received_loss_interval: fields.received_loss_interval,
            respec_ration: fields.respec_ration,
        })
    }
}

impl AdvancementPolicy {
    pub(super) fn validate(&self) -> Result<(), RulesetError> {
        let maximum = crate::limits::NUMERIC as u64;
        super::bound(
            (1..=maximum).contains(&self.respec_prepare)
                && (1..=maximum).contains(&self.received_loss_interval)
                && self.purchase_recovery <= maximum
                && self.respec_recovery <= maximum,
            "advancement_timing",
        )?;
        super::bound(
            self.respec_ration.quantity == 1
                && self.respec_ration.spend_at == super::SupplySpendAt::ReplacementCommit,
            "respec_ration",
        )
    }
}
