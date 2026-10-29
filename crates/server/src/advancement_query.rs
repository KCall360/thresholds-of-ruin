//! Authorized read-only advancement projection. No journal, candidate cache or ledger.
use super::{ActorId, BranchId, Engine, Failure};
use crate::{
    advancement_adapter as adapter,
    advancement_page::{pack, PageBudget},
};
use tor_protocol as p;
use tor_simulation::{
    self as s,
    creatures::{CreatureBuild, DerivedCreature, PurchaseSelection},
};

fn unavailable() -> Failure {
    Failure::new(p::ErrorCode::InvalidAction, "Advancement is unavailable")
}
fn invalid() -> Failure {
    Failure::new(p::ErrorCode::InvalidArchive, "Invalid advancement source")
}
fn stale() -> Failure {
    Failure::new(
        p::ErrorCode::StaleContext,
        "Advancement changed; reopen its preview",
    )
}
fn small(value: usize) -> Result<u16, Failure> {
    u16::try_from(value).map_err(|_| invalid())
}
fn named(id: impl ToString, name: Option<&str>) -> p::AdvancementContentRef {
    let id = id.to_string();
    p::AdvancementContentRef {
        name: name
            .map(str::to_owned)
            .unwrap_or_else(|| crate::creature_view::label(&id)),
        id,
    }
}
fn skill(value: s::attributes::Skill) -> p::AdvancementContentRef {
    named(value.as_str(), None)
}
fn kind(value: s::rules::PurchaseKind) -> p::AdvancementKind {
    match value {
        s::rules::PurchaseKind::SkillRank => p::AdvancementKind::SkillRank,
        s::rules::PurchaseKind::Expertise => p::AdvancementKind::Expertise,
        s::rules::PurchaseKind::Talent => p::AdvancementKind::Talent,
    }
}
fn selection(value: &PurchaseSelection) -> p::PurchaseChoice {
    match value {
        PurchaseSelection::SkillRank(value) => p::PurchaseChoice::SkillRank {
            skill: value.as_str().into(),
        },
        PurchaseSelection::Expertise(value) => p::PurchaseChoice::Expertise {
            expertise: value.to_string(),
        },
        PurchaseSelection::Talent(value) => p::PurchaseChoice::Talent {
            talent: value.to_string(),
        },
        PurchaseSelection::TalentForm { talent, form } => p::PurchaseChoice::TalentForm {
            talent: talent.to_string(),
            form: form.to_string(),
        },
    }
}
fn pool(
    build: &CreatureBuild,
    id: &s::content_ids::PointPoolId,
) -> Result<p::AdvancementContentRef, Failure> {
    let definition = build.ruleset().point_pool(id).ok_or_else(invalid)?;
    Ok(named(id, Some(&definition.name)))
}
fn purchase_row(
    build: &CreatureBuild,
    derived: &DerivedCreature,
    owner: s::HdIndex,
    index: usize,
    value: &s::creatures::AdvancementPurchase,
) -> Result<p::AdvancementLedgerRow, Failure> {
    let (name, status) = match &value.selection {
        PurchaseSelection::SkillRank(value) => (skill(*value).name, p::AdvancementStatus::Active),
        PurchaseSelection::Expertise(value) => (
            build
                .ruleset()
                .expertise(value)
                .ok_or_else(invalid)?
                .name
                .clone(),
            p::AdvancementStatus::Active,
        ),
        PurchaseSelection::Talent(value) | PurchaseSelection::TalentForm { talent: value, .. } => {
            let definition = build.ruleset().talent(value).ok_or_else(invalid)?;
            (
                definition.name.clone(),
                if derived.active_talents.contains(value) {
                    p::AdvancementStatus::Active
                } else {
                    p::AdvancementStatus::Dormant
                },
            )
        }
    };
    Ok(p::AdvancementLedgerRow {
        source: p::ReplacementSource {
            owner: owner.get(),
            index: small(index)?,
        },
        pool: pool(build, &value.pool)?,
        selection: selection(&value.selection),
        name,
        status,
    })
}
fn balances(
    build: &CreatureBuild,
) -> Result<Vec<(s::HdIndex, s::creatures::PointPoolBalance)>, Failure> {
    // At most 256 HD times 16 pool grants. Each owner is traversed once, never
    // once per row. Descriptive names are cloned only for the requested page.
    let mut rows = Vec::new();
    for owner in 0..build.choices().len() {
        // Package pools fund authored innate purchases only. The advancement
        // balance surface is actionable, so it exposes personal pools alone.
        if build.ledger().entries()[owner]
            .owner()
            .is_some_and(|owner| !owner.is_personal())
        {
            continue;
        }
        let owner = s::HdIndex::from_usize(owner).map_err(|_| invalid())?;
        rows.extend(
            build
                .point_balances(owner)
                .map_err(|_| invalid())?
                .into_iter()
                .map(|row| (owner, row)),
        );
    }
    if rows.len() > p::MAX_ADVANCEMENT_BALANCE_ROWS {
        return Err(invalid());
    }
    Ok(rows)
}
fn balance_row(
    build: &CreatureBuild,
    owner: s::HdIndex,
    value: &s::creatures::PointPoolBalance,
) -> Result<p::AdvancementEntry, Failure> {
    let definition = build
        .ruleset()
        .point_pool(&value.pool)
        .ok_or_else(invalid)?;
    Ok(p::AdvancementEntry::Balance {
        value: p::AdvancementBalanceRow {
            owner: owner.get(),
            pool: pool(build, &value.pool)?,
            granted: value.granted,
            spent: value.spent,
            unspent: value.unspent(),
            allowed: definition.allowed.iter().copied().map(kind).collect(),
        },
    })
}

