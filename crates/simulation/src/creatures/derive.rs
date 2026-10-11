use super::{catalog::type_grants, BuildError, CreatureBuild, GrantSource, Subtype, Template};
use crate::attributes::{Attribute, Attributes, Defenses, SkillRanks};
use crate::damage::Protection;
use crate::dice::DicePool;
use crate::grants::{Ability, Descriptor, Grant, Selector};
use crate::progression::{Class, CreatureType, HdSource};
use crate::resources::ResourceMaxima;
use crate::talents::{active_talents, Talent};
use crate::AnatomySpec;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DerivedCreature {
    pub kind: CreatureType,
    pub subtypes: BTreeSet<Subtype>,
    pub attributes: Attributes,
    pub skills: SkillRanks,
    pub anatomy: AnatomySpec,
    pub maximum_health: u32,
    pub resources: ResourceMaxima,
    pub defenses: Defenses,
    pub melee: crate::attacks::MeleeAttack,
    melee_dice: u16,
    melee_flat: i32,
    pub bolt: DicePool,
    pub fear_difficulty_bonus: i32,
    pub fear_duration_bonus: u64,
    pub active_talents: BTreeSet<Talent>,
    pub dormant_talents: BTreeSet<Talent>,
    pub abilities: BTreeSet<Ability>,
    pub grants: BTreeMap<GrantSource, Vec<Grant>>,
    pub protection: Protection,
    pub magical: bool,
    pub mindless: bool,
}

impl DerivedCreature {
    /// Apply permanent and action-specific changes together before zero clamping.
    /// Keeping the source definition intact preserves negative fixed modifiers.
    pub fn melee_damage(
        &self,
        impact_bonus: u32,
    ) -> Result<crate::damage::DamageSpec, crate::damage::DamageError> {
        self.damage_for_melee(&self.melee, impact_bonus)
    }

    pub(crate) fn damage_for_melee(
        &self,
        attack: &crate::attacks::MeleeAttack,
        impact_bonus: u32,
    ) -> Result<crate::damage::DamageSpec, crate::damage::DamageError> {
        attack.damage_with_modifiers(self.melee_dice, self.melee_flat, impact_bonus)
    }

    pub fn immunity_sources(&self, selector: Selector) -> Vec<GrantSource> {
        self.grants
            .iter()
            .filter(|(_, grants)| grants.contains(&Grant::Immunity(selector)))
            .map(|(source, _)| source.clone())
            .collect()
    }
}

