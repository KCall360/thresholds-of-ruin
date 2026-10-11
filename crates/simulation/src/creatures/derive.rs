use super::{BuildError, CreatureBuild, SourceId, Template};
use crate::attributes::{Attribute, Attributes, ByAttribute, Defenses, SkillRanks};
use crate::content_ids::{AbilityId, ClassId, FlagId, SubtypeId, TalentId, TypeId};
use crate::damage::Protection;
use crate::progression::HdSource;
use crate::resources::{Resource, ResourceMaxima};
use crate::rules::{
    AbilityStat, CapacityAttribute, DefenseKind, Modifier, ModifierOp, Ruleset, Scope,
    SkillSelector, Stat,
};
use crate::AnatomySpec;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AbilityValues {
    pub dice: u16,
    pub flat: i32,
    pub check_bonus: i32,
    pub difficulty: i32,
    pub duration: u64,
    pub range: i32,
    pub preparation: i64,
    pub recovery: i64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DerivedCreature {
    pub kind: TypeId,
    pub subtypes: BTreeSet<SubtypeId>,
    pub attributes: Attributes,
    pub skills: SkillRanks,
    pub anatomy: AnatomySpec,
    pub maximum_health: u32,
    pub resources: ResourceMaxima,
    pub defenses: Defenses,
    pub melee: crate::attacks::MeleeAttack,
    pub active_talents: BTreeSet<TalentId>,
    pub dormant_talents: BTreeSet<TalentId>,
    pub abilities: BTreeSet<AbilityId>,
    pub ability_values: BTreeMap<AbilityId, AbilityValues>,
    pub sources: BTreeMap<SourceId, Vec<Modifier>>,
    pub protection: Protection,
    pub flags: BTreeSet<FlagId>,
    /// Eligibility excludes talent, equipment and condition benefits.
    permanent_attributes: Attributes,
    permanent_flags: BTreeSet<FlagId>,
    base_edge: crate::dice::Edge,
    scoped_edges: BTreeMap<crate::ActorId, crate::dice::Edge>,
}
impl DerivedCreature {
    pub fn immunity_sources(&self, selector: &crate::rules::Selector) -> Vec<SourceId> {
        self.sources
            .iter()
            .filter(|(_, modifiers)| {
                modifiers.iter().any(|modifier|
            matches!(&modifier.op, ModifierOp::Immunity { selector: value } if value == selector))
            })
            .map(|(source, _)| source.clone())
            .collect()
    }
    pub fn ability_values(&self, ability: &AbilityId) -> AbilityValues {
        self.ability_values
            .get(ability)
            .cloned()
            .unwrap_or_default()
    }
    /// Effective reach shared by admission and disclosed targeting metadata.
    pub fn ability_reach(
        &self,
        ability: &AbilityId,
        rules: &Ruleset,
    ) -> Result<crate::rules::Reach, BuildError> {
        let definition = rules
            .ability(ability)
            .ok_or(BuildError::InvalidDefinition)?;
        match definition.reach {
            crate::rules::Reach::Sight { range } => {
                let range = i64::from(range) + i64::from(self.ability_values(ability).range);
                if !(1..=i64::from(crate::limits::SIGHT_RANGE)).contains(&range) {
                    return Err(BuildError::DerivedLimit);
                }
                Ok(crate::rules::Reach::Sight {
                    range: range as u16,
                })
            }
            ref reach => Ok(reach.clone()),
        }
    }
    pub fn edge_against(&self, target: crate::ActorId) -> crate::dice::Edge {
        self.scoped_edges
            .get(&target)
            .copied()
            .unwrap_or(self.base_edge)
    }
}

pub(super) struct PermanentFacts<'a> {
    build: &'a CreatureBuild,
    derived: &'a DerivedCreature,
}
impl<'a> PermanentFacts<'a> {
    pub(super) fn new(build: &'a CreatureBuild, derived: &'a DerivedCreature) -> Self {
        Self { build, derived }
    }
}
impl crate::rules::TalentFacts for PermanentFacts<'_> {
    fn total_hd(&self) -> u16 {
        self.build.ledger().total_hd()
    }
    fn racial_hd(&self) -> u16 {
        self.build.ledger().racial_hd()
    }
    fn class_level(&self, class: &ClassId) -> u16 {
        self.build.ledger().class_level(class)
    }
    fn creature_type(&self) -> &TypeId {
        &self.derived.kind
    }
    fn skill_rank(&self, skill: &SkillSelector) -> u16 {
        u16::from(self.derived.skills.get(match skill {
            SkillSelector::Named { skill } => *skill,
            SkillSelector::NaturalAttack | SkillSelector::SelectedWeapon => {
                self.build.species().melee.skill()
            }
        }))
    }
    fn attribute(&self, attribute: Attribute) -> u16 {
        self.derived.permanent_attributes.get(attribute)
    }
    fn has_flag(&self, flag: &FlagId) -> bool {
        self.derived.permanent_flags.contains(flag)
    }
}