impl Engine {
    pub(crate) fn advancement_basis(
        &self,
        context: &p::ReplyContext,
    ) -> Result<p::AdvancementBasis, Failure> {
        let creature = self
            .game
            .creature(s::ActorId(context.actor.0))
            .ok_or_else(unavailable)?;
        Ok(p::AdvancementBasis {
            actor: context.actor,
            branch: context.branch.clone(),
            stream: context.input.stream.stream.clone(),
            reset: context.input.stream.epoch,
            build_epoch: creature.build_epoch().get(),
        })
    }
    pub(crate) fn check_advancement_basis(
        &self,
        context: &p::ReplyContext,
        basis: &p::AdvancementBasis,
    ) -> Result<(), Failure> {
        if &self.advancement_basis(context)? != basis {
            return Err(stale());
        }
        Ok(())
    }
    pub(crate) fn advancement_query(
        &self,
        actor: ActorId,
        branch: &BranchId,
        expected_revision: u64,
        query: &p::AdvancementQuery,
        budget: &PageBudget<'_>,
    ) -> Result<p::AdvancementPage, Failure> {
        let revision = self.check_metadata(
            actor,
            branch,
            Some((expected_revision, "Refresh before inspecting advancement")),
        )?;
        if budget.context.actor != actor
            || &budget.context.branch != branch
            || budget.context.revision != revision
        {
            return Err(stale());
        }
        let basis = self.advancement_basis(budget.context)?;
        if query.basis().is_some_and(|requested| requested != &basis) {
            return Err(stale());
        }
        let creature = self
            .game
            .creature(s::ActorId(actor.0))
            .ok_or_else(unavailable)?;
        let build = creature.build();
        let offset = usize::from(query.offset());
        match query {
            p::AdvancementQuery::Overview {} => {
                let value = self.advancement_overview(actor, &creature)?;
                pack(
                    basis,
                    revision,
                    query.clone(),
                    1,
                    [Ok(p::AdvancementEntry::Overview {
                        value: Box::new(value),
                    })],
                    budget,
                )
            }
            p::AdvancementQuery::IntellectExpertises { .. } => {
                let capacity = creature.derived().intellect_expertise_capacity();
                let total = build.intellect_expertise_choices().count();
                let rows =
                    build
                        .intellect_expertise_choices()
                        .skip(offset)
                        .map(|(threshold, choice)| {
                            let expertise = choice
                                .map(|id| {
                                    build
                                        .ruleset()
                                        .expertise(id)
                                        .map(|definition| named(id, Some(&definition.name)))
                                        .ok_or_else(invalid)
                                })
                                .transpose()?;
                            Ok(p::AdvancementEntry::IntellectExpertise {
                                value: p::AdvancementIntellectExpertiseRow {
                                    threshold,
                                    status: if threshold <= capacity {
                                        p::AdvancementStatus::Active
                                    } else {
                                        p::AdvancementStatus::Dormant
                                    },
                                    expertise,
                                },
                            })
                        });
                pack(basis, revision, query.clone(), total, rows, budget)
            }
            p::AdvancementQuery::Ledger { .. } => {
                let total = build
                    .choices()
                    .iter()
                    .map(|owner| owner.purchases.len())
                    .sum();
                let rows = build
                    .choices()
                    .iter()
                    .enumerate()
                    .flat_map(|(owner, choices)| {
                        choices
                            .purchases
                            .iter()
                            .enumerate()
                            .map(move |(index, row)| (owner, index, row))
                    })
                    .skip(offset)
                    .map(|(owner, index, row)| {
                        purchase_row(
                            build,
                            creature.derived(),
                            s::HdIndex::from_usize(owner).map_err(|_| invalid())?,
                            index,
                            row,
                        )
                        .map(|value| p::AdvancementEntry::Purchase { value })
                    });
                pack(basis, revision, query.clone(), total, rows, budget)
            }
            p::AdvancementQuery::Balances { .. } => {
                let source = balances(build)?;
                let rows = source
                    .iter()
                    .skip(offset)
                    .map(|(owner, row)| balance_row(build, *owner, row));
                pack(basis, revision, query.clone(), source.len(), rows, budget)
            }
            p::AdvancementQuery::Catalog { catalog, .. } => match catalog {
                p::AdvancementKind::SkillRank => pack(
                    basis,
                    revision,
                    query.clone(),
                    s::attributes::Skill::ALL.len(),
                    s::attributes::Skill::ALL
                        .into_iter()
                        .skip(offset)
                        .map(|value| {
                            Ok(p::AdvancementEntry::Catalog {
                                value: p::AdvancementCatalogRow {
                                    selection: p::PurchaseChoice::SkillRank {
                                        skill: value.as_str().into(),
                                    },
                                    name: skill(value).name,
                                    form_choices: Vec::new(),
                                },
                            })
                        }),
                    budget,
                ),
                p::AdvancementKind::Expertise => {
                    let source = &build.ruleset().document().expertises;
                    pack(
                        basis,
                        revision,
                        query.clone(),
                        source.len(),
                        source.iter().skip(offset).map(|value| {
                            Ok(p::AdvancementEntry::Catalog {
                                value: p::AdvancementCatalogRow {
                                    selection: p::PurchaseChoice::Expertise {
                                        expertise: value.id.to_string(),
                                    },
                                    name: value.name.clone(),
                                    form_choices: Vec::new(),
                                },
                            })
                        }),
                        budget,
                    )
                }
                p::AdvancementKind::Talent => {
                    let source = &build.ruleset().document().talents;
                    pack(
                        basis,
                        revision,
                        query.clone(),
                        source.len(),
                        source.iter().skip(offset).map(|value| {
                            Ok(p::AdvancementEntry::Catalog {
                                value: p::AdvancementCatalogRow {
                                    selection: p::PurchaseChoice::Talent {
                                        talent: value.id.to_string(),
                                    },
                                    name: value.name.clone(),
                                    form_choices: value
                                        .form_choices
                                        .iter()
                                        .map(|id| {
                                            let form =
                                                build.ruleset().form(id).ok_or_else(invalid)?;
                                            Ok(named(id, Some(&form.name)))
                                        })
                                        .collect::<Result<_, Failure>>()?,
                                },
                            })
                        }),
                        budget,
                    )
                }
            },
            p::AdvancementQuery::Preview { proposal, .. } => {
                let domain_basis = adapter::basis(&basis)?;
                let preview = match proposal {
                    p::AdvancementProposal::GrowthSource { source } => Preview::Growth(Box::new(
                        self.game
                            .preview_player_growth_source(
                                s::ActorId(actor.0),
                                &domain_basis,
                                &adapter::growth_source(source)?,
                            )
                            .map_err(|_| unavailable())?,
                    )),
                    p::AdvancementProposal::IntellectExpertise { choice } => {
                        Preview::IntellectExpertise(Box::new(
                            self.game
                                .preview_player_intellect_expertise(
                                    s::ActorId(actor.0),
                                    &domain_basis,
                                    &adapter::intellect_expertise(choice)?,
                                )
                                .map_err(|_| unavailable())?,
                        ))
                    }
                    p::AdvancementProposal::Purchase { purchase } => Preview::Purchase(Box::new(
                        self.game
                            .preview_player_purchase(
                                s::ActorId(actor.0),
                                &domain_basis,
                                &adapter::purchase(purchase)?,
                            )
                            .map_err(|_| unavailable())?,
                    )),
                    p::AdvancementProposal::Replacement { replacement } => {
                        Preview::Replacement(Box::new(
                            self.game
                                .preview_player_replacement(
                                    s::ActorId(actor.0),
                                    &domain_basis,
                                    &adapter::replacement(replacement)?,
                                )
                                .map_err(|_| unavailable())?,
                        ))
                    }
                };
                preview.page(basis, revision, query, &creature, budget)
            }
        }
    }
    fn advancement_overview(
        &self,
        actor: ActorId,
        creature: &s::creatures::CreatureState,
    ) -> Result<p::AdvancementOverview, Failure> {
        let build = creature.build();
        let rules = build.ruleset();
        let policy = &rules.document().advancement;
        let ration = &policy.respec_ration;
        let mut earned = 0u64;
        let mut spent = 0u64;
        for (_, row) in balances(build)? {
            earned = earned
                .checked_add(u64::from(row.granted))
                .ok_or_else(invalid)?;
            spent = spent
                .checked_add(u64::from(row.spent))
                .ok_or_else(invalid)?;
        }
        let pending = self.game.preparation(s::ActorId(actor.0));
        let locked = !self.game.scenario_capabilities().owner_advancement;
        let timing = self
            .game
            .preparation_timing(s::ActorId(actor.0))
            .ok_or_else(unavailable)?;
        let respec = match pending.map(|value| (&value.plan, value)) {
            Some((s::abilities::Plan::Respec(plan), pending)) => {
                let intention = pending.intention.ok_or_else(invalid)?;
                let source = self
                    .history_index
                    .intention_admission(intention)
                    .ok_or_else(invalid)?;
                let admission = &self.archive.records[source].entry;
                if admission.actor != actor {
                    return Err(invalid());
                }
                Some(p::OwnerRespecProgress {
                    intention: p::IntentionId(admission.id.0.clone()),
                    basis: adapter::wire_basis(&plan.basis),
                    replacement: adapter::wire_replacement(&plan.replacement),
                    active: pending.active,
                    total_ticks: plan.preparation,
                    remaining_ticks: timing.preparation_remaining.ok_or_else(invalid)?,
                })
            }
            _ => None,
        };
        Ok(p::AdvancementOverview {
            strength_acquired: build.strength_acquired(),
            starting_skills: build.starting_skills().map(skill),
            growth: p::AdvancementGrowth {
                body_package_hit_dice: build
                    .active_body_package()
                    .map(|package| u64::try_from(package.advancement.len()).map_err(|_| invalid()))
                    .transpose()?,
                highest_applied_tier: creature.earned_progress().highest_applied_tier().get(),
                restorable_hit_dice: creature.restorable_hit_dice().get(),
                restoration_blocker: creature.hit_die_restoration_blocker().map(|blocker| {
                    match blocker {
                        s::creatures::HitDieRestorationBlocker::MissingSource => {
                            p::HitDieRestorationBlocker::MissingSource {}
                        }
                        s::creatures::HitDieRestorationBlocker::MissingHealthRoot => {
                            p::HitDieRestorationBlocker::MissingHealthRoot {}
                        }
                    }
                }),
                highest_earned_tier: creature.earned_progress().highest_earned_tier().get(),
                peak_threat: creature.earned_progress().peak_threat(),
                held_xp: creature.earned_progress().held_xp(),
                earned_xp: creature.earned_progress().earned_xp(),
                spent_xp: creature.earned_progress().spent_xp(),
                next_threshold: creature
                    .earned_progress()
                    .next_threshold(&rules.document().growth)
                    .map_err(|_| invalid())?,
                future: adapter::wire_future_growth(build.future_growth()),
            },
            points: p::AdvancementPoints {
                earned,
                spent,
                unspent: earned.checked_sub(spent).ok_or_else(invalid)?,
            },
            timings: p::AdvancementTimings {
                purchase_recovery: creature
                    .derived()
                    .action_duration
                    .apply_bounded(policy.purchase_recovery)
                    .map_err(|_| invalid())?,
                respec_prepare: creature
                    .derived()
                    .action_duration
                    .apply_bounded(policy.respec_prepare)
                    .map_err(|_| invalid())?,
                respec_recovery: creature
                    .derived()
                    .action_duration
                    .apply_bounded(policy.respec_recovery)
                    .map_err(|_| invalid())?,
                received_loss_interval: policy.received_loss_interval,
                provisional: true,
            },
            ration: p::AdvancementRation {
                supply: named(
                    &ration.supply,
                    Some(&rules.supply(&ration.supply).ok_or_else(invalid)?.name),
                ),
                quantity: ration.quantity,
                access: match ration.access {
                    s::rules::SupplyAccess::CarriedOnly => p::SupplyAccess::CarriedOnly,
                    s::rules::SupplyAccess::CarriedOrGroundTouch => {
                        p::SupplyAccess::CarriedOrGroundTouch
                    }
                },
            },
            received_loss_remaining: creature.received_health_loss_remaining(),
            perceived_threat: self.game.has_perceived_hostile(s::ActorId(actor.0)),
            ready: !locked
                && self.game.player_advancement_ready(s::ActorId(actor.0))
                && self.input_readiness(actor).admission,
            locked,
            other_preparation: pending.is_some() && respec.is_none(),
            respec,
        })
    }
}

