use super::{BuildError, CreatureBuild, EquipmentSource, SourceId, Template};
use crate::attributes::{Attribute, Attributes, ByAttribute, Defenses, SkillRanks};
use crate::content_ids::{
    AbilityId, ClassId, ConditionFamilyId, FlagId, SubtypeId, TalentId, TypeId,
};
use crate::damage::Protection;
use crate::progression::HdSource;
use crate::resources::{Resource, ResourceMaxima};
use crate::rules::{
    AbilityStat, CapacityAttribute, DefenseKind, Modifier, ModifierOp, Ruleset, Scope,
    SkillSelector, Stat,
};
use crate::{AnatomySpec, HdCount};
use std::collections::{BTreeMap, BTreeSet};
use tor_world::Shared;

#[cfg(test)]
thread_local! {
    static FOLD_COUNT: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static EDGE_SCANS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Thread-local deltas isolate deterministic work assertions from parallel tests.
#[cfg(test)]
pub(super) fn fold_count() -> usize {
    FOLD_COUNT.with(std::cell::Cell::get)
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AbilityValues {
    pub hold_damage_increment: u32,
    pub dice: u16,
    pub flat: i32,
    pub check_bonus: i32,
    pub difficulty: i32,
    pub duration: u64,
    pub range: i32,
    pub preparation: i64,
    pub recovery: i64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AbilityTiming {
    pub preparation: u64,
    pub recovery: u64,
}

/// The selected base attack and the exact cached operands used by its check.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectedAttack {
    pub source: SourceId,
    pub socket: Option<crate::EquipmentSlotId>,
    pub attack: crate::attacks::SelectedMeleeAttack,
    pub attribute: Attribute,
    pub attribute_value: u16,
    pub rank: u8,
    pub check_bonus: i64,
}

/// Both participants in one check share the family and talent edge limits.
pub struct CheckContext<'a> {
    pub ability: Option<&'a AbilityId>,
    /// Authored dice role, independent of source/target identity aliasing.
    pub roller_role: crate::rules::CheckActor,
    pub roller: crate::ActorId,
    pub opponent: crate::ActorId,
    pub defense: Option<DefenseKind>,
}

/// One decision's immutable incoming-family baseline. Candidate comparison never
/// rebuilds a creature or rescans all of its sources.
type FamilyValues = BTreeMap<ConditionFamilyId, i64>;
pub(crate) struct IncomingConditionEdges {
    families: BTreeMap<Option<DefenseKind>, (i64, FamilyValues)>,
}
impl IncomingConditionEdges {
    pub(crate) fn improves(&self, condition: &crate::rules::ConditionDef) -> Option<bool> {
        let mut improves = false;
        for (defense, (total, families)) in &self.families {
            let before = families.get(&condition.family).copied().unwrap_or(0);
            let after = condition
                .modifiers
                .iter()
                .try_fold(before, |value, modifier| match &modifier.op {
                    ModifierOp::Add {
                        stat: Stat::IncomingEdge { defense: selected },
                        value: amount,
                    } if modifier.scope == Scope::Always
                        && (selected.is_none() || selected == defense) =>
                    {
                        value.checked_add(*amount)
                    }
                    _ => Some(value),
                })?;
            let projected = total
                .checked_sub(before.signum())?
                .checked_add(after.signum())?;
            if after.unsigned_abs() > crate::limits::NUMERIC as u64
                || projected.unsigned_abs() > crate::limits::NUMERIC as u64
            {
                return None;
            }
            improves |= projected < *total;
        }
        Some(improves)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DormantReason {
    InactiveParent(TalentId),
    /// The immutable talent definition supplies the failed requirement text.
    Requirement,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HitDiceSummary {
    pub total: HdCount,
    pub racial: HdCount,
    /// One row per owned class, independent of the number of its HD records.
    pub classes: BTreeMap<ClassId, HdCount>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DerivedCreature {
    pub perception: crate::perception::PerceptionCapabilities,
    pub locomotion: crate::locomotion::LocomotionCapabilities,
    pub kind: TypeId,
    pub subtypes: BTreeSet<SubtypeId>,
    pub attributes: Attributes,
    pub skills: SkillRanks,
    pub anatomy: AnatomySpec,
    /// Species-owned runtime metadata, refreshed only with permanent inputs.
    pub(crate) behavior: crate::behavior::BehaviorPolicy,
    pub(crate) displacement: Option<super::DisplacementProfile>,
    pub maximum_health: u64,
    pub health_recovery_period: u64,
    pub quiet_health_recovery_period: u64,
    contextual_recovery: bool,
    pub health_recovery_reset_percent: u8,
    /// Exact sampled ordinals, shared across effective folds and snapshots.
    pub health_contributions: Shared<Vec<crate::progression::HealthContribution>>,
    health_sum: u64,
    binding: crate::attributes::ManaBinding,
    base_health_recovery_period: u64,
    pub hit_dice: Shared<HitDiceSummary>,
    pub resources: ResourceMaxima,
    pub defenses: Defenses,
    pub selected_attack: SelectedAttack,
    /// Species-owned source and current check operands, independent of equipment.
    pub natural_attack: SelectedAttack,
    pub active_talents: BTreeSet<TalentId>,
    pub dormant_talents: BTreeSet<TalentId>,
    pub dormant_reasons: BTreeMap<TalentId, Vec<DormantReason>>,
    pub abilities: BTreeSet<AbilityId>,
    pub ability_values: BTreeMap<AbilityId, AbilityValues>,
    pub ability_reaches: BTreeMap<AbilityId, crate::rules::Reach>,
    pub ability_timings: BTreeMap<AbilityId, AbilityTiming>,
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
    pub fn binding(&self) -> crate::attributes::ManaBinding {
        self.binding
    }
    pub fn total_hd(&self) -> HdCount {
        self.hit_dice.total
    }
    pub fn racial_hd(&self) -> HdCount {
        self.hit_dice.racial
    }
    pub fn class_levels(&self) -> &BTreeMap<ClassId, HdCount> {
        &self.hit_dice.classes
    }
    pub fn class_level(&self, class: &ClassId) -> HdCount {
        self.hit_dice
            .classes
            .get(class)
            .copied()
            .unwrap_or(HdCount::ZERO)
    }
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
    /// Read the validated effective reach captured by the source fold.
    pub fn ability_reach(&self, ability: &AbilityId) -> Result<crate::rules::Reach, BuildError> {
        self.ability_reaches
            .get(ability)
            .cloned()
            .ok_or(BuildError::InvalidDefinition)
    }
    /// Tactical modifiers are selected for the check before families and talent
    /// signs collapse. This includes inherited flag contributions in their owner.
    pub fn check_edge(
        &self,
        rules: &Ruleset,
        target: crate::ActorId,
        ability: &AbilityId,
    ) -> Result<i64, BuildError> {
        edge_value(&self.sources, rules, target, Some(ability), None)
    }
    pub fn incoming_check_edge(
        &self,
        rules: &Ruleset,
        source: crate::ActorId,
        defense: Option<DefenseKind>,
    ) -> Result<i64, BuildError> {
        edge_value(&self.sources, rules, source, None, Some(defense))
    }
    /// Reuse the ordinary edge fold's own family operands for all check kinds.
    /// The source is a disclosed actor identity; no opponent statistics are read.
    pub(crate) fn incoming_condition_edges(
        &self,
        rules: &Ruleset,
        source: crate::ActorId,
    ) -> Result<IncomingConditionEdges, BuildError> {
        let mut families = BTreeMap::new();
        for defense in [
            None,
            Some(DefenseKind::Physical),
            Some(DefenseKind::Cognitive),
            Some(DefenseKind::Spiritual),
        ] {
            crate::diagnostics::ai_defense_source_scan();
            let (total, baseline) = fold_edges_with_families(
                rules,
                EdgeUse::Check,
                [EdgeContext {
                    sources: &self.sources,
                    role: None,
                    opponent: source,
                    ability: None,
                    incoming: Some(defense),
                    tokens: None,
                    policy: crate::rules::OutgoingEdgePolicy::Ordinary,
                }],
            )?;
            families.insert(
                defense,
                (
                    total,
                    baseline
                        .into_iter()
                        .map(|(family, value)| value.value(0).map(|value| (family, value)))
                        .collect::<Result<_, _>>()?,
                ),
            );
        }
        Ok(IncomingConditionEdges { families })
    }
    pub fn check_edge_with(
        &self,
        opposing: &Self,
        rules: &Ruleset,
        context: CheckContext<'_>,
    ) -> Result<i64, BuildError> {
        fold_edges(
            rules,
            EdgeUse::Check,
            [
                EdgeContext {
                    sources: &self.sources,
                    role: Some(context.roller_role),
                    opponent: context.opponent,
                    ability: context.ability,
                    incoming: None,
                    tokens: None,
                    policy: crate::rules::OutgoingEdgePolicy::Ordinary,
                },
                EdgeContext {
                    sources: &opposing.sources,
                    role: None,
                    opponent: context.roller,
                    ability: None,
                    incoming: Some(context.defense),
                    tokens: None,
                    policy: crate::rules::OutgoingEdgePolicy::Ordinary,
                },
            ],
        )
    }
    /// Site selection precedes family and talent caps. This never subtracts a
    /// shared or token total from an already capped combined check value.
    pub(crate) fn check_edge_with_selection(
        &self,
        opposing: &Self,
        rules: &Ruleset,
        context: CheckContext<'_>,
        shared: bool,
        policy: crate::rules::OutgoingEdgePolicy,
        selected: &[crate::conditions::TokenReservation],
    ) -> Result<i64, BuildError> {
        fold_edges(
            rules,
            if shared {
                EdgeUse::Check
            } else {
                EdgeUse::CheckOnly
            },
            [
                EdgeContext {
                    sources: &self.sources,
                    role: Some(context.roller_role),
                    opponent: context.opponent,
                    ability: context.ability,
                    incoming: None,
                    tokens: Some((context.roller, selected)),
                    policy,
                },
                EdgeContext {
                    sources: &opposing.sources,
                    role: None,
                    opponent: context.roller,
                    ability: None,
                    incoming: Some(context.defense),
                    tokens: Some((context.opponent, selected)),
                    policy: crate::rules::OutgoingEdgePolicy::Ordinary,
                },
            ],
        )
    }
    /// The separately frozen shared budget keeps its magnitude for effects.
    /// Per-check talent and family caps do not limit an authored damage budget.
    pub fn effect_edge_against(
        &self,
        rules: &Ruleset,
        target: crate::ActorId,
    ) -> Result<i64, BuildError> {
        self.effect_edge_against_with_policy(
            rules,
            target,
            crate::rules::OutgoingEdgePolicy::Ordinary,
        )
    }
    pub(crate) fn effect_edge_against_with_policy(
        &self,
        rules: &Ruleset,
        target: crate::ActorId,
        policy: crate::rules::OutgoingEdgePolicy,
    ) -> Result<i64, BuildError> {
        fold_edges(
            rules,
            EdgeUse::Shared,
            [EdgeContext {
                sources: &self.sources,
                role: None,
                opponent: target,
                ability: None,
                incoming: None,
                tokens: None,
                policy,
            }],
        )
    }
    /// Pure generic outgoing check preview, including unclaimed next-check edge.
    /// Action planning still captures ability context and claims tokens separately.
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
    fn total_hd(&self) -> HdCount {
        self.derived.total_hd()
    }
    fn racial_hd(&self) -> HdCount {
        self.derived.racial_hd()
    }
    fn class_level(&self, class: &ClassId) -> HdCount {
        self.derived.class_level(class)
    }
    fn creature_type(&self) -> &TypeId {
        &self.derived.kind
    }
    fn has_subtype(&self, subtype: &SubtypeId) -> bool {
        self.derived.subtypes.contains(subtype)
    }
    fn species_has_flag(&self, flag: &FlagId) -> bool {
        self.derived
            .sources
            .get(&SourceId::Species(self.build.species_id().clone()))
            .is_some_and(|modifiers| {
                modifiers.iter().any(|modifier| {
                matches!(&modifier.op, ModifierOp::Flag { flag: actual } if actual == flag)
            })
            })
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

/// Validate owned purchases independently of mutable templates or current initial
/// attributes. Blueprint validation uses zero initial attributes; actual edits
/// and restoration validate the current attributes through full derivation.
pub(super) fn advancement_facts(
    build: &CreatureBuild,
    mut attributes: Attributes,
) -> Result<(Attributes, SkillRanks, BTreeSet<TalentId>), BuildError> {
    let rules = build.ruleset();
    if build.choices().len() != build.ledger().entries().len() {
        return Err(BuildError::InvalidOwnership);
    }
    if Attribute::ALL
        .into_iter()
        .any(|a| attributes.get(a) > rules.constants().attribute_cap)
    {
        return Err(BuildError::AttributeCap);
    }
    let mut skills = SkillRanks::default();
    let [first, second] = *build.starting_skills();
    if first == second {
        return Err(BuildError::InvalidStartingSkills);
    }
    for skill in [first, second] {
        skills = skills
            .with_rank(skill, 1)
            .map_err(|_| BuildError::RankCap)?;
    }
    let mut selected = BTreeSet::new();
    for (index, choice) in build.choices().iter().enumerate() {
        crate::diagnostics::advancement_choice_visit();
        build.point_balances(crate::HdIndex::from_usize(index).map_err(BuildError::from)?)?;
        for skill in choice.skills() {
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
        for purchase in &choice.purchases {
            purchase.selection.validate(rules)?;
            let Some(talent) = purchase.selection.talent() else {
                continue;
            };
            if !selected.insert(talent.clone()) {
                return Err(BuildError::DuplicateTalent);
            }
            if selected.len() > crate::limits::OWNED_TALENTS {
                return Err(BuildError::TooManyOwnedTalents);
            }
        }
    }
    Ok((attributes, skills, selected))
}

pub(super) fn derive(build: &CreatureBuild) -> Result<DerivedCreature, BuildError> {
    derive_with_sources(build, &BTreeMap::new(), &[])
}
pub(crate) fn derive_with_sources(
    build: &CreatureBuild,
    extra: &BTreeMap<SourceId, Vec<Modifier>>,
    equipment: &[EquipmentSource],
) -> Result<DerivedCreature, BuildError> {
    derive_with_losses(build, extra, equipment, super::RestorableLosses::default())
}

pub(crate) fn derive_with_losses(
    build: &CreatureBuild,
    extra: &BTreeMap<SourceId, Vec<Modifier>>,
    equipment: &[EquipmentSource],
    losses: super::RestorableLosses,
) -> Result<DerivedCreature, BuildError> {
    PermanentCreature::compile(build)?.fold(build, extra, equipment, losses)
}

/// Immutable permanent inputs, deliberately not a condition-free *effective*
/// sheet. Dynamic folds always start from these unmodified attributes/sources,
/// so Set/Add/protection and selected weapons still use one ordinary pipeline.
/// Neither this cache nor its sampled provenance is persisted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PermanentCreature {
    seed: DerivedCreature,
    threat_tier: u64,
}
impl PermanentCreature {
    pub(super) fn compile(build: &CreatureBuild) -> Result<Self, BuildError> {
        crate::diagnostics::permanent_creature_compile();
        let seed = compile_permanent(build)?;
        let threat_tier = crate::earned_growth::assess(build, &seed)?.tier;
        Ok(Self { seed, threat_tier })
    }
    pub(super) fn threat_tier(&self) -> u64 {
        self.threat_tier
    }
    pub(super) fn fold(
        &self,
        build: &CreatureBuild,
        extra: &BTreeMap<SourceId, Vec<Modifier>>,
        equipment: &[EquipmentSource],
        losses: super::RestorableLosses,
    ) -> Result<DerivedCreature, BuildError> {
        if !losses.valid() {
            return Err(BuildError::InvalidDefinition);
        }
        #[cfg(test)]
        FOLD_COUNT.with(|count| count.set(count.get() + 1));
        crate::diagnostics::dynamic_creature_fold();
        let mut derived = self.seed.clone();
        // Socket order alone chooses the weapon, independent of source order.
        for attack in equipment.iter().filter_map(|entry| entry.attack.as_ref()) {
            build
                .ruleset()
                .validate_damage(attack.damage())
                .map_err(|_| BuildError::InvalidDefinition)?;
        }
        if let Some((socket, item, attack)) = equipment
            .iter()
            .filter_map(|entry| {
                entry
                    .attack
                    .as_ref()
                    .map(|attack| (entry.socket, entry.item, attack))
            })
            .max_by_key(|(socket, _, _)| *socket)
        {
            let attack = crate::attacks::SelectedMeleeAttack::Weapon(attack.clone());
            derived.selected_attack = SelectedAttack {
                attribute: attack.skill().attribute(derived.binding),
                attribute_value: 0,
                rank: 0,
                check_bonus: 0,
                attack,
                source: SourceId::Equipment(item),
                socket: Some(socket),
            };
        }
        for (source, modifiers) in extra {
            add_source(
                &mut derived.sources,
                source.clone(),
                modifiers,
                build.ruleset(),
            )?;
        }
        fold(&mut derived, build, losses)?;
        if derived.total_hd() == HdCount::ZERO {
            derived.maximum_health = 0;
            derived.abilities.clear();
            derived.ability_reaches.clear();
            derived.ability_timings.clear();
        }
        Ok(derived)
    }
}

fn compile_permanent(build: &CreatureBuild) -> Result<DerivedCreature, BuildError> {
    let rules = build.ruleset();
    build
        .ledger()
        .advancement_points(rules)
        .map_err(|error| match error {
            // Racial grants always exist; only an unknown class can miss here.
            crate::progression::HdError::UnknownSource => BuildError::UnknownClass,
            other => BuildError::from(other),
        })?;
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
    let (mut attributes, skills, selected) = advancement_facts(build, build.initial_attributes())?;
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
    let mut racial_hd = HdCount::ZERO;
    let mut class_levels = BTreeMap::new();
    for entry in build.ledger().entries() {
        crate::diagnostics::raw_hit_die_visit();
        match entry.source() {
            HdSource::Racial => {
                racial_hd = racial_hd.checked_add(1).ok_or(BuildError::Arithmetic)?
            }
            HdSource::Class(class) => {
                let count = class_levels.entry(class).or_insert(HdCount::ZERO);
                *count = count.checked_add(1).ok_or(BuildError::Arithmetic)?;
            }
        }
    }
    for class in class_levels.keys() {
        add_source(
            &mut sources,
            SourceId::Class(class.clone()),
            &rules
                .class(class)
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
    let permanent_attributes =
        attribute_values(attributes, &collect_stats(&sources, rules)?, rules)?;
    let attack = crate::attacks::SelectedMeleeAttack::Natural(species.melee.clone());
    let selected_attack = SelectedAttack {
        attribute: attack.skill().attribute(build.binding()),
        attribute_value: 0,
        rank: 0,
        check_bonus: 0,
        attack,
        source: SourceId::Species(species.id.clone()),
        socket: None,
    };
    let health_contributions = build
        .ledger()
        .health_provenance(&kind, rules)
        .map_err(BuildError::from)?;
    let health_sum = health_contributions.iter().try_fold(0_u64, |sum, entry| {
        sum.checked_add(u64::from(entry.health))
            .ok_or(BuildError::Arithmetic)
    })?;
    let mut derived = DerivedCreature {
        locomotion: Default::default(),
        perception: Default::default(),
        kind,
        subtypes,
        attributes,
        skills,
        anatomy: species.anatomy.clone(),
        behavior: species.behavior,
        displacement: species.displacement,
        maximum_health: 0,
        health_recovery_period: 1,
        quiet_health_recovery_period: 1,
        contextual_recovery: false,
        health_recovery_reset_percent: 100,
        health_contributions: Shared::new(health_contributions),
        health_sum,
        binding: build.binding(),
        base_health_recovery_period: rules
            .document()
            .health_recovery
            .period(build.ledger().total_hd()),
        hit_dice: Shared::new(HitDiceSummary {
            total: build.ledger().total_hd(),
            racial: racial_hd,
            classes: class_levels,
        }),
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
        natural_attack: selected_attack.clone(),
        selected_attack,
        active_talents: BTreeSet::new(),
        dormant_talents: BTreeSet::new(),
        dormant_reasons: BTreeMap::new(),
        abilities: rules
            .constants()
            .default_abilities
            .iter()
            .cloned()
            .collect(),
        ability_values: BTreeMap::new(),
        ability_reaches: BTreeMap::new(),
        ability_timings: BTreeMap::new(),
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
    for id in &derived.dormant_talents {
        let definition = rules.talent(id).ok_or(BuildError::UnknownTalent)?;
        let mut reasons: Vec<_> = definition
            .prerequisites
            .iter()
            .filter(|parent| !derived.active_talents.contains(*parent))
            .cloned()
            .map(DormantReason::InactiveParent)
            .collect();
        if !definition.requirement.satisfied(
            &PermanentFacts::new(build, &derived),
            &derived.active_talents,
        ) {
            reasons.push(DormantReason::Requirement);
        }
        derived.dormant_reasons.insert(id.clone(), reasons);
    }

    for purchase in build.choices().iter().flat_map(|choice| {
        crate::diagnostics::advancement_choice_visit();
        &choice.purchases
    }) {
        let Some(talent) = purchase.selection.talent() else {
            continue;
        };
        if !derived.active_talents.contains(talent) {
            continue;
        }
        let definition = rules.talent(talent).ok_or(BuildError::UnknownTalent)?;
        let mut modifiers = std::borrow::Cow::Borrowed(definition.modifiers.as_slice());
        if let Some(form) = purchase.selection.form() {
            // Registry validation reserves this one slot. It remains under the
            // original root source and uses the ordinary ability grant pipeline.
            let definition = rules.form(form).ok_or(BuildError::InvalidTalentForm)?;
            modifiers.to_mut().push(Modifier {
                scope: Scope::Always,
                op: ModifierOp::GrantAbility {
                    ability: definition.ability.clone(),
                },
            });
        }
        add_source(
            &mut derived.sources,
            SourceId::Talent(talent.clone()),
            &modifiers,
            rules,
        )?;
    }
    Ok(derived)
}

fn add_source(
    sources: &mut BTreeMap<SourceId, Vec<Modifier>>,
    source: SourceId,
    modifiers: &[Modifier],
    rules: &Ruleset,
) -> Result<(), BuildError> {
    (if matches!(
        source,
        SourceId::Condition(..) | SourceId::QualifiedCondition { .. }
    ) {
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
    fn add(&mut self, value: i64) -> Result<(), BuildError> {
        self.added = self
            .added
            .checked_add(value)
            .ok_or(BuildError::Arithmetic)?;
        Ok(())
    }
    fn set(&mut self, value: i64) -> Result<(), BuildError> {
        if self.set.is_some_and(|previous| previous != value) {
            return Err(BuildError::ConflictingOverride);
        }
        self.set = Some(value);
        Ok(())
    }
    /// Aggregate Health has its own wide domain; individual modifier operands
    /// still pass the same bounded source validation and Set conflict checks.
    fn health(&self, sampled: u64, strength: u16) -> Result<u64, BuildError> {
        let result = match self.set {
            Some(value) => i128::from(value),
            None => i128::from(sampled)
                .checked_add(i128::from(strength))
                .and_then(|base| base.checked_add(i128::from(self.added)))
                .ok_or(BuildError::Arithmetic)?,
        };
        u64::try_from(result.max(0)).map_err(|_| BuildError::Arithmetic)
    }
    fn value(&self, base: i64) -> Result<i64, BuildError> {
        let result = self
            .set
            .unwrap_or(base.checked_add(self.added).ok_or(BuildError::Arithmetic)?);
        if !(-crate::limits::NUMERIC..=crate::limits::NUMERIC).contains(&result) {
            return Err(BuildError::DerivedLimit);
        }
        Ok(result)
    }
}
fn fold_stat_targets(
    target: &Stat,
    rules: &Ruleset,
    mut apply: impl FnMut(Stat) -> Result<(), BuildError>,
) -> Result<(), BuildError> {
    match target {
        Stat::AbilityTag { tag, stat } => {
            for ability in rules
                .abilities_with_tag(tag)
                .ok_or(BuildError::InvalidDefinition)?
            {
                apply(Stat::Ability {
                    ability: ability.clone(),
                    stat: *stat,
                })?;
            }
        }
        Stat::Attribute { .. }
        | Stat::Defense { .. }
        | Stat::Health
        | Stat::HealthRecoveryPeriod
        | Stat::HealthRecoveryPeriodPercent
        | Stat::HealthRecoveryResetPercent
        | Stat::ResourceMax { .. }
        | Stat::Ability { .. }
        | Stat::Edge
        | Stat::CheckEdge
        | Stat::IncomingEdge { .. } => apply(target.clone())?,
    }
    Ok(())
}
fn recovery_period(
    base_period: u64,
    values: &BTreeMap<Stat, Accumulator>,
) -> Result<u64, BuildError> {
    let base = value(values, Stat::HealthRecoveryPeriod, base_period as i64)?.max(1);
    let percent = value(values, Stat::HealthRecoveryPeriodPercent, 100)?;
    if !(1..=10_000).contains(&percent) {
        return Err(BuildError::DerivedLimit);
    }
    let period =
        (u64::try_from(base).map_err(|_| BuildError::DerivedLimit)? * percent as u64).div_ceil(100);
    if period > crate::limits::NUMERIC as u64 {
        return Err(BuildError::DerivedLimit);
    }
    Ok(period)
}
impl DerivedCreature {
    pub fn contextual_health_recovery(&self) -> bool {
        self.contextual_recovery
    }
    pub(crate) fn recovery_period_in_context(&self, sensed_hostile: bool) -> u64 {
        if sensed_hostile {
            self.health_recovery_period
        } else {
            self.quiet_health_recovery_period
        }
    }
}

fn collect_stats(
    sources: &BTreeMap<SourceId, Vec<Modifier>>,
    rules: &Ruleset,
) -> Result<BTreeMap<Stat, Accumulator>, BuildError> {
    let mut values: BTreeMap<Stat, Accumulator> = BTreeMap::new();
    for modifier in sources
        .values()
        .flatten()
        .filter(|m| m.scope == Scope::Always)
    {
        match &modifier.op {
            ModifierOp::Add { stat, .. } | ModifierOp::Set { stat, .. }
                if matches!(
                    stat,
                    Stat::Edge
                        | Stat::CheckEdge
                        | Stat::IncomingEdge { .. }
                        | Stat::Ability {
                            stat: AbilityStat::CheckEdge,
                            ..
                        }
                        | Stat::AbilityTag {
                            stat: AbilityStat::CheckEdge,
                            ..
                        }
                ) => {}
            ModifierOp::Add { stat, value } => fold_stat_targets(stat, rules, |stat| {
                values.entry(stat).or_default().add(*value)
            })?,
            ModifierOp::Set { stat, value } => fold_stat_targets(stat, rules, |stat| {
                values.entry(stat).or_default().set(*value)
            })?,
            ModifierOp::Flag { .. }
            | ModifierOp::Immunity { .. }
            | ModifierOp::Reduction { .. }
            | ModifierOp::TalentReduction { .. }
            | ModifierOp::GrantAbility { .. }
            | ModifierOp::Locomotion { .. }
            | ModifierOp::Perception { .. } => {}
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
/// One deterministic fold for outgoing, ability-specific and incoming check
/// edge. Families are independent of causer and instance count, but selection
/// precedes deduplication so Fear still applies only against its causer.
fn edge_value(
    sources: &BTreeMap<SourceId, Vec<Modifier>>,
    rules: &Ruleset,
    opponent: crate::ActorId,
    ability: Option<&AbilityId>,
    incoming: Option<Option<DefenseKind>>,
) -> Result<i64, BuildError> {
    fold_edges(
        rules,
        EdgeUse::Check,
        [EdgeContext {
            sources,
            role: incoming.is_none().then_some(crate::rules::CheckActor::User),
            opponent,
            ability,
            incoming,
            tokens: None,
            policy: crate::rules::OutgoingEdgePolicy::Ordinary,
        }],
    )
}
struct EdgeContext<'a> {
    sources: &'a BTreeMap<SourceId, Vec<Modifier>>,
    role: Option<crate::rules::CheckActor>,
    opponent: crate::ActorId,
    ability: Option<&'a AbilityId>,
    incoming: Option<Option<DefenseKind>>,
    tokens: Option<(crate::ActorId, &'a [crate::conditions::TokenReservation])>,
    policy: crate::rules::OutgoingEdgePolicy,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum EdgeUse {
    Check,
    CheckOnly,
    Shared,
}
fn fold_edges<'a>(
    rules: &Ruleset,
    usage: EdgeUse,
    contexts: impl IntoIterator<Item = EdgeContext<'a>>,
) -> Result<i64, BuildError> {
    fold_edges_with_families(rules, usage, contexts).map(|(value, _)| value)
}
type EdgeFamilies = BTreeMap<ConditionFamilyId, Accumulator>;
fn fold_edges_with_families<'a>(
    rules: &Ruleset,
    usage: EdgeUse,
    contexts: impl IntoIterator<Item = EdgeContext<'a>>,
) -> Result<(i64, EdgeFamilies), BuildError> {
    #[cfg(test)]
    EDGE_SCANS.with(|count| count.set(count.get() + 1));
    let mut ordinary: BTreeMap<(usize, Stat), Accumulator> = BTreeMap::new();
    let mut positive_talent = false;
    let mut negative_talent = false;
    let mut families: BTreeMap<crate::content_ids::ConditionFamilyId, Accumulator> =
        BTreeMap::new();
    for (
        participant,
        EdgeContext {
            sources,
            role,
            opponent,
            ability,
            incoming,
            tokens,
            policy,
        },
    ) in contexts.into_iter().enumerate()
    {
        for (source, modifiers) in sources {
            let condition_source = match source {
                SourceId::Condition(condition, causer) => Some((condition, causer, None)),
                SourceId::QualifiedCondition {
                    condition,
                    source,
                    qualification,
                } => {
                    // Filter before family deduplication, including previews
                    // without a selected token list. A wrong foe or nonattack
                    // gets neither an edge nor a preparation reservation.
                    if role != Some(crate::rules::CheckActor::User)
                        || incoming.is_some()
                        || !qualification.matches(opponent, ability, rules)
                    {
                        continue;
                    }
                    Some((condition, source, Some(qualification)))
                }
                _ => None,
            };
            if let Some((condition, causer, qualification)) = condition_source {
                let definition = rules
                    .condition(condition)
                    .ok_or(BuildError::InvalidDefinition)?;
                if definition.lifecycle != crate::rules::ConditionLifecycle::Timed
                    && (usage == EdgeUse::Shared
                        || tokens.is_some_and(|(holder, selected)| {
                            !selected.iter().any(|token| {
                                token.holder == holder
                                    && &token.condition == condition
                                    && token.source == *causer
                                    && token.qualification.as_ref() == qualification
                            })
                        }))
                {
                    continue;
                }
            }
            for modifier in modifiers {
                if modifier.scope == Scope::VersusSource
                    && !matches!(source, SourceId::Condition(_, causer) if *causer == opponent)
                {
                    continue;
                }
                let (stat, amount, set) = match &modifier.op {
                    ModifierOp::Add { stat, value } => (stat, *value, false),
                    ModifierOp::Set { stat, value } => (stat, *value, true),
                    _ => continue,
                };
                let applies = match stat {
                    Stat::Edge => incoming.is_none() && usage != EdgeUse::CheckOnly,
                    Stat::CheckEdge => incoming.is_none() && usage != EdgeUse::Shared,
                    Stat::IncomingEdge { defense } => {
                        usage != EdgeUse::Shared
                            && incoming
                                .is_some_and(|checked| defense.is_none() || *defense == checked)
                    }
                    Stat::Ability {
                        ability: selected,
                        stat: AbilityStat::CheckEdge,
                    } => {
                        usage != EdgeUse::Shared && incoming.is_none() && ability == Some(selected)
                    }
                    Stat::AbilityTag {
                        tag,
                        stat: AbilityStat::CheckEdge,
                    } => {
                        usage != EdgeUse::Shared
                            && incoming.is_none()
                            && ability.is_some_and(|ability| {
                                rules
                                    .abilities_with_tag(tag)
                                    .is_some_and(|members| members.contains(ability))
                            })
                    }
                    _ => false,
                };
                if !applies || !policy.allows(amount) {
                    continue;
                }
                let accumulator = match source {
                    SourceId::Talent(_) if usage != EdgeUse::Shared => {
                        positive_talent |= amount > 0;
                        negative_talent |= amount < 0;
                        continue;
                    }
                    SourceId::Condition(id, _)
                    | SourceId::QualifiedCondition { condition: id, .. }
                        if usage != EdgeUse::Shared =>
                    {
                        let definition =
                            rules.condition(id).ok_or(BuildError::InvalidDefinition)?;
                        families.entry(definition.family.clone()).or_default()
                    }
                    _ => {
                        let stat = match stat {
                            Stat::AbilityTag { stat, .. } => Stat::Ability {
                                ability: ability.expect("matching ability tag").clone(),
                                stat: *stat,
                            },
                            stat => stat.clone(),
                        };
                        ordinary.entry((participant, stat)).or_default()
                    }
                };
                if set {
                    accumulator.set(amount)?;
                } else {
                    accumulator.add(amount)?;
                }
            }
        }
    }
    let mut value = i64::from(positive_talent) - i64::from(negative_talent);
    for stat in ordinary.values() {
        value = value
            .checked_add(stat.value(0)?)
            .ok_or(BuildError::Arithmetic)?;
    }
    for family in families.values() {
        value += family.value(0)?.signum();
    }
    if value.unsigned_abs() > crate::limits::NUMERIC as u64 {
        return Err(BuildError::DerivedLimit);
    }
    Ok((value, families))
}

pub(crate) fn scaled_duration(
    base: u64,
    attribute: Option<u16>,
    rules: &Ruleset,
) -> Result<u64, BuildError> {
    let duration = if let Some(attribute) = attribute {
        let numerator = u128::from(rules.constants().duration_scale_numerator);
        let divisor = u128::from(rules.constants().duration_scale_offset) + u128::from(attribute);
        u64::try_from((u128::from(base) * numerator).div_ceil(divisor).max(1))
            .map_err(|_| BuildError::Arithmetic)?
    } else {
        base
    };
    if !(1..=crate::limits::NUMERIC as u64).contains(&duration) {
        return Err(BuildError::DerivedLimit);
    }
    Ok(duration)
}

fn fold(
    derived: &mut DerivedCreature,
    build: &CreatureBuild,
    losses: super::RestorableLosses,
) -> Result<(), BuildError> {
    let rules = build.ruleset();
    let values = collect_stats(&derived.sources, rules)?;
    let edge =
        |value: i64| crate::dice::Edge::from_counts(value.max(0) as u32, (-value).max(0) as u32);
    derived.base_edge = edge(edge_value(
        &derived.sources,
        rules,
        crate::ActorId(0),
        None,
        None,
    )?);
    derived.scoped_edges = derived
        .sources
        .keys()
        .filter_map(|source| match source {
            SourceId::Condition(_, target) => Some(*target),
            _ => None,
        })
        .map(|target| {
            Ok((
                target,
                edge(edge_value(&derived.sources, rules, target, None, None)?),
            ))
        })
        .collect::<Result<_, BuildError>>()?;

    derived.effect_edge_against(rules, crate::ActorId(0))?;
    for target in derived.scoped_edges.keys() {
        derived.effect_edge_against(rules, *target)?;
    }

    derived.attributes = attribute_values(derived.attributes, &values, rules)?;
    derived.flags = flags(&derived.sources);
    derived.locomotion = crate::locomotion::LocomotionCapabilities::from_modifiers(
        derived.sources.values().flatten(),
    );
    derived.perception = crate::perception::PerceptionCapabilities::from_modifiers(
        derived.sources.values().flatten(),
    )?;
    fold_attribute_dependents(derived, build, &values)?;
    derived.health_recovery_period = recovery_period(derived.base_health_recovery_period, &values)?;
    let mut quiet_values = values.clone();
    derived.contextual_recovery = false;
    for modifier in derived
        .sources
        .values()
        .flatten()
        .filter(|m| m.scope == Scope::WhileNoSensedHostile)
    {
        derived.contextual_recovery = true;
        match &modifier.op {
            ModifierOp::Add { stat, value } => {
                quiet_values.entry(stat.clone()).or_default().add(*value)?
            }
            ModifierOp::Set { stat, value } => {
                quiet_values.entry(stat.clone()).or_default().set(*value)?
            }
            _ => unreachable!("validated contextual recovery modifier"),
        }
    }
    derived.quiet_health_recovery_period =
        recovery_period(derived.base_health_recovery_period, &quiet_values)?;
    let reset = value(&values, Stat::HealthRecoveryResetPercent, 100)?;
    if !(0..=100).contains(&reset) {
        return Err(BuildError::DerivedLimit);
    }
    derived.health_recovery_reset_percent = reset as u8;

    for (source, modifiers) in &derived.sources {
        for modifier in modifiers {
            match &modifier.op {
                ModifierOp::GrantAbility { ability } => {
                    derived.abilities.insert(ability.clone());
                }
                ModifierOp::Reduction {
                    selector: crate::rules::Selector::Category { category },
                    amount,
                } if matches!(source, SourceId::Talent(_)) => {
                    derived
                        .protection
                        .apply(&ModifierOp::TalentReduction {
                            category: *category,
                            amount: *amount,
                        })
                        .map_err(|_| BuildError::DerivedLimit)?;
                }
                ModifierOp::Immunity { .. }
                | ModifierOp::Reduction { .. }
                | ModifierOp::TalentReduction { .. } => {
                    derived
                        .protection
                        .apply(&modifier.op)
                        .map_err(|_| BuildError::DerivedLimit)?;
                }
                ModifierOp::Add { .. }
                | ModifierOp::Set { .. }
                | ModifierOp::Flag { .. }
                | ModifierOp::Locomotion { .. }
                | ModifierOp::Perception { .. } => {}
            }
        }
    }
    // Validate tactical override slots when sources change, not later when a
    // player first attempts the affected check. Untargeted checks (including
    // unclaimed tokens) already share the base/scoped fold; only explicitly
    // targeted abilities need extra contexts. Unrelated grants add no scans.
    // Incoming scopes cannot select a causer.
    let mut edge_abilities = BTreeSet::new();
    for modifier in derived.sources.values().flatten() {
        match &modifier.op {
            ModifierOp::Add { stat, .. } | ModifierOp::Set { stat, .. } => match stat {
                Stat::Ability {
                    ability,
                    stat: AbilityStat::CheckEdge,
                } => {
                    edge_abilities.insert(ability.clone());
                }
                Stat::AbilityTag {
                    tag,
                    stat: AbilityStat::CheckEdge,
                } => {
                    edge_abilities
                        .extend(rules.abilities_with_tag(tag).into_iter().flatten().cloned());
                }
                _ => {}
            },
            _ => {}
        }
    }
    for ability in &edge_abilities {
        for opponent in
            std::iter::once(crate::ActorId(0)).chain(derived.scoped_edges.keys().copied())
        {
            derived.check_edge(rules, opponent, ability)?;
        }
    }
    for defense in [
        None,
        Some(DefenseKind::Physical),
        Some(DefenseKind::Cognitive),
        Some(DefenseKind::Spiritual),
    ] {
        derived.incoming_check_edge(rules, crate::ActorId(0), defense)?;
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
                hold_damage_increment: u32::try_from(stat(AbilityStat::HoldDamageIncrement)?)
                    .map_err(|_| BuildError::DerivedLimit)?,
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
        let definition = rules
            .ability(ability)
            .ok_or(BuildError::InvalidDefinition)?;
        let modifiers = derived.ability_values(ability);
        let interaction_error = |error| match error {
            crate::rules::EffectiveInteractionError::DirectHarm => BuildError::InvalidInteraction,
            // Source expressions already passed catalog validation. A composed
            // damage operand can exceed its current numeric/pool bound without
            // becoming malformed content or a direct-harm contradiction.
            crate::rules::EffectiveInteractionError::InvalidDamage => BuildError::DerivedLimit,
        };
        match &definition.strikes {
            crate::rules::StrikeSource::NaturalMelee { additional_riders } => {
                let crate::attacks::SelectedMeleeAttack::Natural(natural) =
                    &derived.natural_attack.attack
                else {
                    return Err(BuildError::InvalidDefinition);
                };
                let strikes =
                    crate::rules::append_natural_riders(natural.strikes(), additional_riders)
                        .map_err(|_| BuildError::InvalidDefinition)?;
                rules
                    .validate_effective_interaction(
                        definition.interaction,
                        &strikes,
                        &modifiers,
                        true,
                        &derived.active_talents,
                    )
                    .map_err(interaction_error)?;
            }
            crate::rules::StrikeSource::Authored { strikes } => {
                rules
                    .validate_effective_interaction(
                        definition.interaction,
                        strikes,
                        &modifiers,
                        false,
                        &derived.active_talents,
                    )
                    .map_err(interaction_error)?;
            }
            crate::rules::StrikeSource::SelectedMelee => {}
        }
        let reach = match definition.reach {
            crate::rules::Reach::MutualSight { range } => {
                let range = i64::from(range) + i64::from(modifiers.range);
                if !(1..=i64::from(crate::limits::SIGHT_RANGE)).contains(&range) {
                    return Err(BuildError::DerivedLimit);
                }
                crate::rules::Reach::MutualSight {
                    range: range as u16,
                }
            }
            crate::rules::Reach::Sight { range } => {
                let range = i64::from(range) + i64::from(modifiers.range);
                if !(1..=i64::from(crate::limits::SIGHT_RANGE)).contains(&range) {
                    return Err(BuildError::DerivedLimit);
                }
                crate::rules::Reach::Sight {
                    range: range as u16,
                }
            }
            crate::rules::Reach::SelfOrAlly {
                range: crate::rules::AllyRange::Sight { range },
            } => {
                let range = i64::from(range) + i64::from(modifiers.range);
                if !(1..=i64::from(crate::limits::SIGHT_RANGE)).contains(&range) {
                    return Err(BuildError::DerivedLimit);
                }
                crate::rules::Reach::SelfOrAlly {
                    range: crate::rules::AllyRange::Sight {
                        range: range as u16,
                    },
                }
            }
            ref reach => reach.clone(),
        };
        derived.ability_reaches.insert(ability.clone(), reach);
        let timing = ability_timing(derived, build, definition, &modifiers)?;
        derived.ability_timings.insert(ability.clone(), timing);
    }
    if losses.has_attribute_loss() {
        // First validate all ordinary computations above. Reduced attributes
        // cannot conceal an invalid undrained Health/resource/timing result.
        derived.attributes = Attributes::from_derived(ByAttribute::from_fn(|attribute| {
            derived
                .attributes
                .get(attribute)
                .saturating_sub(*losses.attributes.get(attribute))
        }))
        .map_err(|_| BuildError::DerivedLimit)?;
        fold_attribute_dependents(derived, build, &values)?;
        fold_ability_timings(derived, build)?;
    }
    derived.maximum_health = derived
        .maximum_health
        .saturating_sub(u64::from(losses.maximum_health));
    Ok(())
}

/// Recompute only values governed by effective attributes, using already
/// validated stat accumulators and the once-collected Health provenance.
fn fold_attribute_dependents(
    derived: &mut DerivedCreature,
    build: &CreatureBuild,
    values: &BTreeMap<Stat, Accumulator>,
) -> Result<(), BuildError> {
    let rules = build.ruleset();
    let attribute = |a| i64::from(derived.attributes.get(a));
    let defense = |kind, left, right| {
        value(
            values,
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
    derived.maximum_health = values
        .get(&Stat::Health)
        .cloned()
        .unwrap_or_default()
        .health(
            derived.health_sum,
            derived.attributes.get(Attribute::Strength),
        )?;
    for definition in &rules.document().resources {
        let enabled = definition
            .capacity
            .required_flag
            .as_ref()
            .is_none_or(|flag| derived.flags.contains(flag));
        let governed = match definition.capacity.attribute {
            CapacityAttribute::Named { attribute } => attribute,
            CapacityAttribute::Binding => derived.binding.attribute(),
        };
        let maximum = if enabled {
            value(
                values,
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
    for selected in [&mut derived.selected_attack, &mut derived.natural_attack] {
        selected.attribute_value = derived.attributes.get(selected.attribute);
        selected.rank = derived.skills.get(selected.attack.skill());
        selected.check_bonus = crate::attributes::SkillCheck {
            skill: selected.attack.skill(),
            binding: derived.binding,
            modifier: selected.attack.bonus(),
            threshold: 0,
        }
        .bonus(derived.attributes, derived.skills);
    }
    Ok(())
}

fn fold_ability_timings(
    derived: &mut DerivedCreature,
    build: &CreatureBuild,
) -> Result<(), BuildError> {
    let rules = build.ruleset();
    for ability in &derived.abilities {
        let definition = rules
            .ability(ability)
            .ok_or(BuildError::InvalidDefinition)?;
        let modifiers = derived.ability_values(ability);
        let timing = ability_timing(derived, build, definition, &modifiers)?;
        derived.ability_timings.insert(ability.clone(), timing);
    }
    Ok(())
}

fn ability_timing(
    derived: &DerivedCreature,
    build: &CreatureBuild,
    definition: &crate::rules::AbilityDef,
    modifiers: &AbilityValues,
) -> Result<AbilityTiming, BuildError> {
    let rules = build.ruleset();
    let source_timing = if matches!(
        definition.strikes,
        crate::rules::StrikeSource::NaturalMelee { .. }
    ) {
        (
            derived.natural_attack.attack.wind_up(),
            derived.natural_attack.attack.recovery(),
        )
    } else {
        (
            derived.selected_attack.attack.wind_up(),
            derived.selected_attack.attack.recovery(),
        )
    };
    let (prepare, recover) = match definition.timing.source {
        crate::rules::TimingSource::Fixed { prepare, recover } => (prepare, recover),
        crate::rules::TimingSource::SelectedWeapon => source_timing,
        crate::rules::TimingSource::SelectedWeaponAdjusted {
            preparation,
            recovery,
        } => (
            source_timing
                .0
                .checked_add(preparation)
                .ok_or(BuildError::Arithmetic)?,
            source_timing
                .1
                .checked_add(recovery)
                .ok_or(BuildError::Arithmetic)?,
        ),
    };
    let duration = |base: u64, adjustment: i64| -> Result<u64, BuildError> {
        let base = u64::try_from((i128::from(base) + i128::from(adjustment)).max(1))
            .map_err(|_| BuildError::Arithmetic)?;
        scaled_duration(
            base,
            definition
                .timing
                .scaled_by
                .map(|attribute| derived.attributes.get(attribute)),
            rules,
        )
    };
    Ok(AbilityTiming {
        preparation: duration(prepare, modifiers.preparation)?,
        recovery: duration(recover, modifiers.recovery)?,
    })
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
                    .ok_or(BuildError::Arithmetic)?;
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
    #[test]
    fn outgoing_edge_policy_selects_before_family_and_shared_talent_sign_caps() {
        let build = crate::test_creatures::build(crate::test_creatures::species());
        let ability = AbilityId::new("basic_melee").unwrap();
        let actor = crate::ActorId(1);
        let target = crate::ActorId(2);
        let owner = derive_with_sources(
            &build,
            &BTreeMap::from([
                (
                    SourceId::Talent(TalentId::new("mixed").unwrap()),
                    vec![
                        modifier(Scope::Always, false, 5),
                        modifier(Scope::Always, false, -2),
                    ],
                ),
                (
                    SourceId::Condition(
                        crate::content_ids::ConditionId::new("fear").unwrap(),
                        target,
                    ),
                    vec![
                        modifier(Scope::VersusSource, false, 4),
                        modifier(Scope::VersusSource, false, -4),
                    ],
                ),
                (
                    SourceId::Condition(
                        crate::content_ids::ConditionId::new("fear").unwrap(),
                        crate::ActorId(3),
                    ),
                    vec![modifier(Scope::VersusSource, false, 20)],
                ),
            ]),
            &[],
        )
        .unwrap();
        let opponent = derive_with_sources(
            &build,
            &BTreeMap::from([
                (
                    SourceId::Talent(TalentId::new("incoming").unwrap()),
                    vec![Modifier {
                        scope: Scope::Always,
                        op: ModifierOp::Add {
                            stat: Stat::IncomingEdge { defense: None },
                            value: -5,
                        },
                    }],
                ),
                (
                    SourceId::Condition(
                        crate::content_ids::ConditionId::new("braced").unwrap(),
                        target,
                    ),
                    vec![Modifier {
                        scope: Scope::Always,
                        op: ModifierOp::Add {
                            stat: Stat::IncomingEdge {
                                defense: Some(DefenseKind::Physical),
                            },
                            value: -1,
                        },
                    }],
                ),
            ]),
            &[],
        )
        .unwrap();
        let sources = owner.sources.clone();
        for (policy, check, shared) in [
            (crate::rules::OutgoingEdgePolicy::Ordinary, -1, 3),
            (crate::rules::OutgoingEdgePolicy::IgnoreNegative, 0, 9),
        ] {
            assert_eq!(
                owner
                    .check_edge_with_selection(
                        &opponent,
                        build.ruleset(),
                        CheckContext {
                            ability: Some(&ability),
                            roller_role: crate::rules::CheckActor::User,
                            roller: actor,
                            opponent: target,
                            defense: Some(DefenseKind::Physical),
                        },
                        true,
                        policy,
                        &[]
                    )
                    .unwrap(),
                check
            );
            assert_eq!(
                owner
                    .effect_edge_against_with_policy(build.ruleset(), target, policy)
                    .unwrap(),
                shared
            );
        }
        assert_eq!(owner.sources, sources, "policy never rewrites provenance");
    }

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
    fn next_check_tokens_participate_in_generic_check_preview_without_shared_edge() {
        let build = crate::test_creatures::build(crate::test_creatures::species());
        let mut creature = super::super::CreatureState::new(build).unwrap();
        creature
            .apply_condition(
                &crate::content_ids::ConditionId::new("readied").unwrap(),
                crate::ActorId(1),
                500,
            )
            .unwrap();
        assert_eq!(
            creature.derived().edge_against(crate::ActorId(2)).balance(),
            1
        );
        assert_eq!(
            creature
                .derived()
                .effect_edge_against(creature.build().ruleset(), crate::ActorId(2))
                .unwrap(),
            0
        );
        assert!(creature
            .conditions()
            .instances()
            .all(|entry| entry.reserved_by.is_none()));
    }

    #[test]
    fn adding_a_check_token_rejects_combined_numeric_overflow_atomically() {
        let build = crate::test_creatures::build(crate::test_creatures::species());
        let mut creature = super::super::CreatureState::new(build).unwrap();
        let equipment = (1..=8)
            .map(|item| EquipmentSource {
                item: crate::ItemId(item),
                socket: crate::EquipmentSlotId(item as u16),
                attack: None,
                modifiers: vec![
                    Modifier {
                        scope: Scope::Always,
                        op: ModifierOp::Add {
                            stat: Stat::CheckEdge,
                            value: 1000
                        }
                    };
                    125
                ],
            })
            .collect();
        creature.set_equipment(equipment).unwrap();
        let before = creature.clone();
        assert_eq!(
            creature.apply_condition(
                &crate::content_ids::ConditionId::new("readied").unwrap(),
                crate::ActorId(1),
                500
            ),
            Err(crate::conditions::ConditionError::InvalidModifiers)
        );
        assert_eq!(creature, before);
    }

    #[test]
    fn untargeted_ability_grants_do_not_multiply_edge_validation_scans() {
        let original = crate::test_creatures::build(crate::test_creatures::species());
        let mut document = original.ruleset().document().clone();
        let template = document
            .abilities
            .iter()
            .find(|d| d.id.as_str() == "basic_melee")
            .unwrap()
            .clone();
        let granted: Vec<_> = (0..32)
            .map(|index| {
                let mut ability = template.clone();
                ability.id = AbilityId::new(format!("edge_scan_{index}")).unwrap();
                ability.basic_attack = false;
                let id = ability.id.clone();
                document.abilities.push(ability);
                Modifier {
                    scope: Scope::Always,
                    op: ModifierOp::GrantAbility { ability: id },
                }
            })
            .collect();
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
            *original.starting_skills(),
        )
        .unwrap();
        let fear = crate::content_ids::ConditionId::new("fear").unwrap();
        let mut sources: BTreeMap<_, _> = [9, 10]
            .into_iter()
            .map(|actor| {
                (
                    SourceId::Condition(fear.clone(), crate::ActorId(actor)),
                    build.ruleset().condition(&fear).unwrap().modifiers.clone(),
                )
            })
            .collect();
        let before = EDGE_SCANS.with(std::cell::Cell::get);
        let ordinary_abilities = derive_with_sources(&build, &sources, &[])
            .unwrap()
            .abilities
            .len();
        let ordinary = EDGE_SCANS.with(std::cell::Cell::get) - before;
        sources.insert(SourceId::Equipment(crate::ItemId(10)), granted);
        let before = EDGE_SCANS.with(std::cell::Cell::get);
        let derived = derive_with_sources(&build, &sources, &[]).unwrap();
        let many = EDGE_SCANS.with(std::cell::Cell::get) - before;
        assert_eq!(derived.abilities.len(), ordinary_abilities + 32);
        assert_eq!(
            many, ordinary,
            "unrelated grants must not add full edge-source scans"
        );
    }

    #[test]
    fn conflicting_tactical_edge_overrides_reject_equipment_atomically() {
        for stat in [
            Stat::Ability {
                ability: AbilityId::new("falter").unwrap(),
                stat: AbilityStat::CheckEdge,
            },
            Stat::IncomingEdge {
                defense: Some(DefenseKind::Physical),
            },
        ] {
            let build = crate::test_creatures::build(crate::test_creatures::species());
            let mut creature = super::super::CreatureState::new(build).unwrap();
            let before = creature.clone();
            let equipment = (1..=2)
                .map(|value| EquipmentSource {
                    item: crate::ItemId(value),
                    socket: crate::EquipmentSlotId(value as u16),
                    modifiers: vec![Modifier {
                        scope: Scope::Always,
                        op: ModifierOp::Set {
                            stat: stat.clone(),
                            value: value as i64,
                        },
                    }],
                    attack: None,
                })
                .collect();
            assert_eq!(
                creature.set_equipment(equipment),
                Err(super::super::CreatureStateError::Build(
                    BuildError::ConflictingOverride
                ))
            );
            assert_eq!(creature, before);
        }
    }

    #[test]
    fn one_condition_family_contributes_once_after_causer_selection() {
        let original = crate::test_creatures::build(crate::test_creatures::species());
        let mut document = original.ruleset().document().clone();
        let mut alias = document
            .conditions
            .iter()
            .find(|condition| condition.id.as_str() == "fear")
            .unwrap()
            .clone();
        alias.id = crate::content_ids::ConditionId::new("hesitant").unwrap();
        alias.stacking = crate::rules::Stacking::Stack { maximum: 3 };
        document.conditions.push(alias);
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
            *original.starting_skills(),
        )
        .unwrap();
        let mut creature = super::super::CreatureState::new(build).unwrap();
        for _ in 0..3 {
            creature
                .apply_condition(
                    &crate::content_ids::ConditionId::new("hesitant").unwrap(),
                    crate::ActorId(9),
                    500,
                )
                .unwrap();
        }
        for source in [9, 10] {
            creature
                .apply_condition(
                    &crate::content_ids::ConditionId::new("fear").unwrap(),
                    crate::ActorId(source),
                    500,
                )
                .unwrap();
        }
        assert_eq!(
            creature.derived().edge_against(crate::ActorId(9)).balance(),
            -1
        );
        assert_eq!(
            creature
                .derived()
                .edge_against(crate::ActorId(10))
                .balance(),
            -1
        );
        assert_eq!(
            creature
                .derived()
                .edge_against(crate::ActorId(11))
                .balance(),
            0
        );
        creature
            .apply_condition(
                &crate::content_ids::ConditionId::new("rattled").unwrap(),
                crate::ActorId(9),
                500,
            )
            .unwrap();
        assert_eq!(
            creature.derived().edge_against(crate::ActorId(9)).balance(),
            -2
        );
    }
    #[test]
    fn ability_specific_talent_edge_combines_before_the_talent_clamp() {
        let build = crate::test_creatures::build(crate::test_creatures::species());
        let fear = AbilityId::new("falter").unwrap();
        let extra = BTreeMap::from([
            (
                SourceId::Talent(TalentId::new("positive").unwrap()),
                vec![modifier(Scope::Always, false, 4)],
            ),
            (
                SourceId::Talent(TalentId::new("conditional").unwrap()),
                vec![Modifier {
                    scope: Scope::Always,
                    op: ModifierOp::Add {
                        stat: Stat::Ability {
                            ability: fear.clone(),
                            stat: AbilityStat::CheckEdge,
                        },
                        value: -2,
                    },
                }],
            ),
        ]);
        let derived = derive_with_sources(&build, &extra, &[]).unwrap();
        assert_eq!(
            derived
                .check_edge(build.ruleset(), crate::ActorId(9), &fear)
                .unwrap(),
            0
        );
        assert_eq!(
            derived
                .check_edge(
                    build.ruleset(),
                    crate::ActorId(9),
                    &AbilityId::new("basic_melee").unwrap()
                )
                .unwrap(),
            1
        );
    }
    #[test]
    fn independent_edge_stats_and_participants_do_not_share_override_slots() {
        let build = crate::test_creatures::build(crate::test_creatures::species());
        let ability = AbilityId::new("basic_melee").unwrap();
        let caster = derive_with_sources(
            &build,
            &BTreeMap::from([(
                SourceId::Equipment(crate::ItemId(10)),
                vec![
                    modifier(Scope::Always, true, 1),
                    Modifier {
                        scope: Scope::Always,
                        op: ModifierOp::Set {
                            stat: Stat::Ability {
                                ability: ability.clone(),
                                stat: AbilityStat::CheckEdge,
                            },
                            value: 0,
                        },
                    },
                ],
            )]),
            &[],
        )
        .unwrap();
        let defender = derive_with_sources(
            &build,
            &BTreeMap::from([(
                SourceId::Equipment(crate::ItemId(11)),
                vec![Modifier {
                    scope: Scope::Always,
                    op: ModifierOp::Set {
                        stat: Stat::IncomingEdge { defense: None },
                        value: -1,
                    },
                }],
            )]),
            &[],
        )
        .unwrap();
        assert_eq!(
            caster
                .check_edge(build.ruleset(), crate::ActorId(2), &ability)
                .unwrap(),
            1
        );
        assert_eq!(
            caster
                .check_edge_with(
                    &defender,
                    build.ruleset(),
                    CheckContext {
                        ability: Some(&ability),
                        roller_role: crate::rules::CheckActor::User,
                        roller: crate::ActorId(1),
                        opponent: crate::ActorId(2),
                        defense: Some(DefenseKind::Physical)
                    }
                )
                .unwrap(),
            0
        );
    }
    #[test]
    fn incoming_and_outgoing_talents_share_one_check_cap() {
        let build = crate::test_creatures::build(crate::test_creatures::species());
        let caster = derive_with_sources(
            &build,
            &BTreeMap::from([
                (
                    SourceId::Talent(TalentId::new("outgoing").unwrap()),
                    vec![modifier(Scope::Always, false, 4)],
                ),
                (
                    SourceId::Equipment(crate::ItemId(10)),
                    vec![modifier(Scope::Always, false, -1)],
                ),
            ]),
            &[],
        )
        .unwrap();
        let defender = derive_with_sources(
            &build,
            &BTreeMap::from([(
                SourceId::Talent(TalentId::new("incoming").unwrap()),
                vec![Modifier {
                    scope: Scope::Always,
                    op: ModifierOp::Add {
                        stat: Stat::IncomingEdge { defense: None },
                        value: 3,
                    },
                }],
            )]),
            &[],
        )
        .unwrap();
        assert_eq!(
            caster
                .check_edge_with(
                    &defender,
                    build.ruleset(),
                    CheckContext {
                        ability: Some(&AbilityId::new("basic_melee").unwrap()),
                        roller_role: crate::rules::CheckActor::User,
                        roller: crate::ActorId(1),
                        opponent: crate::ActorId(2),
                        defense: Some(DefenseKind::Physical)
                    }
                )
                .unwrap(),
            0
        );
    }
    #[test]
    fn per_check_caps_leave_the_shared_effect_budget_and_provenance_intact() {
        let build = crate::test_creatures::build(crate::test_creatures::species());
        let talent = SourceId::Talent(TalentId::new("shared_edge").unwrap());
        let fear = SourceId::Condition(
            crate::content_ids::ConditionId::new("fear").unwrap(),
            crate::ActorId(9),
        );
        let mut sources = BTreeMap::from([
            (talent.clone(), vec![modifier(Scope::Always, false, 5)]),
            (
                fear,
                vec![Modifier {
                    scope: Scope::VersusSource,
                    op: ModifierOp::Add {
                        stat: Stat::CheckEdge,
                        value: -1,
                    },
                }],
            ),
        ]);
        let ability = AbilityId::new("basic_melee").unwrap();
        let derived = derive_with_sources(&build, &sources, &[]).unwrap();
        assert_eq!(
            derived
                .effect_edge_against(build.ruleset(), crate::ActorId(9))
                .unwrap(),
            5
        );
        assert_eq!(
            derived
                .check_edge(build.ruleset(), crate::ActorId(9), &ability)
                .unwrap(),
            0
        );
        assert_eq!(
            derived
                .check_edge(build.ruleset(), crate::ActorId(10), &ability)
                .unwrap(),
            1
        );
        assert_eq!(derived.sources[&talent], sources[&talent]);
        sources.insert(
            SourceId::Talent(TalentId::new("opposing_shared").unwrap()),
            vec![modifier(Scope::Always, false, -4)],
        );
        let derived = derive_with_sources(&build, &sources, &[]).unwrap();
        assert_eq!(
            derived
                .effect_edge_against(build.ruleset(), crate::ActorId(9))
                .unwrap(),
            1
        );
        assert_eq!(
            derived
                .check_edge(build.ruleset(), crate::ActorId(9), &ability)
                .unwrap(),
            -1
        );
        assert_eq!(
            derived
                .check_edge(build.ruleset(), crate::ActorId(10), &ability)
                .unwrap(),
            0
        );
    }
    #[test]
    fn talent_edge_clamps_each_sign_before_other_sources_cancel() {
        let build = crate::test_creatures::build(crate::test_creatures::species());
        let extra = BTreeMap::from([
            (
                SourceId::Talent(TalentId::new("positive").unwrap()),
                vec![modifier(Scope::Always, false, 4)],
            ),
            (
                SourceId::Talent(TalentId::new("negative").unwrap()),
                vec![modifier(Scope::Always, false, -2)],
            ),
            (
                SourceId::Equipment(crate::ItemId(10)),
                vec![modifier(Scope::Always, false, -1)],
            ),
        ]);
        let derived = derive_with_sources(&build, &extra, &[]).unwrap();
        assert_eq!(derived.edge_against(crate::ActorId(9)).balance(), -1);
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
            let derived = derive_with_sources(&build, &extra, &[]).unwrap();
            assert_eq!(derived.edge_against(crate::ActorId(9)).balance(), 1);
        }
    }
}

#[cfg(test)]
mod loss_fold_tests {
    use super::*;

    #[test]
    fn restorable_losses_reuse_the_single_source_fold_and_zero_loss_is_identical() {
        let mut species = crate::test_creatures::species();
        species.default_attributes = Attributes::new(ByAttribute {
            strength: 2,
            ..ByAttribute::default()
        })
        .unwrap();
        let build = crate::test_creatures::build(species);
        let before = fold_count();
        let edge_before = EDGE_SCANS.with(std::cell::Cell::get);
        let ordinary = derive(&build).unwrap();
        let edge_work = EDGE_SCANS.with(std::cell::Cell::get) - edge_before;
        assert_eq!(fold_count() - before, 1);
        let zero = derive_with_losses(&build, &BTreeMap::new(), &[], Default::default()).unwrap();
        assert_eq!(zero, ordinary);
        let before = fold_count();
        let edge_before = EDGE_SCANS.with(std::cell::Cell::get);
        let changed = derive_with_losses(
            &build,
            &BTreeMap::new(),
            &[],
            super::super::RestorableLosses {
                attributes: ByAttribute {
                    strength: 1,
                    ..ByAttribute::default()
                },
                maximum_health: 1,
            },
        )
        .unwrap();
        assert_eq!(fold_count() - before, 1);
        assert_eq!(
            EDGE_SCANS.with(std::cell::Cell::get) - edge_before,
            edge_work
        );
        assert_eq!(changed.maximum_health, ordinary.maximum_health - 2);
        assert_eq!(changed.attributes.get(Attribute::Strength), 1);
        assert_eq!(changed.permanent_attributes, ordinary.permanent_attributes);
        assert_eq!(changed.sources, ordinary.sources);
    }
    #[test]
    fn wide_scalar_health_exceeds_the_bounded_individual_modifier_limit() {
        let build = crate::test_creatures::build(crate::test_creatures::species());
        let ordinary = build.derive().unwrap();
        let changed = derive_with_sources(
            &build,
            &BTreeMap::from([(
                SourceId::Equipment(crate::ItemId(1)),
                vec![Modifier {
                    scope: Scope::Always,
                    op: ModifierOp::Add {
                        stat: Stat::Health,
                        value: 1_000_000,
                    },
                }],
            )]),
            &[],
        )
        .unwrap();
        assert_eq!(changed.maximum_health, ordinary.maximum_health + 1_000_000);
    }

    #[test]
    fn wide_scalar_health_fold_checks_aggregate_overflow_and_preserves_set_precedence() {
        for health in [
            1_000_001,
            u64::from(u32::MAX),
            u64::from(u32::MAX) + 1,
            u64::MAX,
        ] {
            assert_eq!(Accumulator::default().health(health, 0), Ok(health));
            assert_eq!(
                Accumulator {
                    added: -1,
                    set: None
                }
                .health(health, 1),
                Ok(health)
            );
        }
        assert_eq!(
            Accumulator::default().health(u64::MAX, 1),
            Err(BuildError::Arithmetic)
        );
        assert_eq!(
            Accumulator {
                added: 1,
                set: None
            }
            .health(u64::MAX, 0),
            Err(BuildError::Arithmetic)
        );
        assert_eq!(
            Accumulator {
                added: -10,
                set: None
            }
            .health(1, 1),
            Ok(0)
        );
        assert_eq!(
            Accumulator {
                added: i64::MAX,
                set: Some(7)
            }
            .health(u64::MAX, 1000),
            Ok(7)
        );
        assert_eq!(
            Accumulator {
                added: 0,
                set: Some(-1)
            }
            .health(u64::MAX, 0),
            Ok(0)
        );
    }

    #[test]
    fn machine_arithmetic_and_authored_numeric_bounds_have_distinct_failures() {
        let mut sum = Accumulator {
            added: i64::MAX,
            set: None,
        };
        assert_eq!(sum.add(1), Err(BuildError::Arithmetic));
        assert_eq!(sum.value(1), Err(BuildError::Arithmetic));
        assert_eq!(
            Accumulator {
                added: crate::limits::NUMERIC,
                set: None
            }
            .value(1),
            Err(BuildError::DerivedLimit)
        );
        let mut document = crate::test_creatures::ruleset().document().clone();
        document.constants.duration_scale_numerator = 10;
        let rules = crate::rules::Ruleset::validate(document).unwrap();
        assert_eq!(
            scaled_duration(u64::MAX, Some(0), &rules),
            Err(BuildError::Arithmetic)
        );
        assert_eq!(
            scaled_duration(crate::limits::NUMERIC as u64 + 1, None, &rules),
            Err(BuildError::DerivedLimit)
        );
    }
}
