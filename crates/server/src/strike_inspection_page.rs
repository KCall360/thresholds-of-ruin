//! Project at most one bounded page, directly from borrowed indexed source rows.
use super::{attack_view, check_view, effect_view, rider_check_view};
use crate::{
    creature_view::{descriptor, named, talent},
    Failure,
};
use p::{StrikePageEntry as E, StrikePagePosition as P};
use tor_protocol as p;
use tor_simulation as s;

fn unavailable() -> Failure {
    Failure::new(p::ErrorCode::InvalidAction, "Strike source is unavailable")
}
fn invalid() -> Failure {
    Failure::new(
        p::ErrorCode::InvalidArchive,
        "Strike source entry exceeds the inspection page contract",
    )
}

enum Body<'a> {
    Strikes(&'a [s::rules::StrikeDef]),
    Weapon(&'a s::attacks::MeleeAttack),
    SelectedMelee,
}
enum Entry<'a> {
    Header(u16, &'a s::rules::StrikeDef),
    Effect(u16, Option<u16>, u16, bool, &'a s::rules::Effect),
    Descriptor(u16, Option<u16>, u16, &'a s::content_ids::DescriptorId),
    Rider(u16, u16, &'a s::rules::RiderDef),
    Weapon(&'a s::attacks::MeleeAttack),
    SelectedMelee,
}
fn check_fields(check: &s::rules::CheckDef) -> usize {
    2 + match &check.against {
        s::rules::Against::Difficulty { formula } => {
            1 + usize::from(formula.skill.is_some()) + usize::from(formula.attribute.is_some())
        }
        _ => 1,
    }
}
fn effect_fields(effect: &s::rules::Effect) -> usize {
    match effect {
        s::rules::Effect::Damage {
            damage: s::rules::DamageExpr::Components { components },
        } => components.len(),
        s::rules::Effect::Heal { .. } => 1,
        s::rules::Effect::ObservePublicPlan | s::rules::Effect::StepSelfOneCell => 0,
        s::rules::Effect::PushTargetOneCell { .. } => 1,
        s::rules::Effect::ApplyQualifiedConditionToSource { .. } => 3,
        _ => 2,
    }
}
fn reference_valid(id: &str, name: &str) -> bool {
    id.len() <= 60 && name.len() <= 256 && !name.chars().any(char::is_control)
}
impl Entry<'_> {
    fn fields(&self) -> usize {
        match self {
            Self::Header(_, strike) => strike.check.as_ref().map_or(0, check_fields),
            Self::Rider(_, _, rider) => {
                if rider.check.is_some() {
                    3
                } else {
                    0
                }
            }
            Self::Effect(_, _, _, _, effect) => effect_fields(effect),
            Self::Weapon(attack) => attack.damage().components().len(),
            _ => 0,
        }
    }
    /// Check operand counts and borrowed reference strings before cloning/rendering.
    fn preflight(&self, rules: &s::rules::Ruleset) -> Result<usize, Failure> {
        let descriptor_ok = |id: &s::content_ids::DescriptorId| {
            rules
                .descriptor(id)
                .is_some_and(|d| reference_valid(d.id.as_str(), &d.name))
        };
        let fields = match self {
            Self::Header(_, strike) => {
                if strike.unlock.as_ref().is_some_and(|id| {
                    !rules
                        .talent(id)
                        .is_some_and(|d| reference_valid(d.id.as_str(), &d.name))
                }) || strike.on_success.len() > 128
                    || strike.on_failure.len() > 128
                    || strike.descriptors.len() > 128
                    || strike.riders.len() > 8
                {
                    return Err(invalid());
                }
                strike.check.as_ref().map_or(0, check_fields)
            }
            Self::Rider(_, _, rider) => {
                if rider.effects.len() > 128 || rider.descriptors.len() > 128 {
                    return Err(invalid());
                }
                if rider.check.is_some() {
                    3
                } else {
                    0
                }
            }
            Self::Descriptor(_, _, _, id) => {
                if !descriptor_ok(id) {
                    return Err(invalid());
                }
                0
            }
            Self::Effect(_, _, _, _, effect) => {
                match effect {
                    s::rules::Effect::Damage {
                        damage: s::rules::DamageExpr::Components { components },
                    } => {
                        if components.len() > 32
                            || components.iter().any(|c| {
                                c.descriptor
                                    .as_ref()
                                    .is_some_and(|id| id.as_str().len() > 60)
                            })
                        {
                            return Err(invalid());
                        }
                    }
                    s::rules::Effect::ApplyCondition { condition, .. }
                        if !rules
                            .condition(condition)
                            .is_some_and(|d| reference_valid(d.id.as_str(), &d.name)) =>
                    {
                        return Err(invalid());
                    }
                    _ => {}
                }
                effect_fields(effect)
            }
            Self::Weapon(attack) => {
                if attack.damage().components().len() > 32
                    || attack.damage().components().iter().any(|c| {
                        c.key()
                            .descriptor
                            .as_ref()
                            .is_some_and(|id| !descriptor_ok(id))
                    })
                {
                    return Err(invalid());
                }
                attack.damage().components().len()
            }
            Self::SelectedMelee => 0,
        };
        if fields > 32 {
            return Err(invalid());
        }
        Ok(fields)
    }
    fn project(self, rules: &s::rules::Ruleset) -> E {
        match self {
            Self::Header(strike, row) => E::StrikeHeader {
                strike,
                offset: row.offset,
                unlock: row.unlock.as_ref().map(|id| talent(id, rules)),
                check: row.check.as_ref().map(check_view),
                success_count: row.on_success.len() as u16,
                failure_count: row.on_failure.len() as u16,
                descriptor_count: row.descriptors.len() as u16,
                rider_count: row.riders.len() as u16,
            },
            Self::Rider(strike, rider, row) => E::RiderHeader {
                strike,
                rider,
                trigger: match row.trigger {
                    s::rules::RiderTrigger::Success => p::RiderTrigger::Success,
                    s::rules::RiderTrigger::Failure => p::RiderTrigger::Failure,
                },
                check: row.check.as_ref().map(rider_check_view),
                effect_count: row.effects.len() as u16,
                descriptor_count: row.descriptors.len() as u16,
            },
            Self::Effect(strike, rider, index, success, row) => {
                let row = effect_view(row, rules);
                match rider {
                    Some(rider) => E::RiderEffect {
                        strike,
                        rider,
                        index,
                        row,
                    },
                    None if success => E::SuccessEffect { strike, index, row },
                    None => E::FailureEffect { strike, index, row },
                }
            }
            Self::Descriptor(strike, rider, index, id) => {
                let descriptor = descriptor(id, Some(rules));
                match rider {
                    Some(rider) => E::RiderDescriptor {
                        strike,
                        rider,
                        index,
                        descriptor,
                    },
                    None => E::StrikeDescriptor {
                        strike,
                        index,
                        descriptor,
                    },
                }
            }
            Self::Weapon(attack) => E::Weapon {
                attack: attack_view(attack, Some(rules)),
            },
            Self::SelectedMelee => E::SelectedMelee {},
        }
    }
}
impl<'a> Body<'a> {
    fn first(&self) -> P {
        match self {
            Self::Strikes(_) => P::StrikeHeader { strike: 0 },
            Self::Weapon(_) => P::Weapon {},
            Self::SelectedMelee => P::SelectedMelee {},
        }
    }
    fn count(&self) -> u16 {
        match self {
            Self::Strikes(rows) => rows.len() as u16,
            Self::Weapon(_) => 1,
            Self::SelectedMelee => 0,
        }
    }
    fn entry(&self, position: &P) -> Option<Entry<'a>> {
        let rows = match self {
            Self::Weapon(attack) => {
                return matches!(position, P::Weapon {}).then_some(Entry::Weapon(attack))
            }
            Self::SelectedMelee => {
                return matches!(position, P::SelectedMelee {}).then_some(Entry::SelectedMelee)
            }
            Self::Strikes(rows) => *rows,
        };
        let strike = position.strike()?;
        let row = rows.get(usize::from(strike))?;
        Some(match *position {
            P::StrikeHeader { .. } => Entry::Header(strike, row),
            P::SuccessEffect { index, .. } => Entry::Effect(
                strike,
                None,
                index,
                true,
                row.on_success.get(usize::from(index))?,
            ),
            P::FailureEffect { index, .. } => Entry::Effect(
                strike,
                None,
                index,
                false,
                row.on_failure.get(usize::from(index))?,
            ),
            P::StrikeDescriptor { index, .. } => Entry::Descriptor(
                strike,
                None,
                index,
                row.descriptors.get(usize::from(index))?,
            ),
            P::RiderHeader { rider, .. } => {
                Entry::Rider(strike, rider, row.riders.get(usize::from(rider))?)
            }
            P::RiderEffect { rider, index, .. } => Entry::Effect(
                strike,
                Some(rider),
                index,
                true,
                row.riders
                    .get(usize::from(rider))?
                    .effects
                    .get(usize::from(index))?,
            ),
            P::RiderDescriptor { rider, index, .. } => Entry::Descriptor(
                strike,
                Some(rider),
                index,
                row.riders
                    .get(usize::from(rider))?
                    .descriptors
                    .get(usize::from(index))?,
            ),
            _ => return None,
        })
    }
    /// Each boundary makes only fixed-count list-length checks; no prior-body scan.
    fn next(&self, position: &P) -> Option<P> {
        let Self::Strikes(rows) = self else {
            return None;
        };
        let strike = position.strike()?;
        let row = rows.get(usize::from(strike))?;
        let next_strike = || {
            (usize::from(strike) + 1 < rows.len()).then_some(P::StrikeHeader { strike: strike + 1 })
        };
        let riders = || {
            if !row.riders.is_empty() {
                Some(P::RiderHeader { strike, rider: 0 })
            } else {
                next_strike()
            }
        };
        let descriptors = || {
            if !row.descriptors.is_empty() {
                Some(P::StrikeDescriptor { strike, index: 0 })
            } else {
                riders()
            }
        };
        let failure = || {
            if !row.on_failure.is_empty() {
                Some(P::FailureEffect { strike, index: 0 })
            } else {
                descriptors()
            }
        };
        match *position {
            P::StrikeHeader { .. } => {
                if !row.on_success.is_empty() {
                    Some(P::SuccessEffect { strike, index: 0 })
                } else {
                    failure()
                }
            }
            P::SuccessEffect { index, .. } => {
                if usize::from(index) + 1 < row.on_success.len() {
                    Some(P::SuccessEffect {
                        strike,
                        index: index + 1,
                    })
                } else {
                    failure()
                }
            }
            P::FailureEffect { index, .. } => {
                if usize::from(index) + 1 < row.on_failure.len() {
                    Some(P::FailureEffect {
                        strike,
                        index: index + 1,
                    })
                } else {
                    descriptors()
                }
            }
            P::StrikeDescriptor { index, .. } => {
                if usize::from(index) + 1 < row.descriptors.len() {
                    Some(P::StrikeDescriptor {
                        strike,
                        index: index + 1,
                    })
                } else {
                    riders()
                }
            }
            P::RiderHeader { rider, .. }
            | P::RiderEffect { rider, .. }
            | P::RiderDescriptor { rider, .. } => {
                let source = row.riders.get(usize::from(rider))?;
                let next_rider = || {
                    if usize::from(rider) + 1 < row.riders.len() {
                        Some(P::RiderHeader {
                            strike,
                            rider: rider + 1,
                        })
                    } else {
                        next_strike()
                    }
                };
                let descriptors = || {
                    if !source.descriptors.is_empty() {
                        Some(P::RiderDescriptor {
                            strike,
                            rider,
                            index: 0,
                        })
                    } else {
                        next_rider()
                    }
                };
                match *position {
                    P::RiderHeader { .. } => {
                        if !source.effects.is_empty() {
                            Some(P::RiderEffect {
                                strike,
                                rider,
                                index: 0,
                            })
                        } else {
                            descriptors()
                        }
                    }
                    P::RiderEffect { index, .. } => {
                        if usize::from(index) + 1 < source.effects.len() {
                            Some(P::RiderEffect {
                                strike,
                                rider,
                                index: index + 1,
                            })
                        } else {
                            descriptors()
                        }
                    }
                    P::RiderDescriptor { index, .. } => {
                        if usize::from(index) + 1 < source.descriptors.len() {
                            Some(P::RiderDescriptor {
                                strike,
                                rider,
                                index: index + 1,
                            })
                        } else {
                            next_rider()
                        }
                    }
                    _ => None,
                }
            }
            _ => None,
        }
    }
}

#[cfg(test)]
fn owned_string_bytes(entry: &E) -> usize {
    fn reference(value: &p::ContentRef) -> usize {
        value.id.len() + value.name.len()
    }
    fn row(value: &p::CombatTraceStep) -> usize {
        value.id.len()
            + value.name.len()
            + value
                .fields
                .iter()
                .map(|field| {
                    field.id.len()
                        + field.name.len()
                        + match &field.value {
                            p::DiagnosticValue::Text { value } => value.len(),
                            p::DiagnosticValue::Content { value } => reference(value),
                            _ => 0,
                        }
                })
                .sum::<usize>()
    }
    match entry {
        E::StrikeHeader { unlock, check, .. } => {
            unlock.as_ref().map_or(0, reference) + check.as_ref().map_or(0, row)
        }
        E::RiderHeader { check, .. } => check.as_ref().map_or(0, row),
        E::SuccessEffect { row: value, .. }
        | E::FailureEffect { row: value, .. }
        | E::RiderEffect { row: value, .. } => row(value),
        E::StrikeDescriptor { descriptor, .. } | E::RiderDescriptor { descriptor, .. } => {
            reference(descriptor)
        }
        E::Weapon { attack } => {
            reference(&attack.skill)
                + attack
                    .damage
                    .primary
                    .descriptor
                    .as_ref()
                    .map_or(0, reference)
                + attack
                    .damage
                    .components
                    .iter()
                    .map(|c| c.descriptor.as_ref().map_or(0, reference))
                    .sum::<usize>()
        }
        E::SelectedMelee {} => 0,
    }
}

/// Complete reply metadata is bounded independently from body entries. The empty
/// envelope reserves the longest typed continuation before any row is projected.
pub(crate) struct PageBudget<'a> {
    pub context: &'a p::ReplyContext,
    pub request_id: &'a str,
    pub inspection: &'a p::StrikeInspectionId,
    pub actor: p::ActorId,
    pub target_revision: u64,
    pub source: &'a p::StrikeSourceSelector,
    pub limit: usize,
}
fn capacity() -> Failure {
    Failure::new(
        p::ErrorCode::ResourceLimit,
        "Strike detail entry exceeds the advertised frame capacity",
    )
}
#[derive(serde::Serialize)]
struct PageEnvelope<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    context: &'a p::ReplyContext,
    request_id: &'a str,
    page: &'a p::CreatureStrikePage,
}
impl PageBudget<'_> {
    fn available(
        &self,
        resolved: &p::StrikeSourceAttribution,
        strike_count: u16,
        start: &P,
    ) -> Result<usize, Failure> {
        let page = p::CreatureStrikePage {
            inspection: self.inspection.clone(),
            actor: self.actor,
            target_revision: self.target_revision,
            source: self.source.clone(),
            resolved: resolved.clone(),
            strike_count,
            start: start.clone(),
            entries: Vec::new(),
            next: Some(p::StrikePageCursor {
                inspection: self.inspection.clone(),
                position: P::RiderDescriptor {
                    strike: 15,
                    rider: 7,
                    index: 127,
                },
            }),
        };
        let envelope = PageEnvelope {
            kind: "creature_strike_page",
            context: self.context,
            request_id: self.request_id,
            page: &page,
        };
        let limit = self.limit.min(p::MAX_STRIKE_PAGE_BYTES);
        let length = p::encoded_length(&envelope, limit)
            .map_err(|_| invalid())?
            .ok_or_else(capacity)?;
        Ok(limit - length)
    }
    pub(crate) fn check(&self, page: &p::CreatureStrikePage) -> Result<(), Failure> {
        let envelope = PageEnvelope {
            kind: "creature_strike_page",
            context: self.context,
            request_id: self.request_id,
            page,
        };
        p::encoded_length(&envelope, self.limit.min(p::MAX_STRIKE_PAGE_BYTES))
            .map_err(|_| invalid())?
            .ok_or_else(capacity)?;
        Ok(())
    }
}
pub(crate) struct ProjectedPage {
    pub resolved: p::StrikeSourceAttribution,
    pub strike_count: u16,
    pub start: P,
    pub entries: Vec<E>,
    pub next: Option<P>,
}
pub(crate) fn project(
    game: &s::Game,
    actor: s::ActorId,
    selector: &p::StrikeSourceSelector,
    start: Option<&P>,
    budget: Option<&PageBudget<'_>>,
) -> Result<ProjectedPage, Failure> {
    // No OwnStats/list-of-grants projection and no unselected authored bodies.
    let creature = game.creature(actor).ok_or_else(unavailable)?;
    #[cfg(test)]
    crate::creature_view::strike_page_tests::authority_projected(matches!(
        &creature,
        std::borrow::Cow::Owned(_)
    ));
    let build = creature.build();
    let rules = build.ruleset();
    let derived = creature.derived();
    let (body, resolved) = match selector {
        p::StrikeSourceSelector::Natural {} => (
            Body::Strikes(build.species().melee.strikes()),
            p::StrikeSourceAttribution::Natural {
                species: named(build.species_id(), None),
            },
        ),
        p::StrikeSourceSelector::SelectedMelee {} => match &derived.selected_attack.attack {
            s::attacks::SelectedMeleeAttack::Natural(attack) => (
                Body::Strikes(attack.strikes()),
                p::StrikeSourceAttribution::Natural {
                    species: named(build.species_id(), None),
                },
            ),
            s::attacks::SelectedMeleeAttack::Weapon(attack) => {
                let (_, source) = super::source(&derived.selected_attack.source, rules);
                (
                    Body::Weapon(attack),
                    p::StrikeSourceAttribution::Weapon {
                        source,
                        socket: derived.selected_attack.socket.ok_or_else(unavailable)?.0,
                    },
                )
            }
        },
        p::StrikeSourceSelector::Ability { id } => {
            let id = s::content_ids::AbilityId::new(id).map_err(|_| unavailable())?;
            if !derived.abilities.contains(&id) {
                return Err(unavailable());
            }
            let def = rules.ability(&id).ok_or_else(unavailable)?;
            if !reference_valid(def.id.as_str(), &def.name) {
                return Err(invalid());
            }
            let body = match &def.strikes {
                s::rules::StrikeSource::Authored { strikes } => Body::Strikes(strikes),
                s::rules::StrikeSource::SelectedMelee => Body::SelectedMelee,
            };
            (
                body,
                p::StrikeSourceAttribution::Ability {
                    ability: named(&def.id, Some(&def.name)),
                },
            )
        }
    };
    let start = start.cloned().unwrap_or_else(|| body.first());
    let mut available = budget
        .map(|budget| budget.available(&resolved, body.count(), &start))
        .transpose()?
        .unwrap_or(p::MAX_STRIKE_PAGE_BYTES - 8192);
    let mut position = Some(start.clone());
    let mut entries = Vec::with_capacity(p::MAX_STRIKE_PAGE_ENTRIES);
    let mut fields = 0;
    while let Some(current) = position.as_ref() {
        if entries.len() == p::MAX_STRIKE_PAGE_ENTRIES {
            break;
        }
        let entry = body.entry(current).ok_or_else(unavailable)?;
        let count = entry.fields();
        if fields + count > p::MAX_STRIKE_PAGE_FIELDS {
            break;
        }
        // Reserve the proven maximum projected size before any field allocation.
        // One comma is included even for the first entry, so arrays cannot exceed
        // the complete envelope reservation. Small host limits fail explicitly.
        let bound = 2048 + count * p::MAX_STRIKE_PAGE_FIELD_BYTES + 1;
        if bound > available {
            if entries.is_empty() {
                return Err(capacity());
            }
            break;
        }
        entry.preflight(rules)?;
        let projected = entry.project(rules);
        if !projected.valid() {
            return Err(invalid());
        }
        available -= bound;
        fields += count;
        #[cfg(test)]
        crate::creature_view::strike_page_tests::projected(
            1,
            count,
            owned_string_bytes(&projected),
        );
        entries.push(projected);
        position = body.next(current);
    }
    if entries.is_empty() {
        return Err(invalid());
    }
    Ok(ProjectedPage {
        resolved,
        strike_count: body.count(),
        start,
        entries,
        next: position,
    })
}