enum Preview {
    IntellectExpertise(Box<s::player_advancement::PlayerIntellectExpertisePreview>),
    Growth(Box<s::player_advancement::PlayerGrowthPreview>),
    Purchase(Box<s::player_advancement::PlayerPurchasePreview>),
    Replacement(Box<s::player_respec::PlayerReplacementPreview>),
}
impl Preview {
    fn build(&self) -> &CreatureBuild {
        match self {
            Self::IntellectExpertise(value) => value.build(),
            Self::Growth(value) => value.build(),
            Self::Purchase(value) => value.build(),
            Self::Replacement(value) => value.permanent().build(),
        }
    }
    fn derived(&self) -> &DerivedCreature {
        match self {
            Self::IntellectExpertise(value) => value.derived(),
            Self::Growth(value) => value.derived(),
            Self::Purchase(value) => value.derived(),
            Self::Replacement(value) => value.permanent().derived(),
        }
    }
    fn runtime(&self) -> &s::player_advancement::AdvancementConsequences {
        match self {
            Self::IntellectExpertise(value) => value.runtime(),
            Self::Growth(value) => value.runtime(),
            Self::Purchase(value) => value.runtime(),
            Self::Replacement(value) => value.runtime(),
        }
    }
    fn removed(&self) -> &[s::creatures::RemovedPurchase] {
        match self {
            Self::IntellectExpertise(_) | Self::Growth(_) | Self::Purchase(_) => &[],
            Self::Replacement(value) => value.permanent().removed(),
        }
    }
    fn refunds(&self) -> &[s::creatures::PointRefund] {
        match self {
            Self::IntellectExpertise(_) | Self::Growth(_) | Self::Purchase(_) => &[],
            Self::Replacement(value) => value.permanent().refunds(),
        }
    }
    fn page(
        &self,
        basis: p::AdvancementBasis,
        revision: u64,
        query: &p::AdvancementQuery,
        before: &s::creatures::CreatureState,
        budget: &PageBudget<'_>,
    ) -> Result<p::AdvancementPage, Failure> {
        let p::AdvancementQuery::Preview {
            proposal, section, ..
        } = query
        else {
            return Err(invalid());
        };
        let offset = usize::from(query.offset());
        let build = self.build();
        let derived = self.derived();
        match section {
            p::AdvancementPreviewSection::Summary => pack(
                basis,
                revision,
                query.clone(),
                1,
                [Ok(p::AdvancementEntry::Summary {
                    value: self.summary(proposal)?,
                })],
                budget,
            ),
            p::AdvancementPreviewSection::Removed => pack(
                basis,
                revision,
                query.clone(),
                self.removed().len(),
                self.removed().iter().skip(offset).map(|row| {
                    purchase_row(
                        before.build(),
                        before.derived(),
                        row.location.owner,
                        row.location.index,
                        &row.purchase,
                    )
                    .map(|value| p::AdvancementEntry::Removed { value })
                }),
                budget,
            ),
            p::AdvancementPreviewSection::Refunds => pack(
                basis,
                revision,
                query.clone(),
                self.refunds().len(),
                self.refunds().iter().skip(offset).map(|row| {
                    Ok(p::AdvancementEntry::Refund {
                        value: p::AdvancementRefundRow {
                            owner: row.owner.get(),
                            pool: pool(build, &row.pool)?,
                            points: row.points,
                        },
                    })
                }),
                budget,
            ),
            p::AdvancementPreviewSection::Balances => {
                let source = balances(build)?;
                pack(
                    basis,
                    revision,
                    query.clone(),
                    source.len(),
                    source
                        .iter()
                        .skip(offset)
                        .map(|(owner, row)| balance_row(build, *owner, row)),
                    budget,
                )
            }
            p::AdvancementPreviewSection::Skills => pack(
                basis,
                revision,
                query.clone(),
                s::attributes::Skill::ALL.len(),
                s::attributes::Skill::ALL
                    .into_iter()
                    .skip(offset)
                    .map(|value| {
                        Ok(p::AdvancementEntry::Skill {
                            value: p::AdvancementSkillRow {
                                skill: skill(value),
                                rank: derived.skills.get(value),
                            },
                        })
                    }),
                budget,
            ),
            p::AdvancementPreviewSection::Expertises => {
                let rows =
                    crate::creature_view::expertise_rows(&derived.expertises, build.ruleset());
                pack(
                    basis,
                    revision,
                    query.clone(),
                    rows.len(),
                    rows.into_iter()
                        .skip(offset)
                        .map(|value| Ok(p::AdvancementEntry::Expertise { value })),
                    budget,
                )
            }
            p::AdvancementPreviewSection::Talents => {
                let total = derived.active_talents.len() + derived.dormant_talents.len();
                let rows = build
                    .choices()
                    .iter()
                    .flat_map(|choice| choice.talents())
                    .skip(offset)
                    .map(|id| {
                        let definition = build.ruleset().talent(id).ok_or_else(invalid)?;
                        Ok(p::AdvancementEntry::Talent {
                            value: p::AdvancementTalentRow {
                                talent: named(id, Some(&definition.name)),
                                status: if derived.active_talents.contains(id) {
                                    p::AdvancementStatus::Active
                                } else {
                                    p::AdvancementStatus::Dormant
                                },
                            },
                        })
                    });
                pack(basis, revision, query.clone(), total, rows, budget)
            }
            p::AdvancementPreviewSection::RemovedConditions => {
                let source = &self.runtime().removed_conditions;
                pack(
                    basis,
                    revision,
                    query.clone(),
                    source.len(),
                    source.iter().skip(offset).map(|row| {
                        let definition = build
                            .ruleset()
                            .condition(&row.condition)
                            .ok_or_else(invalid)?;
                        Ok(p::AdvancementEntry::RemovedCondition {
                            value: p::AdvancementRemovedConditionRow {
                                condition: named(&row.condition, Some(&definition.name)),
                                applications: row.applications,
                                stacks: row.stacks,
                            },
                        })
                    }),
                    budget,
                )
            }
        }
    }
    fn summary(&self, proposal: &p::AdvancementProposal) -> Result<p::AdvancementSummary, Failure> {
        let runtime = self.runtime();
        let resource =
            |id, value: s::player_advancement::AdvancementResource| p::AdvancementResourceRow {
                resource: named(id, None),
                balance: value.balance,
                maximum: value.maximum,
            };
        Ok(p::AdvancementSummary {
            strength_acquired: self.build().strength_acquired(),
            proposal: proposal.clone(),
            resulting_build_epoch: runtime.build_epoch.get(),
            removed_count: small(self.removed().len())?,
            refund_count: small(self.refunds().len())?,
            removed_condition_count: small(runtime.removed_conditions.len())?,
            health: p::AdvancementHealth {
                current: runtime.health.current(),
                maximum: runtime.health.maximum(),
                injury: runtime.health.injury(),
                dead: runtime.health.dead(),
            },
            health_recovery: p::AdvancementHealthRecovery {
                period_ticks: runtime.health_recovery.period(),
                elapsed_ticks: runtime.health_recovery.elapsed(),
                stopped: runtime.health_recovery.stopped(),
            },
            resources: p::AdvancementResources {
                stamina: resource("stamina", runtime.resources.stamina),
                focus: resource("focus", runtime.resources.focus),
                mana: resource("mana", runtime.resources.mana),
            },
            canceled_resource_holds: small(runtime.canceled_resource_holds)?,
        })
    }
}