pub(super) fn derive(build: &CreatureBuild) -> Result<DerivedCreature, BuildError> {
    if !build.species.valid()
        || build.templates.len() > 32
        || build.templates.iter().any(|template| !template.valid())
        || build
            .templates
            .iter()
            .map(|template| &template.id)
            .collect::<BTreeSet<_>>()
            .len()
            != build.templates.len()
    {
        return Err(BuildError::InvalidDefinition);
    }
    if build.choices.len() != usize::from(build.ledger.total_hd()) {
        return Err(BuildError::InvalidOwnership);
    }
    let mut attributes = build.initial_attributes;
    if Attribute::ALL
        .into_iter()
        .any(|attribute| attributes.get(attribute) > 5)
    {
        return Err(BuildError::AttributeCap);
    }
    let mut skills = SkillRanks::default();
    let mut selected = BTreeSet::new();
    for (index, (entry, choice)) in build
        .ledger
        .entries()
        .iter()
        .zip(&build.choices)
        .enumerate()
    {
        let budget = match entry.source() {
            HdSource::Racial => 1,
            HdSource::Class(_) => 2,
        };
        if choice.training.len() > budget {
            return Err(BuildError::NoTrainingPoint);
        }
        for &skill in &choice.training {
            skills = skills
                .with_rank(skill, skills.get(skill) + 1)
                .map_err(|_| BuildError::RankCap)?;
        }
        if let Some(attribute) = choice.attribute {
            if !(index + 1).is_multiple_of(4) {
                return Err(BuildError::NoAttributeOpportunity);
            }
            if attributes.get(attribute) >= 5 {
                return Err(BuildError::AttributeCap);
            }
            let mut adjustment = [0; 6];
            adjustment[attribute as usize] = 1;
            attributes = attributes
                .adjusted(adjustment)
                .map_err(|_| BuildError::AttributeCap)?;
        }
        if choice.talent.is_some_and(|talent| !selected.insert(talent)) {
            return Err(BuildError::DuplicateTalent);
        }
    }
    let mut kind = build.species.kind;
    let mut subtypes = build.species.subtypes.clone();
    apply_templates(&build.templates, &mut kind, &mut subtypes, &mut attributes)?;
    let mut derived = DerivedCreature {
        kind,
        subtypes,
        attributes,
        skills,
        anatomy: build.species.anatomy.clone(),
        maximum_health: build.ledger.health(kind) + u32::from(attributes.get(Attribute::Strength)),
        resources: ResourceMaxima::derived(attributes, build.binding, false),
        defenses: attributes.defenses(),
        melee: build.species.melee.clone(),
        melee_dice: 0,
        melee_flat: 0,
        bolt: DicePool::new(1, 6, 1).expect("constant bolt pool is valid"),
        fear_difficulty_bonus: 0,
        fear_duration_bonus: 0,
        active_talents: BTreeSet::new(),
        dormant_talents: BTreeSet::new(),
        abilities: BTreeSet::from([Ability::BasicMelee]),
        grants: BTreeMap::new(),
        protection: Protection::default(),
        magical: false,
        mindless: false,
    };
    add_source(
        &mut derived.grants,
        GrantSource::Species(build.species.id.clone()),
        &build.species.grants,
    );
    add_source(
        &mut derived.grants,
        GrantSource::Type(kind),
        type_grants(kind),
    );
    for &subtype in &derived.subtypes {
        add_source(
            &mut derived.grants,
            GrantSource::Subtype(subtype),
            subtype.grants(),
        );
    }
    if build.ledger.class_level(Class::Mage) > 0 {
        add_source(
            &mut derived.grants,
            GrantSource::Class(Class::Mage),
            &[Grant::Magical],
        );
    }
    for template in &build.templates {
        add_source(
            &mut derived.grants,
            GrantSource::Template(template.id.clone()),
            &template.grants,
        );
    }
    derived.magical = derived
        .grants
        .values()
        .flatten()
        .any(|grant| *grant == Grant::Magical);
    derived.mindless = derived
        .grants
        .values()
        .flatten()
        .any(|grant| *grant == Grant::Mindless);
    derived.resources = ResourceMaxima::derived(attributes, build.binding, derived.magical);
    derived.active_talents = active_talents(&build.talent_facts(&derived), &selected);
    derived.dormant_talents = selected
        .difference(&derived.active_talents)
        .copied()
        .collect();
    for &talent in &derived.active_talents {
        add_source(
            &mut derived.grants,
            GrantSource::Talent(talent),
            talent.grants(),
        );
    }
    apply_grants(&mut derived)?;
    derived.protection = Protection::from_grants(derived.grants.values().flatten().copied())
        .map_err(|_| BuildError::DerivedLimit)?;
    if build.ledger.total_hd() == 0 {
        derived.maximum_health = 0;
        derived.abilities.clear();
    }
    Ok(derived)
}

fn add_source(
    sources: &mut BTreeMap<GrantSource, Vec<Grant>>,
    source: GrantSource,
    grants: &[Grant],
) {
    if grants.is_empty() {
        return;
    }
    let mut values = grants.to_vec();
    if grants.contains(&Grant::Mindless) {
        for descriptor in [Descriptor::Fear, Descriptor::MindAffecting] {
            let immunity = Grant::Immunity(Selector::Descriptor(descriptor));
            if !values.contains(&immunity) {
                values.push(immunity);
            }
        }
    }
    sources.insert(source, values);
}

