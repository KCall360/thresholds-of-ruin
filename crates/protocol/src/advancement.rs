//! Owner-only descriptive advancement pages and exact action operands.
//! These records carry no candidate build, refund authority or supply quantities.
use crate::{ActorId, AdvancementKind, BranchId, IntentionId, StreamId, SupplyAccess};
use serde::{Deserialize, Serialize};

pub const MAX_ADVANCEMENT_PAGE_ENTRIES: usize = 32;
pub const MAX_ADVANCEMENT_PAGE_BYTES: usize = 32 * 1024;
/// Conservative capacity; two free starting ranks make the attainable ledger 354.
pub const MAX_ADVANCEMENT_LEDGER_ROWS: usize = 356;
pub const MAX_ADVANCEMENT_BALANCE_ROWS: usize = 4096;
pub const MAX_ADVANCEMENT_CATALOG_ROWS: usize = 4096;
pub const MAX_ADVANCEMENT_FORM_OPTIONS: usize = 16;
pub const MAX_ADVANCEMENT_SKILL_ROWS: usize = 20;
pub const MAX_ADVANCEMENT_TALENT_ROWS: usize = 256;
pub const MAX_ADVANCEMENT_EXPERTISE_ROWS: usize = 256;
pub const MAX_ADVANCEMENT_INTELLECT_EXPERTISE_ROWS: usize = 1000;
pub const MAX_EXPERTISE_ORIGINS: usize = 257;
pub const MAX_ADVANCEMENT_CONDITION_ROWS: usize = 128;