#[cfg(test)]
#[path = "advancement_engine_tests.rs"]
mod tests;

#[cfg(test)]
mod owned_growth_disclosure_tests {
    use super::*;

    #[test]
    fn body_only_overview_keeps_authored_capacity_after_real_body_drain() {
        let engine = Engine::memory(crate::advancement_test_support::scenario()).unwrap();
        let original = engine.game.creature(s::ActorId(1)).unwrap();
        let build = original.build();
        let package = std::sync::Arc::new(s::creatures::BodyPackage {
            id: s::content_ids::BodyFormId::new("overview_body").unwrap(),
            species: build.species_id().clone(),
            body: s::BodySpec::default(),
            advancement: build
                .ledger()
                .entries()
                .iter()
                .zip(build.choices())
                .map(|(die, choices)| s::creatures::PlannedAdvancement {
                    source: die.source(),
                    choices: choices.clone(),
                })
                .collect(),
        });
        let owned = build
            .with_body_package(package, build.personal_health_root_seed().unwrap())
            .unwrap();
        let mut creature = s::creatures::CreatureState::new(owned).unwrap();
        for loss in [0, 1] {
            if loss != 0 {
                assert_eq!(creature.drain_latest_hit_die().unwrap().applied, 1);
            }
            let overview = engine.advancement_overview(ActorId(1), &creature).unwrap();
            assert_eq!(overview.growth.body_package_hit_dice, Some(2));
            assert_eq!(overview.growth.highest_applied_tier, 0);
            assert_eq!(overview.growth.highest_earned_tier, 0);
            assert_eq!(overview.growth.restorable_hit_dice, loss);
            assert_eq!(creature.build().ledger().total_hd().get(), 2 - loss);
            let decoded: p::AdvancementGrowth =
                serde_json::from_value(serde_json::to_value(&overview.growth).unwrap()).unwrap();
            assert_eq!(decoded, overview.growth);
        }
    }
}