fn apply_templates(
    templates: &[Template],
    kind: &mut CreatureType,
    subtypes: &mut BTreeSet<Subtype>,
    attributes: &mut Attributes,
) -> Result<(), BuildError> {
    let mut ordered: Vec<_> = templates.iter().collect();
    ordered.sort_by_key(|template| (template.priority, &template.id));
    let mut cursor = 0;
    while cursor < ordered.len() {
        let end = cursor
            + ordered[cursor..]
                .partition_point(|template| template.priority == ordered[cursor].priority);
        let mut change_kind = None;
        let mut overrides = BTreeMap::new();
        let mut additions: BTreeSet<Subtype> = BTreeSet::new();
        let mut removals: BTreeSet<Subtype> = BTreeSet::new();
        let mut adjustments = [0_i16; 6];
        for template in &ordered[cursor..end] {
            if let Some(change) = template.kind {
                if change_kind.is_some_and(|previous| previous != change) {
                    return Err(BuildError::ConflictingTemplates);
                }
                change_kind = Some(change);
            }
            for (&attribute, &value) in &template.overrides {
                if overrides
                    .insert(attribute, value)
                    .is_some_and(|previous| previous != value)
                {
                    return Err(BuildError::ConflictingTemplates);
                }
            }
            additions.extend(&template.add_subtypes);
            removals.extend(&template.remove_subtypes);
            for (adjustment, value) in adjustments.iter_mut().zip(template.adjustments) {
                *adjustment += value;
            }
        }
        if !additions.is_disjoint(&removals) {
            return Err(BuildError::ConflictingTemplates);
        }
        *attributes = attributes
            .adjusted(adjustments)
            .map_err(|_| BuildError::DerivedLimit)?;
        let mut values = Attribute::ALL.map(|attribute| attributes.get(attribute));
        for (attribute, value) in overrides {
            values[attribute as usize] = value;
        }
        *attributes = Attributes::from_derived(values).map_err(|_| BuildError::DerivedLimit)?;
        if let Some(change) = change_kind {
            *kind = change;
        }
        subtypes.retain(|subtype| !removals.contains(subtype));
        subtypes.extend(additions);
        cursor = end;
    }
    Ok(())
}

fn apply_grants(derived: &mut DerivedCreature) -> Result<(), BuildError> {
    let mut melee_dice = 0_u16;
    let mut melee_flat = 0_i32;
    let mut bolt_dice = 0_u16;
    let mut bolt_flat = 0_i32;
    for &grant in derived.grants.values().flatten() {
        match grant {
            Grant::Health(value) => add_unsigned(&mut derived.maximum_health, value)?,
            Grant::Stamina(value) => add_unsigned(&mut derived.resources.stamina, value)?,
            Grant::Focus(value) => add_unsigned(&mut derived.resources.focus, value)?,
            Grant::Mana(value) if derived.magical => {
                add_unsigned(&mut derived.resources.mana, value)?
            }
            Grant::PhysicalDefense(value) => add_signed(&mut derived.defenses.physical, value)?,
            Grant::MeleeFlat(value) => add_signed(&mut melee_flat, value)?,
            Grant::MeleeDice(value) => {
                melee_dice = melee_dice
                    .checked_add(value)
                    .ok_or(BuildError::DerivedLimit)?
            }
            Grant::BoltFlat(value) => add_signed(&mut bolt_flat, value)?,
            Grant::BoltDice(value) => {
                bolt_dice = bolt_dice
                    .checked_add(value)
                    .ok_or(BuildError::DerivedLimit)?
            }
            Grant::FearDifficulty(value) => add_signed(&mut derived.fear_difficulty_bonus, value)?,
            Grant::FearDuration(value) => {
                derived.fear_duration_bonus = derived
                    .fear_duration_bonus
                    .checked_add(value)
                    .filter(|value| {
                        *value <= crate::fear::MAX_FEAR_DURATION - crate::fear::BASE_FEAR_DURATION
                    })
                    .ok_or(BuildError::DerivedLimit)?;
            }
            Grant::Ability(ability) => {
                derived.abilities.insert(ability);
            }
            _ => {}
        }
    }
    derived.melee_dice = melee_dice;
    derived.melee_flat = melee_flat;
    derived
        .melee_damage(0)
        .map_err(|_| BuildError::DerivedLimit)?;
    derived.bolt = derived
        .bolt
        .augmented(bolt_dice, bolt_flat)
        .map_err(|_| BuildError::DerivedLimit)?;
    Ok(())
}

fn add_unsigned(total: &mut u32, amount: u32) -> Result<(), BuildError> {
    *total = total
        .checked_add(amount)
        .filter(|value| *value <= 1_000_000)
        .ok_or(BuildError::DerivedLimit)?;
    Ok(())
}

fn add_signed(total: &mut i32, amount: i32) -> Result<(), BuildError> {
    *total = total
        .checked_add(amount)
        .filter(|value| (-1_000_000..=1_000_000).contains(value))
        .ok_or(BuildError::DerivedLimit)?;
    Ok(())
}