pub(super) fn derive(build: &CreatureBuild) -> Result<DerivedCreature, BuildError> {
    derive_with_sources(build, &BTreeMap::new())
}
pub(crate) fn derive_with_sources(
    build: &CreatureBuild,
    extra: &BTreeMap<SourceId, Vec<Modifier>>,
) -> Result<DerivedCreature, BuildError> {
    let rules = build.ruleset();
    let species = build.species();
    if build.template_ids().len() > crate::limits::TEMPLATES
        || build.template_ids().iter().collect::<BTreeSet<_>>().len() != build.template_ids().len()
    {
        return Err(BuildError::InvalidDefinition);
    }
    let templates: Vec<_> = build
        .template_ids()
        .iter()
        .map(|id| {
            build
                .content()
                .catalog
                .templates
                .get(id)
                .ok_or(BuildError::UnknownTemplate)
        })
        .collect::<Result<_, _>>()?;
    if build.choices().len() != usize::from(build.ledger().total_hd()) {
        return Err(BuildError::InvalidOwnership);
    }
    let mut attributes = build.initial_attributes();
    if Attribute::ALL
        .into_iter()
        .any(|a| attributes.get(a) > rules.constants().attribute_cap)
    {
        return Err(BuildError::AttributeCap);
    }
    let mut skills = SkillRanks::default();
    let mut selected = BTreeSet::new();
    for (index, (entry, choice)) in build
        .ledger()
        .entries()
        .iter()
        .zip(build.choices())
        .enumerate()
    {
        let budget = match entry.source() {
            HdSource::Racial => rules.constants().racial_training_points,
            HdSource::Class(class) => {
                rules
                    .class(&class)
                    .ok_or(BuildError::UnknownClass)?
                    .training_points
            }
        };
        if choice.training.len() > usize::from(budget) {
            return Err(BuildError::NoTrainingPoint);
        }
        for &skill in &choice.training {
            skills = skills
                .with_rank(
                    skill,
                    skills
                        .get(skill)
                        .checked_add(1)
                        .ok_or(BuildError::RankCap)?,
                )
                .map_err(|_| BuildError::RankCap)?;
        }
        if let Some(attribute) = choice.attribute {
            if !(index + 1).is_multiple_of(usize::from(
                rules.constants().attribute_opportunity_interval,
            )) {
                return Err(BuildError::NoAttributeOpportunity);
            }
            if attributes.get(attribute) >= rules.constants().attribute_cap {
                return Err(BuildError::AttributeCap);
            }
            let mut adjustment = ByAttribute::default();
            *adjustment.get_mut(attribute) = 1;
            attributes = attributes
                .adjusted(adjustment)
                .map_err(|_| BuildError::AttributeCap)?;
        }
        if let Some(talent) = &choice.talent {
            if rules.talent(talent).is_none() {
                return Err(BuildError::UnknownTalent);
            }
            if !selected.insert(talent.clone()) {
                return Err(BuildError::DuplicateTalent);
            }
        }
    }
    let mut kind = species.kind.clone();
    let mut subtypes = species.subtypes.clone();
    apply_templates(&templates, &mut kind, &mut subtypes, &mut attributes)?;
    let mut sources = BTreeMap::new();
    add_source(
        &mut sources,
        SourceId::Species(species.id.clone()),
        &species.modifiers,
        rules,
    )?;
    add_source(
        &mut sources,
        SourceId::Type(kind.clone()),
        &rules
            .creature_type(&kind)
            .ok_or(BuildError::InvalidDefinition)?
            .modifiers,
        rules,
    )?;
    for subtype in &subtypes {
        add_source(
            &mut sources,
            SourceId::Subtype(subtype.clone()),
            &rules
                .subtype(subtype)
                .ok_or(BuildError::InvalidDefinition)?
                .modifiers,
            rules,
        )?;
    }
    let classes: BTreeSet<_> = build
        .ledger()
        .entries()
        .iter()
        .filter_map(|entry| match entry.source() {
            HdSource::Racial => None,
            HdSource::Class(class) => Some(class),
        })
        .collect();
    for class in classes {
        add_source(
            &mut sources,
            SourceId::Class(class.clone()),
            &rules
                .class(&class)
                .ok_or(BuildError::UnknownClass)?
                .modifiers,
            rules,
        )?;
    }
    for template in templates {
        add_source(
            &mut sources,
            SourceId::Template(template.id.clone()),
            &template.modifiers,
            rules,
        )?;
    }
    let permanent_flags = flags(&sources);
    let permanent_attributes = attribute_values(attributes, &collect_stats(&sources)?, rules)?;
    let mut derived = DerivedCreature {
        kind,
        subtypes,
        attributes,
        skills,
        anatomy: species.anatomy.clone(),
        maximum_health: 0,
        resources: ResourceMaxima {
            stamina: 0,
            focus: 0,
            mana: 0,
        },
        defenses: crate::attributes::Defenses {
            physical: 0,
            cognitive: 0,
            spiritual: 0,
        },
        melee: species.melee.clone(),
        active_talents: BTreeSet::new(),
        dormant_talents: BTreeSet::new(),
        abilities: rules
            .constants()
            .default_abilities
            .iter()
            .cloned()
            .collect(),
        ability_values: BTreeMap::new(),
        sources,
        protection: Protection::default(),
        flags: BTreeSet::new(),
        permanent_attributes,
        permanent_flags,
        base_edge: crate::dice::Edge::default(),
        scoped_edges: BTreeMap::new(),
    };
    derived.active_talents = rules.active_talents(&selected, &PermanentFacts::new(build, &derived));
    derived.dormant_talents = selected
        .difference(&derived.active_talents)
        .cloned()
        .collect();
    for talent in &derived.active_talents {
        add_source(
            &mut derived.sources,
            SourceId::Talent(talent.clone()),
            &rules
                .talent(talent)
                .ok_or(BuildError::UnknownTalent)?
                .modifiers,
            rules,
        )?;
    }
    for (source, modifiers) in extra {
        add_source(&mut derived.sources, source.clone(), modifiers, rules)?;
    }
    fold(&mut derived, build)?;
    if build.ledger().total_hd() == 0 {
        derived.maximum_health = 0;
        derived.abilities.clear();
    }
    Ok(derived)
}

