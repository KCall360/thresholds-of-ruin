//! Retained Intellect capacity. Thresholds own choices; current permanent
//! Intellect only controls activation. Reaching an earned threshold again never
//! clears its choice. A genuinely new maximum earns its new thresholds once.
use super::intellect::{self, IntellectSources};
use super::{BuildError, CreatureBuild, HdChoices};
use crate::attributes::Attributes;
use crate::content_ids::{ExpertiseId, TemplateId};
use crate::progression::HdSource;
use std::collections::BTreeSet;

/// One exact historical HD choice version. Unchanged rows are shared by receipt
/// indices; discarded ChooseAgain choices never become current paid ownership.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct IntellectChoiceWitness {
    pub owner: crate::HdIndex,
    pub source: HdSource,
    pub health_seed: u64,
    pub choices: HdChoices,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct IntellectBirthWitness {
    pub attributes: Attributes,
    pub templates: Vec<TemplateId>,
    pub choices: Vec<u16>,
    pub sources: IntellectSources,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct IntellectAcquisition {
    pub before_choices: Vec<u16>,
    pub after_choices: Vec<u16>,
    pub before: IntellectSources,
    pub after: IntellectSources,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct IntellectThreshold {
    /// Zero denotes birth; otherwise one-based into retained acquisitions.
    pub acquisition: u16,
    pub expertise: Option<ExpertiseId>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct IntellectLearning {
    pub birth: IntellectBirthWitness,
    pub history: Vec<IntellectChoiceWitness>,
    pub acquisitions: Vec<IntellectAcquisition>,
    /// Index zero is the Intellect-1 threshold. Contiguity is structural.
    pub thresholds: Vec<IntellectThreshold>,
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
enum SelectionKey<'a> {
    Skill(crate::attributes::Skill),
    Expertise(&'a ExpertiseId),
    Talent(&'a crate::content_ids::TalentId),
    Form(
        &'a crate::content_ids::TalentId,
        &'a crate::content_ids::FormId,
    ),
}
type ChoiceKey<'a> = (
    crate::HdIndex,
    Option<&'a crate::content_ids::ClassId>,
    u64,
    Option<crate::attributes::Attribute>,
    Vec<(&'a crate::content_ids::PointPoolId, SelectionKey<'a>)>,
);
impl IntellectChoiceWitness {
    fn key(&self) -> ChoiceKey<'_> {
        let class = match &self.source {
            HdSource::Racial => None,
            HdSource::Class(id) => Some(id),
        };
        let purchases = self
            .choices
            .purchases
            .iter()
            .map(|purchase| {
                let selection = match &purchase.selection {
                    super::PurchaseSelection::SkillRank(skill) => SelectionKey::Skill(*skill),
                    super::PurchaseSelection::Expertise(id) => SelectionKey::Expertise(id),
                    super::PurchaseSelection::Talent(id) => SelectionKey::Talent(id),
                    super::PurchaseSelection::TalentForm { talent, form } => {
                        SelectionKey::Form(talent, form)
                    }
                };
                (&purchase.pool, selection)
            })
            .collect();
        (
            self.owner,
            class,
            self.health_seed,
            self.choices.attribute,
            purchases,
        )
    }
}

impl IntellectLearning {
    pub fn new(
        build: &CreatureBuild,
        sources: IntellectSources,
        capacity: u16,
    ) -> Result<Self, BuildError> {
        if capacity > crate::limits::DERIVED_ATTRIBUTE {
            return Err(BuildError::DerivedLimit);
        }
        let history = Self::capture_choices(build)?;
        let choices = (0..history.len())
            .map(|index| u16::try_from(index).map_err(|_| BuildError::Arithmetic))
            .collect::<Result<_, _>>()?;
        Ok(Self {
            birth: IntellectBirthWitness {
                attributes: build.initial_attributes(),
                templates: build.template_ids().to_vec(),
                choices,
                sources,
            },
            history,
            acquisitions: Vec::new(),
            thresholds: vec![
                IntellectThreshold {
                    acquisition: 0,
                    expertise: None
                };
                usize::from(capacity)
            ],
        })
    }
    fn capture_choices(build: &CreatureBuild) -> Result<Vec<IntellectChoiceWitness>, BuildError> {
        build
            .ledger()
            .entries()
            .iter()
            .zip(build.choices())
            .enumerate()
            .map(|(owner, (die, choices))| {
                Ok(IntellectChoiceWitness {
                    owner: crate::HdIndex::from_usize(owner).map_err(BuildError::from)?,
                    source: die.source(),
                    health_seed: die.health_seed(),
                    choices: choices.clone(),
                })
            })
            .collect()
    }
    fn retain_choices(&mut self, build: &CreatureBuild) -> Result<Vec<u16>, BuildError> {
        Self::capture_choices(build)?
            .into_iter()
            .map(|row| {
                if let Some(index) = self.history.iter().position(|existing| existing == &row) {
                    return u16::try_from(index).map_err(|_| BuildError::Arithmetic);
                }
                if self.history.len() >= crate::limits::EXPERTISE_HISTORY_ROWS {
                    return Err(BuildError::DerivedLimit);
                }
                let index =
                    u16::try_from(self.history.len()).map_err(|_| BuildError::Arithmetic)?;
                self.history.push(row);
                Ok(index)
            })
            .collect()
    }
    /// Called only at an explicit, validated permanent advancement boundary.
    /// Increases within retained capacity neither allocate nor rewrite choices.
    pub fn record_acquisition(
        &mut self,
        before_build: &CreatureBuild,
        after_build: &CreatureBuild,
        before: IntellectSources,
        after: IntellectSources,
        after_capacity: u16,
    ) -> Result<(), BuildError> {
        if after_capacity > crate::limits::DERIVED_ATTRIBUTE {
            return Err(BuildError::DerivedLimit);
        }
        if usize::from(after_capacity) <= self.thresholds.len() {
            return Ok(());
        }
        let before_capacity = intellect::eligible_intellect_at(
            before_build,
            self.birth.attributes,
            &self.birth.templates,
            &before,
        )?;
        if usize::from(before_capacity) > self.thresholds.len()
            || after_capacity <= before_capacity
            || self.acquisitions.len() >= usize::from(crate::limits::DERIVED_ATTRIBUTE)
        {
            return Err(BuildError::InvalidOwnership);
        }
        let before_choices = self.retain_choices(before_build)?;
        let after_choices = self.retain_choices(after_build)?;
        let acquisition =
            u16::try_from(self.acquisitions.len() + 1).map_err(|_| BuildError::Arithmetic)?;
        self.acquisitions.push(IntellectAcquisition {
            before_choices,
            after_choices,
            before,
            after,
        });
        self.thresholds.resize(
            usize::from(after_capacity),
            IntellectThreshold {
                acquisition,
                expertise: None,
            },
        );
        Ok(())
    }
    pub fn choose(
        &mut self,
        threshold: u16,
        expertise: ExpertiseId,
        capacity: u16,
    ) -> Result<(), BuildError> {
        if threshold == 0 || threshold > capacity {
            return Err(BuildError::InvalidOwnership);
        }
        if self
            .thresholds
            .iter()
            .any(|row| row.expertise.as_ref() == Some(&expertise))
        {
            return Err(BuildError::DuplicateExpertise);
        }
        let row = self
            .thresholds
            .get_mut(usize::from(threshold - 1))
            .ok_or(BuildError::InvalidOwnership)?;
        if row.expertise.is_some() {
            return Err(BuildError::OccupiedSlot);
        }
        row.expertise = Some(expertise);
        Ok(())
    }
    fn validate_choice(
        build: &CreatureBuild,
        witness: &IntellectChoiceWitness,
    ) -> Result<(), BuildError> {
        if witness.owner.get() >= crate::limits::HIT_DICE as u64 {
            return Err(BuildError::InvalidOwnership);
        }
        let grants = build
            .ruleset()
            .point_grants(&witness.source)
            .ok_or(BuildError::UnknownClass)?;
        let choices = &witness.choices;
        if choices.purchases.len() > crate::limits::POINTS_PER_HIT_DIE {
            return Err(BuildError::NoAdvancementPoint);
        }
        if choices.attribute.is_some()
            && !(witness.owner.get() + 1).is_multiple_of(u64::from(
                build.ruleset().constants().attribute_opportunity_interval,
            ))
        {
            return Err(BuildError::NoAttributeOpportunity);
        }
        let mut spent = std::collections::BTreeMap::new();
        for purchase in &choices.purchases {
            purchase.selection.validate(build.ruleset())?;
            let pool = build
                .ruleset()
                .point_pool(&purchase.pool)
                .ok_or(BuildError::UnknownPointPool)?;
            if !pool.allowed.contains(&purchase.selection.kind()) {
                return Err(BuildError::PurchaseNotAllowed);
            }
            let grant = grants
                .iter()
                .find(|grant| grant.pool == purchase.pool)
                .ok_or(BuildError::UnknownPointPool)?;
            let count = spent.entry(&purchase.pool).or_insert(0_u16);
            *count = count.checked_add(1).ok_or(BuildError::Arithmetic)?;
            if *count > grant.points {
                return Err(BuildError::NoAdvancementPoint);
            }
        }
        Ok(())
    }
    fn validate_sources(
        &self,
        build: &CreatureBuild,
        rows: &[u16],
        witness: &IntellectSources,
    ) -> Result<u16, BuildError> {
        if rows.len() > crate::limits::HIT_DICE {
            return Err(BuildError::InvalidOwnership);
        }
        let choices = rows
            .iter()
            .enumerate()
            .map(|(owner, index)| {
                let row = self
                    .history
                    .get(usize::from(*index))
                    .ok_or(BuildError::InvalidOwnership)?;
                if row.owner.get() != owner as u64 {
                    return Err(BuildError::InvalidOwnership);
                }
                Ok((&row.source, &row.choices))
            })
            .collect::<Result<Vec<_>, BuildError>>()?;
        let mut paid = BTreeSet::new();
        for (_, choice) in &choices {
            for id in choice
                .purchases
                .iter()
                .filter_map(|purchase| purchase.selection.expertise())
            {
                if !paid.insert(id) {
                    return Err(BuildError::DuplicateExpertise);
                }
            }
        }
        if let Some(root) = build.ledger().health_root_seed() {
            let expected = crate::progression::HdLedger::seeded(
                choices
                    .iter()
                    .map(|(source, _)| (*source).clone())
                    .collect(),
                root,
            )
            .map_err(BuildError::from)?;
            if rows.iter().zip(expected.entries()).any(|(index, die)| {
                self.history[usize::from(*index)].health_seed != die.health_seed()
            }) {
                return Err(BuildError::InvalidHealthRoot);
            }
        }
        let actual = intellect::historical_sources_refs(
            build,
            self.birth.attributes,
            &self.birth.templates,
            choices.iter().copied(),
        )?;
        if &actual != witness {
            return Err(BuildError::InvalidOwnership);
        }
        intellect::eligible_intellect_at(
            build,
            self.birth.attributes,
            &self.birth.templates,
            witness,
        )
    }
    pub fn validate(&self, build: &CreatureBuild) -> Result<(), BuildError> {
        if self.birth.attributes != build.initial_attributes() {
            return Err(BuildError::InvalidOwnership);
        }
        if self.thresholds.len() > usize::from(crate::limits::DERIVED_ATTRIBUTE)
            || self.acquisitions.len() > usize::from(crate::limits::DERIVED_ATTRIBUTE)
            || self.history.len() > crate::limits::EXPERTISE_HISTORY_ROWS
        {
            return Err(BuildError::DerivedLimit);
        }
        let mut distinct = BTreeSet::new();
        for row in &self.history {
            Self::validate_choice(build, row)?;
            if !distinct.insert(row.key()) {
                return Err(BuildError::InvalidOwnership);
            }
        }
        let mut used = self.birth.choices.iter().copied().collect::<BTreeSet<_>>();
        let birth_capacity =
            self.validate_sources(build, &self.birth.choices, &self.birth.sources)?;
        let mut earned = usize::from(birth_capacity);
        if self.thresholds.len() < earned
            || self.thresholds[..earned]
                .iter()
                .any(|row| row.acquisition != 0)
        {
            return Err(BuildError::InvalidOwnership);
        }
        for (index, receipt) in self.acquisitions.iter().enumerate() {
            if receipt.after_choices.len() < receipt.before_choices.len()
                || receipt.after_choices.len() > receipt.before_choices.len() + 1
            {
                return Err(BuildError::InvalidOwnership);
            }
            for (before, after) in receipt.before_choices.iter().zip(&receipt.after_choices) {
                let left = self
                    .history
                    .get(usize::from(*before))
                    .ok_or(BuildError::InvalidOwnership)?;
                let right = self
                    .history
                    .get(usize::from(*after))
                    .ok_or(BuildError::InvalidOwnership)?;
                if left.source != right.source || left.health_seed != right.health_seed {
                    return Err(BuildError::InvalidOwnership);
                }
            }
            let before = self.validate_sources(build, &receipt.before_choices, &receipt.before)?;
            let after = self.validate_sources(build, &receipt.after_choices, &receipt.after)?;
            if usize::from(before) > earned
                || after <= before
                || usize::from(after) <= earned
                || self.thresholds.len() < usize::from(after)
            {
                return Err(BuildError::InvalidOwnership);
            }
            if self.thresholds[earned..usize::from(after)]
                .iter()
                .any(|row| usize::from(row.acquisition) != index + 1)
            {
                return Err(BuildError::InvalidOwnership);
            }
            earned = usize::from(after);
            used.extend(
                receipt
                    .before_choices
                    .iter()
                    .chain(&receipt.after_choices)
                    .copied(),
            );
        }
        if earned != self.thresholds.len() || used.len() != self.history.len() {
            return Err(BuildError::InvalidOwnership);
        }
        let current = intellect::current_sources(build, &self.birth.templates)?;
        if usize::from(intellect::eligible_intellect(
            build,
            &self.birth.templates,
            &current,
        )?) > earned
        {
            return Err(BuildError::InvalidOwnership);
        }
        let mut chosen = BTreeSet::new();
        for id in self
            .thresholds
            .iter()
            .filter_map(|row| row.expertise.as_ref())
        {
            let definition = build
                .ruleset()
                .expertise(id)
                .ok_or(BuildError::UnknownExpertise)?;
            if definition.category == crate::expertise::ExpertiseCategory::Specialist {
                return Err(BuildError::PurchaseNotAllowed);
            }
            if !chosen.insert(id) {
                return Err(BuildError::DuplicateExpertise);
            }
        }
        Ok(())
    }
}
