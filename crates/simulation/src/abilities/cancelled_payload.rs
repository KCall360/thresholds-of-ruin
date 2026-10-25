//! Effective projection never edits the canonical frozen/source-validated plan.
use super::{AbilityPlan, FrozenStrike, RuleError, StrikeOrigin};
use crate::{Game, GameError};
use std::borrow::Cow;

pub(super) struct EffectiveStrike<'a> {
    pub strike: Cow<'a, FrozenStrike>,
    pub suppressed_riders: Vec<usize>,
}
impl Game {
    /// Resolution and current-preview owners share this projection; historical
    /// public observations retain their already captured values.
    pub(super) fn effective_cancelled_strike<'a>(
        &self,
        ability: &AbilityPlan,
        strike: &'a FrozenStrike,
    ) -> Result<EffectiveStrike<'a>, GameError> {
        let unchanged = || EffectiveStrike {
            strike: Cow::Borrowed(strike),
            suppressed_riders: vec![],
        };
        if !self
            .creature(ability.actor)
            .is_some_and(|actor| actor.cancelled())
        {
            return Ok(unchanged());
        }
        let invalid = || GameError::Rule(RuleError::InvalidPlan);
        let definition = self
            .content
            .rules()
            .ability(&ability.ability)
            .ok_or_else(invalid)?;
        let (source, additional) = match &strike.origin {
            StrikeOrigin::Ability { ordinal } => {
                let crate::rules::StrikeSource::Authored { strikes } = &definition.strikes else {
                    return Err(invalid());
                };
                (
                    strikes.get(usize::from(*ordinal)).ok_or_else(invalid)?,
                    &[][..],
                )
            }
            StrikeOrigin::Natural { species, ordinal } => {
                let source = self
                    .content
                    .catalog()
                    .species
                    .get(species)
                    .ok_or_else(invalid)?;
                (
                    source
                        .melee
                        .strikes()
                        .get(usize::from(*ordinal))
                        .ok_or_else(invalid)?,
                    &[][..],
                )
            }
            StrikeOrigin::NaturalMelee {
                species, ordinal, ..
            } => {
                let source = self
                    .content
                    .catalog()
                    .species
                    .get(species)
                    .ok_or_else(invalid)?;
                let crate::rules::StrikeSource::NaturalMelee { additional_riders } =
                    &definition.strikes
                else {
                    return Err(invalid());
                };
                (
                    source
                        .melee
                        .strikes()
                        .get(usize::from(*ordinal))
                        .ok_or_else(invalid)?,
                    additional_riders.as_slice(),
                )
            }
            StrikeOrigin::Weapon { .. } => return Ok(unchanged()),
        };
        let suppressed_riders: Vec<_> = source
            .riders
            .iter()
            .enumerate()
            .filter_map(|(index, rider)| (source.cancellable || rider.cancellable).then_some(index))
            .chain(additional.iter().enumerate().filter_map(|(index, rider)| {
                rider.cancellable.then_some(source.riders.len() + index)
            }))
            .collect();
        if !source.cancellable && suppressed_riders.is_empty() {
            return Ok(unchanged());
        }
        let mut projected = strike.clone();
        if source.cancellable {
            projected.on_success.clear();
            projected.on_failure.clear();
        }
        for &index in &suppressed_riders {
            let rider = projected.riders.get_mut(index).ok_or_else(invalid)?;
            rider.effects.clear();
            rider.check = None;
        }
        Ok(EffectiveStrike {
            strike: Cow::Owned(projected),
            suppressed_riders,
        })
    }
}