fn add_source(
    sources: &mut BTreeMap<SourceId, Vec<Modifier>>,
    source: SourceId,
    modifiers: &[Modifier],
    rules: &Ruleset,
) -> Result<(), BuildError> {
    (if matches!(source, SourceId::Condition { .. }) {
        rules.validate_condition_modifiers(modifiers)
    } else {
        rules.validate_modifiers(modifiers)
    })
    .map_err(|_| BuildError::InvalidDefinition)?;
    let mut expanded = modifiers.to_vec();
    let mut seen = BTreeSet::new();
    let mut cursor = 0;
    while cursor < expanded.len() {
        if let ModifierOp::Flag { flag } = &expanded[cursor].op {
            if seen.insert(flag.clone()) {
                let implied = &rules
                    .flag(flag)
                    .ok_or(BuildError::InvalidDefinition)?
                    .modifiers;
                expanded.extend(implied.iter().cloned());
            }
        }
        cursor += 1;
    }
    sources.insert(source, expanded);
    Ok(())
}
fn flags(sources: &BTreeMap<SourceId, Vec<Modifier>>) -> BTreeSet<FlagId> {
    sources
        .values()
        .flatten()
        .filter_map(|modifier| match &modifier.op {
            ModifierOp::Flag { flag } => Some(flag.clone()),
            _ => None,
        })
        .collect()
}
#[derive(Clone, Default)]
struct Accumulator {
    added: i64,
    set: Option<i64>,
}
impl Accumulator {
    fn contribute(&mut self, op: &ModifierOp) -> Result<(), BuildError> {
        match op {
            ModifierOp::Add { value, .. } => {
                self.added = self
                    .added
                    .checked_add(*value)
                    .ok_or(BuildError::DerivedLimit)?
            }
            ModifierOp::Set { value, .. } => {
                if self.set.is_some_and(|previous| previous != *value) {
                    return Err(BuildError::ConflictingTemplates);
                }
                self.set = Some(*value);
            }
            _ => {}
        }
        Ok(())
    }
    fn value(&self, base: i64) -> Result<i64, BuildError> {
        let result = self.set.unwrap_or(
            base.checked_add(self.added)
                .ok_or(BuildError::DerivedLimit)?,
        );
        if !(-crate::limits::NUMERIC..=crate::limits::NUMERIC).contains(&result) {
            return Err(BuildError::DerivedLimit);
        }
        Ok(result)
    }
}
fn collect_stats(
    sources: &BTreeMap<SourceId, Vec<Modifier>>,
) -> Result<BTreeMap<Stat, Accumulator>, BuildError> {
    let mut values: BTreeMap<Stat, Accumulator> = BTreeMap::new();
    for modifier in sources
        .values()
        .flatten()
        .filter(|m| m.scope == Scope::Always)
    {
        match &modifier.op {
            ModifierOp::Add { stat, .. } | ModifierOp::Set { stat, .. } => {
                values
                    .entry(stat.clone())
                    .or_default()
                    .contribute(&modifier.op)?;
            }
            ModifierOp::Flag { .. }
            | ModifierOp::Immunity { .. }
            | ModifierOp::Reduction { .. }
            | ModifierOp::GrantAbility { .. } => {}
        }
    }
    Ok(values)
}
fn value(values: &BTreeMap<Stat, Accumulator>, stat: Stat, base: i64) -> Result<i64, BuildError> {
    values.get(&stat).map_or(Ok(base), |acc| acc.value(base))
}
fn attribute_values(
    base: Attributes,
    values: &BTreeMap<Stat, Accumulator>,
    rules: &Ruleset,
) -> Result<Attributes, BuildError> {
    let mut attributes = base.values();
    for attribute in Attribute::ALL {
        let number = value(
            values,
            Stat::Attribute { attribute },
            i64::from(base.get(attribute)),
        )?
        .max(0);
        if number > i64::from(rules.constants().derived_attribute_cap) {
            return Err(BuildError::DerivedLimit);
        }
        *attributes.get_mut(attribute) = number as u16;
    }
    Attributes::from_derived(attributes).map_err(|_| BuildError::DerivedLimit)
}
fn fold(derived: &mut DerivedCreature, build: &CreatureBuild) -> Result<(), BuildError> {
    let rules = build.ruleset();
    let values = collect_stats(&derived.sources)?;
    let base = values.get(&Stat::Edge).cloned().unwrap_or_default();
    let edge =
        |value: i64| crate::dice::Edge::from_counts(value.max(0) as u32, (-value).max(0) as u32);
    derived.base_edge = edge(base.value(0)?);
    let mut scoped: BTreeMap<crate::ActorId, Accumulator> = BTreeMap::new();
    for (source, modifiers) in &derived.sources {
        if let SourceId::Condition(_, target) = source {
            for modifier in modifiers
                .iter()
                .filter(|modifier| modifier.scope == Scope::VersusSource)
            {
                if matches!(
                    modifier.op,
                    ModifierOp::Add {
                        stat: Stat::Edge,
                        ..
                    } | ModifierOp::Set {
                        stat: Stat::Edge,
                        ..
                    }
                ) {
                    scoped
                        .entry(*target)
                        .or_insert_with(|| base.clone())
                        .contribute(&modifier.op)?;
                }
            }
        }
    }
    derived.scoped_edges = scoped
        .into_iter()
        .map(|(target, accumulator)| Ok((target, edge(accumulator.value(0)?))))
        .collect::<Result<_, BuildError>>()?;

    derived.attributes = attribute_values(derived.attributes, &values, rules)?;
    derived.flags = flags(&derived.sources);
    let attribute = |a| i64::from(derived.attributes.get(a));
    let defense = |kind, left, right| {
        value(
            &values,
            Stat::Defense { defense: kind },
            i64::from(rules.constants().base_defense) + attribute(left) + attribute(right),
        )
        .map(|v| v as i32)
    };
    derived.defenses = Defenses {
        physical: defense(DefenseKind::Physical, Attribute::Strength, Attribute::Speed)?,
        cognitive: defense(
            DefenseKind::Cognitive,
            Attribute::Intellect,
            Attribute::Willpower,
        )?,
        spiritual: defense(
            DefenseKind::Spiritual,
            Attribute::Awareness,
            Attribute::Presence,
        )?,
    };
    let health = i64::from(
        build
            .ledger()
            .health(&derived.kind, rules)
            .map_err(|_| BuildError::InvalidDefinition)?,
    ) + attribute(Attribute::Strength);
    derived.maximum_health = value(&values, Stat::Health, health)?.max(0) as u32;
    for definition in &rules.document().resources {
        let enabled = definition
            .capacity
            .required_flag
            .as_ref()
            .is_none_or(|flag| derived.flags.contains(flag));
        let governed = match definition.capacity.attribute {
            CapacityAttribute::Named { attribute } => attribute,
            CapacityAttribute::Binding => build.binding().attribute(),
        };
        let maximum = if enabled {
            value(
                &values,
                Stat::ResourceMax {
                    resource: definition.resource,
                },
                i64::from(definition.capacity.base) + attribute(governed),
            )?
            .max(0) as u32
        } else {
            0
        };
        match definition.resource {
            Resource::Stamina => derived.resources.stamina = maximum,
            Resource::Focus => derived.resources.focus = maximum,
            Resource::Mana => derived.resources.mana = maximum,
        }
    }
    for modifier in derived.sources.values().flatten() {
        match &modifier.op {
            ModifierOp::GrantAbility { ability } => {
                derived.abilities.insert(ability.clone());
            }
            ModifierOp::Immunity { .. } | ModifierOp::Reduction { .. } => {
                derived
                    .protection
                    .apply(&modifier.op)
                    .map_err(|_| BuildError::DerivedLimit)?;
            }
            ModifierOp::Add { .. } | ModifierOp::Set { .. } | ModifierOp::Flag { .. } => {}
        }
    }
    for ability in &derived.abilities {
        let stat = |stat| {
            value(
                &values,
                Stat::Ability {
                    ability: ability.clone(),
                    stat,
                },
                0,
            )
        };
        let dice = stat(AbilityStat::Dice)?;
        if !(0..i64::from(crate::limits::DICE)).contains(&dice) {
            return Err(BuildError::DerivedLimit);
        }
        derived.ability_values.insert(
            ability.clone(),
            AbilityValues {
                dice: dice as u16,
                flat: stat(AbilityStat::FlatBonus)? as i32,
                check_bonus: stat(AbilityStat::CheckBonus)? as i32,
                difficulty: stat(AbilityStat::Difficulty)? as i32,
                duration: u64::try_from(stat(AbilityStat::Duration)?)
                    .map_err(|_| BuildError::DerivedLimit)?,
                range: stat(AbilityStat::Range)? as i32,
                preparation: stat(AbilityStat::Preparation)?,
                recovery: stat(AbilityStat::Recovery)?,
            },
        );
        derived.ability_reach(ability, rules)?;
    }
    Ok(())
}