trait Checked {
    fn checked(&self) -> bool;
}
// Keep the named reader and public serializer's fields in one declaration. All
// nullable keys remain required through explicit required_option annotations.
macro_rules! record {
    ($name:ident { $($(#[$attribute:meta])* $field:ident: $ty:ty),* $(,)? }) => {
        #[derive(Clone, Debug, PartialEq, Eq, Serialize)]
        pub struct $name { $($(#[$attribute])* pub $field: $ty,)* }
        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Fields { $($(#[$attribute])* $field: $ty,)* }
                let f: Fields = crate::bounded::named_map(d)?;
                let value = Self { $($field: f.$field,)* };
                if value.checked() { Ok(value) }
                else { Err(serde::de::Error::custom(concat!("invalid ", stringify!($name)))) }
            }
        }
    };
}
macro_rules! tagged {
    ($name:ident { $($variant:ident { $($(#[$attribute:meta])* $field:ident: $ty:ty),* $(,)? }),* $(,)? }) => {
        #[derive(Clone, Debug, PartialEq, Eq, Serialize)]
        #[serde(tag = "kind", rename_all = "snake_case")]
        pub enum $name { $($variant { $($(#[$attribute])* $field: $ty,)* },)* }
        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                #[derive(Deserialize)]
                #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
                enum Fields { $($variant { $($(#[$attribute])* $field: $ty,)* },)* }
                let value = match crate::bounded::named_map(d)? {
                    $(Fields::$variant { $($field,)* } => Self::$variant { $($field,)* },)*
                };
                if value.checked() { Ok(value) }
                else { Err(serde::de::Error::custom(concat!("invalid ", stringify!($name)))) }
            }
        }
    };
}
fn required_option<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> Result<Option<T>, D::Error> {
    Option::deserialize(d)
}
fn entries<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<AdvancementEntry>, D::Error> {
    crate::bounded::bounded::<D, _, MAX_ADVANCEMENT_PAGE_ENTRIES>(d)
}
fn form_options<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Vec<AdvancementContentRef>, D::Error> {
    crate::bounded::bounded::<D, _, MAX_ADVANCEMENT_FORM_OPTIONS>(d)
}
fn kinds<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<AdvancementKind>, D::Error> {
    crate::bounded::bounded::<D, _, 3>(d)
}
fn label(value: &str, maximum: usize) -> bool {
    crate::creature::valid_label(value, maximum)
}

record!(AdvancementContentRef {
    id: String,
    name: String
});
record!(AdvancementBasis {
    actor: ActorId,
    branch: BranchId,
    stream: StreamId,
    #[serde(with = "crate::integers::unsigned")]
    reset: u64,
    #[serde(with = "crate::integers::unsigned")]
    build_epoch: u64,
});
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdvancementStatus {
    Active,
    Dormant,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdvancementPreviewSection {
    Summary,
    Removed,
    Refunds,
    Balances,
    Skills,
    Talents,
    Expertises,
    RemovedConditions,
}

tagged!(PurchaseChoice { SkillRank { skill: String }, Talent { talent: String }, TalentForm { talent: String, form: String }, Expertise { expertise: String } });
record!(IntellectExpertiseOperation {
    threshold: u16,
    expertise: String
});
record!(PurchaseOperation {
    #[serde(with = "crate::integers::unsigned")]
    owner: u64,
    pool: String,
    selection: PurchaseChoice
});
record!(ReplacementSource {
    #[serde(with = "crate::integers::unsigned")]
    owner: u64,
    index: u16
});
record!(ReplacementPurchase {
    pool: String,
    selection: PurchaseChoice
});
record!(ReplacementOperation {
    source: ReplacementSource,
    #[serde(deserialize_with = "required_option")]
    replacement: Option<ReplacementPurchase>,
});
tagged!(GrowthSource { Racial {}, Class { class: String } });
tagged!(FutureGrowth { Disabled {}, Selected { source: GrowthSource }, Authored {} });
tagged!(HitDieRestorationBlocker { MissingSource {}, MissingHealthRoot {} });
record!(AdvancementGrowth {
    // Full authored active-package capacity, not surviving body rows. Required
    // null identifies the ordinary unowned advancement domain.
    #[serde(with = "crate::integers::optional_unsigned")]
    body_package_hit_dice: Option<u64>,
    #[serde(with = "crate::integers::unsigned")]
    highest_applied_tier: u64,
    #[serde(with = "crate::integers::unsigned")]
    restorable_hit_dice: u64,
    #[serde(deserialize_with = "required_option")]
    restoration_blocker: Option<HitDieRestorationBlocker>,
    #[serde(with = "crate::integers::unsigned")]
    highest_earned_tier: u64,
    #[serde(with = "crate::integers::unsigned")]
    peak_threat: u64,
    #[serde(with = "crate::integers::unsigned")]
    held_xp: u64,
    #[serde(with = "crate::integers::unsigned")]
    earned_xp: u64,
    #[serde(with = "crate::integers::unsigned")]
    spent_xp: u64,
    #[serde(with = "crate::integers::unsigned")]
    next_threshold: u64,
    future: FutureGrowth,
});
tagged!(AdvancementProposal {
    IntellectExpertise { choice: IntellectExpertiseOperation },
    GrowthSource { source: GrowthSource },
    Purchase { purchase: PurchaseOperation },
    Replacement { replacement: ReplacementOperation },
});
tagged!(AdvancementQuery {
    Overview {},
    Ledger { basis: AdvancementBasis, offset: u16 },
    IntellectExpertises { basis: AdvancementBasis, offset: u16 },
    Balances { basis: AdvancementBasis, offset: u16 },
    Catalog { basis: AdvancementBasis, catalog: AdvancementKind, offset: u16 },
    Preview { basis: AdvancementBasis, proposal: AdvancementProposal, section: AdvancementPreviewSection, offset: u16 },
});
record!(AdvancementPage {
    basis: AdvancementBasis,
    #[serde(with = "crate::integers::unsigned")]
    revision: u64,
    query: AdvancementQuery,
    total: u16,
    #[serde(deserialize_with = "required_option")]
    next: Option<u16>,
    #[serde(deserialize_with = "entries")]
    entries: Vec<AdvancementEntry>,
});
tagged!(AdvancementEntry {
    Overview { value: Box<AdvancementOverview> },
    Purchase { value: AdvancementLedgerRow },
    Balance { value: AdvancementBalanceRow },
    Catalog { value: AdvancementCatalogRow },
    Summary { value: AdvancementSummary },
    Removed { value: AdvancementLedgerRow },
    Refund { value: AdvancementRefundRow },
    Skill { value: AdvancementSkillRow },
    Talent { value: AdvancementTalentRow },
    Expertise { value: AdvancementExpertiseRow },
    IntellectExpertise { value: AdvancementIntellectExpertiseRow },
    RemovedCondition { value: AdvancementRemovedConditionRow },
});
record!(AdvancementPoints {
    #[serde(with = "crate::integers::unsigned")]
    earned: u64,
    #[serde(with = "crate::integers::unsigned")]
    spent: u64,
    #[serde(with = "crate::integers::unsigned")]
    unspent: u64
});
record!(AdvancementTimings {
    #[serde(with = "crate::integers::unsigned")]
    purchase_recovery: u64,
    #[serde(with = "crate::integers::unsigned")]
    respec_prepare: u64,
    #[serde(with = "crate::integers::unsigned")]
    respec_recovery: u64,
    #[serde(with = "crate::integers::unsigned")]
    received_loss_interval: u64,
    provisional: bool,
});
record!(AdvancementRation {
    supply: AdvancementContentRef,
    #[serde(with = "crate::integers::unsigned")]
    quantity: u64,
    access: SupplyAccess,
});
record!(AdvancementOverview {
    strength_acquired: u8,
    starting_skills: [AdvancementContentRef; 2], points: AdvancementPoints,
    timings: AdvancementTimings, ration: AdvancementRation, growth: AdvancementGrowth,
    #[serde(with = "crate::integers::unsigned")]
    received_loss_remaining: u64,
    perceived_threat: bool, ready: bool, other_preparation: bool,
    #[serde(deserialize_with = "required_option")]
    respec: Option<OwnerRespecProgress>,
});
record!(OwnerRespecProgress {
    intention: IntentionId,
    basis: AdvancementBasis,
    replacement: ReplacementOperation,
    active: bool,
    #[serde(with = "crate::integers::unsigned")]
    total_ticks: u64,
    #[serde(with = "crate::integers::unsigned")]
    remaining_ticks: u64,
});
record!(AdvancementLedgerRow {
    source: ReplacementSource,
    pool: AdvancementContentRef,
    selection: PurchaseChoice,
    name: String,
    status: AdvancementStatus,
});
record!(AdvancementBalanceRow {
    #[serde(with = "crate::integers::unsigned")]
    owner: u64, pool: AdvancementContentRef, granted: u16, spent: u16, unspent: u16,
    #[serde(deserialize_with = "kinds")]
    allowed: Vec<AdvancementKind>,
});
record!(AdvancementCatalogRow {
    selection: PurchaseChoice,
    name: String,
    #[serde(deserialize_with = "form_options")]
    form_choices: Vec<AdvancementContentRef>,
});
record!(AdvancementRefundRow {
    #[serde(with = "crate::integers::unsigned")]
    owner: u64,
    pool: AdvancementContentRef,
    points: u16
});
record!(AdvancementSkillRow {
    skill: AdvancementContentRef,
    rank: u8
});
record!(AdvancementTalentRow {
    talent: AdvancementContentRef,
    status: AdvancementStatus
});
record!(AdvancementIntellectExpertiseRow {
    threshold: u16,
    status: AdvancementStatus,
    #[serde(deserialize_with = "required_option")]
    expertise: Option<AdvancementContentRef>,
});
record!(AdvancementExpertiseRow {
    expertise: AdvancementContentRef,
    status: AdvancementStatus,
    source_count: u16,
});
record!(AdvancementRemovedConditionRow {
    condition: AdvancementContentRef,
    applications: u16,
    stacks: u32
});
record!(AdvancementHealth {
    #[serde(with = "crate::integers::unsigned")]
    current: u64,
    #[serde(with = "crate::integers::unsigned")]
    maximum: u64,
    #[serde(with = "crate::integers::unsigned")]
    injury: u64,
    dead: bool
});
record!(AdvancementHealthRecovery {
    #[serde(with = "crate::integers::unsigned")]
    period_ticks: u64,
    #[serde(with = "crate::integers::unsigned")]
    elapsed_ticks: u64,
    stopped: bool,
});
record!(AdvancementResourceRow {
    resource: AdvancementContentRef,
    balance: u32,
    maximum: u32
});
record!(AdvancementResources {
    stamina: AdvancementResourceRow,
    focus: AdvancementResourceRow,
    mana: AdvancementResourceRow
});
record!(AdvancementSummary {
    strength_acquired: u8,
    proposal: AdvancementProposal,
    #[serde(with = "crate::integers::unsigned")]
    resulting_build_epoch: u64,
    removed_count: u16,
    refund_count: u16,
    removed_condition_count: u16,
    health: AdvancementHealth,
    health_recovery: AdvancementHealthRecovery,
    resources: AdvancementResources,
    canceled_resource_holds: u16,
});

impl Checked for AdvancementContentRef {
    fn checked(&self) -> bool {
        crate::valid_content_id(&self.id) && label(&self.name, 256)
    }
}
impl Checked for AdvancementBasis {
    fn checked(&self) -> bool {
        self.actor.0 > 0
            && self.actor.0 < u64::MAX
            && self.build_epoch > 0
            && label(&self.branch.0, 128)
            && label(&self.stream.0, 128)
    }
}
impl PurchaseChoice {
    pub fn kind(&self) -> AdvancementKind {
        match self {
            Self::SkillRank { .. } => AdvancementKind::SkillRank,
            Self::Expertise { .. } => AdvancementKind::Expertise,
            Self::Talent { .. } | Self::TalentForm { .. } => AdvancementKind::Talent,
        }
    }
}
impl Checked for PurchaseChoice {
    fn checked(&self) -> bool {
        match self {
            Self::SkillRank { skill } => crate::valid_content_id(skill),
            Self::Expertise { expertise } => crate::valid_content_id(expertise),
            Self::Talent { talent } => crate::valid_content_id(talent),
            Self::TalentForm { talent, form } => {
                crate::valid_content_id(talent) && crate::valid_content_id(form)
            }
        }
    }
}
impl Checked for PurchaseOperation {
    fn checked(&self) -> bool {
        self.owner < u64::MAX && crate::valid_content_id(&self.pool) && self.selection.checked()
    }
}
impl Checked for ReplacementSource {
    fn checked(&self) -> bool {
        self.owner < u64::MAX && self.index < 256
    }
}
impl Checked for ReplacementPurchase {
    fn checked(&self) -> bool {
        crate::valid_content_id(&self.pool) && self.selection.checked()
    }
}
impl Checked for ReplacementOperation {
    fn checked(&self) -> bool {
        self.source.checked() && self.replacement.as_ref().is_none_or(Checked::checked)
    }
}
impl Checked for GrowthSource {
    fn checked(&self) -> bool {
        match self {
            Self::Racial {} => true,
            Self::Class { class } => crate::valid_content_id(class),
        }
    }
}
impl Checked for FutureGrowth {
    fn checked(&self) -> bool {
        match self {
            Self::Disabled {} | Self::Authored {} => true,
            Self::Selected { source } => source.checked(),
        }
    }
}
impl Checked for HitDieRestorationBlocker {
    fn checked(&self) -> bool {
        true
    }
}
impl Checked for AdvancementGrowth {
    fn checked(&self) -> bool {
        let loss_ceiling = match self.body_package_hit_dice {
            None if self.highest_applied_tier > 0 => Some(self.highest_applied_tier),
            Some(body) if (1..=256).contains(&body) => self.highest_applied_tier.checked_add(body),
            _ => None,
        };
        self.highest_applied_tier <= self.highest_earned_tier
            && loss_ceiling.is_some_and(|maximum| self.restorable_hit_dice <= maximum)
            && (self.restoration_blocker.is_none() || self.restorable_hit_dice > 0)
            && self.peak_threat > 0
            && self.next_threshold > 0
            && self.held_xp.checked_add(self.spent_xp) == Some(self.earned_xp)
            && self.future.checked()
    }
}
impl Checked for AdvancementProposal {
    fn checked(&self) -> bool {
        match self {
            Self::GrowthSource { source } => source.checked(),
            Self::IntellectExpertise { choice } => choice.checked(),
            Self::Purchase { purchase } => purchase.checked(),
            Self::Replacement { replacement } => replacement.checked(),
        }
    }
}
impl AdvancementQuery {
    pub fn basis(&self) -> Option<&AdvancementBasis> {
        match self {
            Self::Overview {} => None,
            Self::Ledger { basis, .. }
            | Self::IntellectExpertises { basis, .. }
            | Self::Balances { basis, .. }
            | Self::Catalog { basis, .. }
            | Self::Preview { basis, .. } => Some(basis),
        }
    }
    pub fn offset(&self) -> u16 {
        match self {
            Self::Overview {} => 0,
            Self::Ledger { offset, .. }
            | Self::IntellectExpertises { offset, .. }
            | Self::Balances { offset, .. }
            | Self::Catalog { offset, .. }
            | Self::Preview { offset, .. } => *offset,
        }
    }
    /// A bound, never a promise that a given authoring catalog reaches it.
    pub fn row_limit(&self) -> usize {
        match self {
            Self::IntellectExpertises { .. } => MAX_ADVANCEMENT_INTELLECT_EXPERTISE_ROWS,
            Self::Overview {}
            | Self::Preview {
                section: AdvancementPreviewSection::Summary,
                ..
            } => 1,
            Self::Ledger { .. }
            | Self::Preview {
                section: AdvancementPreviewSection::Removed | AdvancementPreviewSection::Refunds,
                ..
            } => MAX_ADVANCEMENT_LEDGER_ROWS,
            Self::Balances { .. }
            | Self::Preview {
                section: AdvancementPreviewSection::Balances,
                ..
            } => MAX_ADVANCEMENT_BALANCE_ROWS,
            Self::Catalog {
                catalog: AdvancementKind::SkillRank,
                ..
            }
            | Self::Preview {
                section: AdvancementPreviewSection::Skills,
                ..
            } => MAX_ADVANCEMENT_SKILL_ROWS,
            Self::Catalog {
                catalog: AdvancementKind::Talent | AdvancementKind::Expertise,
                ..
            } => MAX_ADVANCEMENT_CATALOG_ROWS,
            Self::Preview {
                section: AdvancementPreviewSection::Expertises,
                ..
            } => MAX_ADVANCEMENT_EXPERTISE_ROWS,
            Self::Preview {
                section: AdvancementPreviewSection::Talents,
                ..
            } => MAX_ADVANCEMENT_TALENT_ROWS,
            Self::Preview {
                section: AdvancementPreviewSection::RemovedConditions,
                ..
            } => MAX_ADVANCEMENT_CONDITION_ROWS,
        }
    }
    fn accepts(&self, entry: &AdvancementEntry) -> bool {
        use AdvancementEntry as E;
        use AdvancementPreviewSection as S;
        match (self, entry) {
            (Self::Overview {}, E::Overview { .. })
            | (Self::Ledger { .. }, E::Purchase { .. })
            | (Self::IntellectExpertises { .. }, E::IntellectExpertise { .. })
            | (
                Self::Balances { .. }
                | Self::Preview {
                    section: S::Balances,
                    ..
                },
                E::Balance { .. },
            )
            | (
                Self::Preview {
                    section: S::Removed,
                    ..
                },
                E::Removed { .. },
            )
            | (
                Self::Preview {
                    section: S::Refunds,
                    ..
                },
                E::Refund { .. },
            )
            | (
                Self::Preview {
                    section: S::Skills, ..
                },
                E::Skill { .. },
            )
            | (
                Self::Preview {
                    section: S::Expertises,
                    ..
                },
                E::Expertise { .. },
            )
            | (
                Self::Preview {
                    section: S::Talents,
                    ..
                },
                E::Talent { .. },
            )
            | (
                Self::Preview {
                    section: S::RemovedConditions,
                    ..
                },
                E::RemovedCondition { .. },
            ) => true,
            (Self::Catalog { catalog, .. }, E::Catalog { value }) => {
                *catalog == value.selection.kind()
            }
            (
                Self::Preview {
                    proposal,
                    section: S::Summary,
                    ..
                },
                E::Summary { value },
            ) => *proposal == value.proposal,
            _ => false,
        }
    }
}
impl Checked for AdvancementQuery {
    fn checked(&self) -> bool {
        self.basis().is_none_or(Checked::checked)
            && usize::from(self.offset()) < self.row_limit()
            && match self {
                Self::Preview { proposal, .. } => proposal.checked(),
                _ => true,
            }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidAdvancementPage;
impl AdvancementPage {
    pub fn validate(&self) -> Result<(), InvalidAdvancementPage> {
        self.checked().then_some(()).ok_or(InvalidAdvancementPage)
    }
}
impl Checked for AdvancementPage {
    fn checked(&self) -> bool {
        let start = usize::from(self.query.offset());
        let total = usize::from(self.total);
        let Some(end) = start.checked_add(self.entries.len()) else {
            return false;
        };
        if !self.basis.checked()
            || !self.query.checked()
            || self.query.basis().is_some_and(|basis| *basis != self.basis)
            || total > self.query.row_limit()
            || self.entries.len() > MAX_ADVANCEMENT_PAGE_ENTRIES
            || end > total
            || (total == 0 && (start != 0 || self.next.is_some()))
            || (total > 0 && (start >= total || self.entries.is_empty()))
            || (self.query.row_limit() == 1 && total != 1)
            || self.next
                != (end < total).then(|| u16::try_from(end).expect("bounded advancement total"))
        {
            return false;
        }
        self.entries.iter().all(|entry| {
            entry.checked()
                && self.query.accepts(entry)
                && match entry {
                    AdvancementEntry::Summary { value } => {
                        self.basis.build_epoch.checked_add(1) == Some(value.resulting_build_epoch)
                    }
                    AdvancementEntry::Overview { value } => {
                        value.respec.as_ref().is_none_or(|progress| {
                            progress.basis.actor == self.basis.actor
                                && progress.basis.build_epoch == self.basis.build_epoch
                        })
                    }
                    _ => true,
                }
        })
    }
}
impl Checked for AdvancementEntry {
    fn checked(&self) -> bool {
        match self {
            Self::Overview { value } => value.checked(),
            Self::Purchase { value } | Self::Removed { value } => value.checked(),
            Self::Balance { value } => value.checked(),
            Self::Catalog { value } => value.checked(),
            Self::Summary { value } => value.checked(),
            Self::Refund { value } => value.checked(),
            Self::Skill { value } => value.checked(),
            Self::Talent { value } => value.checked(),
            Self::Expertise { value } => value.checked(),
            Self::IntellectExpertise { value } => value.checked(),
            Self::RemovedCondition { value } => value.checked(),
        }
    }
}
impl Checked for AdvancementPoints {
    fn checked(&self) -> bool {
        self.spent.checked_add(self.unspent) == Some(self.earned)
    }
}
impl Checked for AdvancementTimings {
    fn checked(&self) -> bool {
        self.purchase_recovery <= 1_000_000
            && self.respec_recovery <= 1_000_000
            && (1..=1_000_000).contains(&self.respec_prepare)
            && (1..=1_000_000).contains(&self.received_loss_interval)
    }
}
impl Checked for AdvancementRation {
    fn checked(&self) -> bool {
        self.supply.checked() && self.quantity == 1
    }
}
impl Checked for AdvancementOverview {
    fn checked(&self) -> bool {
        self.strength_acquired <= 7
            && self.starting_skills.iter().all(Checked::checked)
            && self.starting_skills[0].id != self.starting_skills[1].id
            && self.points.checked()
            && self.growth.checked()
            && self.timings.checked()
            && self.ration.checked()
            && self.received_loss_remaining <= self.timings.received_loss_interval
            && self.respec.as_ref().is_none_or(Checked::checked)
            && !(self.other_preparation && self.respec.is_some())
    }
}
impl Checked for OwnerRespecProgress {
    fn checked(&self) -> bool {
        label(&self.intention.0, 64)
            && self.basis.checked()
            && self.replacement.checked()
            && (1..=1_000_000).contains(&self.total_ticks)
            && self.remaining_ticks <= self.total_ticks
    }
}
impl Checked for AdvancementLedgerRow {
    fn checked(&self) -> bool {
        self.source.checked()
            && self.pool.checked()
            && self.selection.checked()
            && label(&self.name, 256)
            && (self.selection.kind() != AdvancementKind::SkillRank
                || self.status == AdvancementStatus::Active)
    }
}
impl Checked for AdvancementBalanceRow {
    fn checked(&self) -> bool {
        self.owner < u64::MAX
            && self.pool.checked()
            && self.granted <= 256
            && self.spent.checked_add(self.unspent) == Some(self.granted)
            && (1..=3).contains(&self.allowed.len())
            && self
                .allowed
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                == self.allowed.len()
    }
}
impl Checked for AdvancementCatalogRow {
    fn checked(&self) -> bool {
        self.selection.checked()
            && label(&self.name, 256)
            && self.form_choices.len() <= MAX_ADVANCEMENT_FORM_OPTIONS
            && self.form_choices.iter().all(Checked::checked)
            && self
                .form_choices
                .iter()
                .map(|form| &form.id)
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                == self.form_choices.len()
            && match self.selection {
                PurchaseChoice::Talent { .. } => true,
                PurchaseChoice::SkillRank { .. } | PurchaseChoice::Expertise { .. } => {
                    self.form_choices.is_empty()
                }
                PurchaseChoice::TalentForm { .. } => false,
            }
    }
}
impl Checked for AdvancementRefundRow {
    fn checked(&self) -> bool {
        self.owner < u64::MAX && self.pool.checked() && (1..=256).contains(&self.points)
    }
}
impl Checked for AdvancementSkillRow {
    fn checked(&self) -> bool {
        self.skill.checked() && self.rank <= 5
    }
}
impl Checked for AdvancementTalentRow {
    fn checked(&self) -> bool {
        self.talent.checked()
    }
}
impl Checked for AdvancementRemovedConditionRow {
    fn checked(&self) -> bool {
        self.condition.checked()
            && (1..=128).contains(&self.applications)
            && self.stacks >= u32::from(self.applications)
    }
}
impl Checked for AdvancementHealth {
    fn checked(&self) -> bool {
        if self.dead {
            self.current == 0
        } else {
            self.injury < self.maximum
                && self.maximum.checked_sub(self.injury) == Some(self.current)
        }
    }
}
impl Checked for AdvancementHealthRecovery {
    fn checked(&self) -> bool {
        self.period_ticks > 0 && self.elapsed_ticks < self.period_ticks
    }
}
impl Checked for AdvancementResourceRow {
    fn checked(&self) -> bool {
        self.resource.checked() && self.balance <= self.maximum
    }
}
impl Checked for AdvancementResources {
    fn checked(&self) -> bool {
        self.stamina.checked()
            && self.focus.checked()
            && self.mana.checked()
            && self.stamina.resource.id == "stamina"
            && self.focus.resource.id == "focus"
            && self.mana.resource.id == "mana"
    }
}
impl Checked for AdvancementSummary {
    fn checked(&self) -> bool {
        self.strength_acquired <= 7
            && self.proposal.checked()
            && self.resulting_build_epoch > 0
            && usize::from(self.removed_count) <= MAX_ADVANCEMENT_LEDGER_ROWS
            && usize::from(self.refund_count) <= MAX_ADVANCEMENT_LEDGER_ROWS
            && usize::from(self.removed_condition_count) <= MAX_ADVANCEMENT_CONDITION_ROWS
            && self.health.checked()
            && self.health_recovery.checked()
            && self.resources.checked()
            && self.canceled_resource_holds <= 64
    }
}

pub(crate) fn valid_respec_selection(selection: &crate::SupplySelection) -> bool {
    match selection {
        crate::SupplySelection::Carried => true,
        crate::SupplySelection::Explicit { items } => {
            items.len() == 1 && items[0].cost_ordinal == 0
        }
    }
}
/// Narrow named payload readers retain the existing outer enum representations.
pub(crate) fn read_query_command<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<(u64, AdvancementQuery), D::Error> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Fields {
        #[serde(with = "crate::integers::unsigned")]
        expected_revision: u64,
        query: AdvancementQuery,
    }
    let f: Fields = crate::bounded::named_map(d)?;
    Ok((f.expected_revision, f.query))
}
pub(crate) fn read_page_reply<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<(crate::ReplyContext, String, Box<AdvancementPage>), D::Error> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Fields {
        context: crate::ReplyContext,
        request_id: String,
        page: Box<AdvancementPage>,
    }
    let f: Fields = crate::bounded::named_map(d)?;
    let basis = &f.page.basis;
    if !label(&f.request_id, 128)
        || f.context.actor != basis.actor
        || f.context.branch != basis.branch
        || f.context.input.stream.stream != basis.stream
        || f.context.input.stream.epoch != basis.reset
        || f.context.revision != f.page.revision
    {
        return Err(serde::de::Error::custom(
            "advancement reply context mismatch",
        ));
    }
    Ok((f.context, f.request_id, f.page))
}

impl Checked for IntellectExpertiseOperation {
    fn checked(&self) -> bool {
        (1..=MAX_ADVANCEMENT_INTELLECT_EXPERTISE_ROWS).contains(&usize::from(self.threshold))
            && crate::valid_content_id(&self.expertise)
    }
}
impl Checked for AdvancementIntellectExpertiseRow {
    fn checked(&self) -> bool {
        (1..=MAX_ADVANCEMENT_INTELLECT_EXPERTISE_ROWS).contains(&usize::from(self.threshold))
            && self.expertise.as_ref().is_none_or(Checked::checked)
    }
}

impl AdvancementExpertiseRow {
    pub(crate) fn valid(&self) -> bool {
        self.checked()
    }
}
impl Checked for AdvancementExpertiseRow {
    fn checked(&self) -> bool {
        self.expertise.checked()
            && (1..=MAX_EXPERTISE_ORIGINS).contains(&usize::from(self.source_count))
    }
}

#[cfg(test)]
mod wide_scalar_tests {
    use super::*;

    #[test]
    fn wide_scalar_advancement_health_and_points_use_canonical_decimal_strings() {
        let wide = ((1_u64 << 53) + 1).to_string();
        let health =
            serde_json::json!({"current": wide, "maximum": wide, "injury": "0", "dead": false});
        let decoded: AdvancementHealth = serde_json::from_value(health.clone()).unwrap();
        assert_eq!(serde_json::to_value(decoded).unwrap(), health);
        let points = serde_json::json!({"earned": wide, "spent": "0", "unspent": wide});
        let decoded: AdvancementPoints = serde_json::from_value(points.clone()).unwrap();
        assert_eq!(serde_json::to_value(decoded).unwrap(), points);
    }

    #[test]
    fn wide_scalar_owner_is_independent_of_the_bounded_purchase_number() {
        let source = serde_json::json!({"owner": "65536", "index": 255});
        let decoded: ReplacementSource = serde_json::from_value(source.clone()).unwrap();
        assert_eq!(serde_json::to_value(decoded).unwrap(), source);
        for owner in [
            serde_json::json!(u64::MAX.to_string()),
            serde_json::json!(65536),
            serde_json::json!("065536"),
            serde_json::json!("18446744073709551616"),
        ] {
            assert!(serde_json::from_value::<ReplacementSource>(
                serde_json::json!({"owner": owner, "index": 0})
            )
            .is_err());
            assert!(serde_json::from_value::<PurchaseOperation>(
                serde_json::json!({"owner": owner, "pool": "general", "selection": {"kind": "skill_rank", "skill": "athletics"}})
            ).is_err());
        }
        assert!(serde_json::from_value::<ReplacementSource>(
            serde_json::json!({"owner": "65536", "index": 256})
        )
        .is_err());
    }
}