fn apply_templates(
    templates: &[&Template],
    kind: &mut TypeId,
    subtypes: &mut BTreeSet<SubtypeId>,
    attributes: &mut Attributes,
) -> Result<(), BuildError> {
    let mut ordered = templates.to_vec();
    ordered.sort_by_key(|template| (template.priority, &template.id));
    let mut cursor = 0;
    while cursor < ordered.len() {
        let end =
            cursor + ordered[cursor..].partition_point(|t| t.priority == ordered[cursor].priority);
        let mut change_kind = None;
        let mut overrides = ByAttribute::<Option<u16>>::default();
        let mut additions = BTreeSet::new();
        let mut removals = BTreeSet::new();
        let mut adjustments = ByAttribute::<i16>::default();
        for template in &ordered[cursor..end] {
            if let Some(change) = &template.kind {
                if change_kind
                    .as_ref()
                    .is_some_and(|previous| previous != change)
                {
                    return Err(BuildError::ConflictingTemplates);
                }
                change_kind = Some(change.clone());
            }
            for attribute in Attribute::ALL {
                if let Some(value) = template.overrides.get(attribute) {
                    let previous = overrides.get_mut(attribute);
                    if previous.is_some_and(|previous| previous != *value) {
                        return Err(BuildError::ConflictingTemplates);
                    }
                    *previous = Some(*value);
                }
                let adjustment = adjustments.get_mut(attribute);
                *adjustment = adjustment
                    .checked_add(*template.adjustments.get(attribute))
                    .ok_or(BuildError::DerivedLimit)?;
            }
            additions.extend(template.add_subtypes.iter().cloned());
            removals.extend(template.remove_subtypes.iter().cloned());
        }
        if !additions.is_disjoint(&removals) {
            return Err(BuildError::ConflictingTemplates);
        }
        *attributes = attributes
            .adjusted(adjustments)
            .map_err(|_| BuildError::DerivedLimit)?;
        let mut values = attributes.values();
        for attribute in Attribute::ALL {
            if let Some(value) = overrides.get(attribute) {
                *values.get_mut(attribute) = *value;
            }
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

#[cfg(test)]
mod edge_tests {
    use super::*;
    fn modifier(scope: Scope, set: bool, value: i64) -> Modifier {
        Modifier {
            scope,
            op: if set {
                ModifierOp::Set {
                    stat: Stat::Edge,
                    value,
                }
            } else {
                ModifierOp::Add {
                    stat: Stat::Edge,
                    value,
                }
            },
        }
    }
    #[test]
    fn edge_uses_the_same_set_after_add_fold_regardless_of_source_placement() {
        for reversed in [false, true] {
            let mut species = crate::test_creatures::species();
            species.modifiers.push(modifier(
                Scope::Always,
                !reversed,
                if reversed { 2 } else { 1 },
            ));
            let build = crate::test_creatures::build(species);
            let extra = BTreeMap::from([(
                SourceId::Equipment(crate::ItemId(10)),
                vec![modifier(
                    Scope::Always,
                    reversed,
                    if reversed { 1 } else { 2 },
                )],
            )]);
            let derived = derive_with_sources(&build, &extra).unwrap();
            assert_eq!(derived.edge_against(crate::ActorId(9)).balance(), 1);
        }
    }
    #[test]
    fn scoped_edge_conflicts_reject_condition_application_and_restore_atomically() {
        use crate::content_ids::ConditionId;
        use crate::rules::{ConditionDef, Eviction, Stacking};
        let original = crate::test_creatures::build(crate::test_creatures::species());
        let mut document = original.ruleset().document().clone();
        for (id, value) in [("edge_one", 1), ("edge_two", 2)] {
            document.conditions.push(ConditionDef {
                id: ConditionId::new(id).unwrap(),
                name: id.into(),
                modifiers: vec![modifier(Scope::VersusSource, true, value)],
                stacking: Stacking::Longest,
                source_scoped: true,
                descriptors: vec![],
                capacity: Eviction::Reject,
            });
        }
        let content = super::super::Content::new(
            Ruleset::validate(document).unwrap(),
            (*original.content().catalog).clone(),
        )
        .unwrap();
        let build = CreatureBuild::new(
            content,
            original.species_id().clone(),
            original.ledger().clone(),
            original.binding(),
        )
        .unwrap();
        let mut creature = super::super::CreatureState::new(build).unwrap();
        let first = ConditionId::new("edge_one").unwrap();
        let second = ConditionId::new("edge_two").unwrap();
        let source = crate::ActorId(9);
        creature.apply_condition(&first, source, 100).unwrap();
        let before = creature.clone();
        assert_eq!(
            creature.apply_condition(&second, source, 100),
            Err(crate::conditions::ConditionError::InvalidModifiers)
        );
        assert_eq!(creature, before);
        assert_eq!(creature.derived().edge_against(source).balance(), 1);
        assert_eq!(
            creature
                .derived()
                .edge_against(crate::ActorId(10))
                .balance(),
            0
        );
        let mut forged = creature.conditions().clone();
        forged
            .apply(
                creature.build().ruleset().condition(&second).unwrap(),
                source,
                100,
                false,
            )
            .unwrap();
        assert!(super::super::CreatureState::from_recorded(
            creature.build().clone(),
            0,
            false,
            creature.costs().clone(),
            forged,
            BTreeMap::new()
        )
        .is_err());
        creature
            .apply_condition(&second, crate::ActorId(10), 100)
            .unwrap();
        assert_eq!(
            creature
                .derived()
                .edge_against(crate::ActorId(10))
                .balance(),
            2
        );
        assert_eq!(creature.derived().edge_against(source).balance(), 1);
    }
}
