//! Eligible permanent Intellect, independent of effective conditions, equipment,
//! restorable attribute losses, and templates acquired after birth.
use super::derive::{add_source, apply_templates, flags, source_attributes, PermanentFacts};
use super::{BuildError, CreatureBuild, HdChoices, HitDiceSummary, SourceId};
use crate::attributes::{Attribute, Attributes, ByAttribute, SkillRanks};
use crate::content_ids::{ClassId, SubtypeId, TalentId, TemplateId, TypeId};
use crate::progression::HdSource;
use crate::rules::Modifier;
use crate::HdCount;
use std::collections::{BTreeMap, BTreeSet};

/// Compact composition witness, never a trusted score or stored grant amount.
/// Birth templates, species and initial attributes come from immutable birth
/// ownership. Receipt validation must prove these counts and identities against
/// the exact retained/current choice receipts before accepting a historical row.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct IntellectSources {
    pub(super) strength_acquired: u8,
    pub(super) attribute_increases: ByAttribute<u16>,
    /// Each owned class contributes its definition exactly once.
    pub(super) classes: BTreeSet<ClassId>,
    /// Active under the eligible permanent composition, not effective state and
    /// not merely owned. Historical witnesses retain the then-active identities.
    pub(super) talents: BTreeSet<TalentId>,
}

struct Baseline {
    kind: TypeId,
    subtypes: BTreeSet<SubtypeId>,
    attributes: Attributes,
    sources: BTreeMap<SourceId, Vec<Modifier>>,
}

fn baseline(
    build: &CreatureBuild,
    initial_attributes: Attributes,
    birth_templates: &[TemplateId],
    witness: &IntellectSources,
) -> Result<Baseline, BuildError> {
    if birth_templates.len() > crate::limits::TEMPLATES
        || birth_templates.iter().collect::<BTreeSet<_>>().len() != birth_templates.len()
        || witness.talents.len() > crate::limits::OWNED_TALENTS
        || witness.classes.len() > crate::limits::HIT_DICE
    {
        return Err(BuildError::InvalidDefinition);
    }
    let rules = build.ruleset();
    let mut values = initial_attributes.values();
    for attribute in Attribute::ALL {
        let value = values.get_mut(attribute);
        *value = value
            .checked_add(*witness.attribute_increases.get(attribute))
            .ok_or(BuildError::Arithmetic)?;
        if *value > rules.constants().attribute_cap {
            return Err(BuildError::AttributeCap);
        }
    }
    let ordinary = Attributes::new(values).map_err(|_| BuildError::AttributeCap)?;
    let mut attributes =
        super::derive::with_acquired_strength(ordinary, witness.strength_acquired)?;
    let templates = birth_templates
        .iter()
        .map(|id| {
            build
                .content()
                .catalog()
                .templates
                .get(id)
                .ok_or(BuildError::UnknownTemplate)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let species = build.species();
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
    for class in &witness.classes {
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
    Ok(Baseline {
        kind,
        subtypes,
        attributes,
        sources,
    })
}

/// Evaluate either a current composition or a validated historical witness.
/// All definitions are resolved from the immutable catalog. In particular, Set
/// masking and flags are evaluated together, never as per-source nominal grants.
pub(super) fn eligible_intellect(
    build: &CreatureBuild,
    birth_templates: &[TemplateId],
    witness: &IntellectSources,
) -> Result<u16, BuildError> {
    eligible_intellect_at(build, build.initial_attributes(), birth_templates, witness)
}

/// Historical receipt evaluation retains the immutable initial attributes from
/// that birth record, even after a privileged edit of the current build.
pub(super) fn eligible_intellect_at(
    build: &CreatureBuild,
    initial_attributes: Attributes,
    birth_templates: &[TemplateId],
    witness: &IntellectSources,
) -> Result<u16, BuildError> {
    let mut baseline = baseline(build, initial_attributes, birth_templates, witness)?;
    for talent in &witness.talents {
        add_source(
            &mut baseline.sources,
            SourceId::Talent(talent.clone()),
            &build
                .ruleset()
                .talent(talent)
                .ok_or(BuildError::UnknownTalent)?
                .modifiers,
            build.ruleset(),
        )?;
    }
    Ok(
        source_attributes(baseline.attributes, &baseline.sources, build.ruleset())?
            .get(Attribute::Intellect),
    )
}

/// Reuse already validated advancement facts from the permanent compile. The
/// attributes must be the pre-template initial-plus-HD-choice attributes. This
/// path performs no additional advancement/HD scan, effective fold, or build copy.
pub(super) fn current_sources_from_facts(
    build: &CreatureBuild,
    birth_templates: &[TemplateId],
    attributes: Attributes,
    skills: &SkillRanks,
    selected: &BTreeSet<TalentId>,
    hit_dice: &HitDiceSummary,
) -> Result<IntellectSources, BuildError> {
    sources_from_facts(
        build,
        build.initial_attributes(),
        birth_templates,
        build.strength_acquired(),
        attributes,
        skills,
        selected,
        hit_dice,
    )
}

fn sources_from_facts(
    build: &CreatureBuild,
    initial_attributes: Attributes,
    birth_templates: &[TemplateId],
    strength_acquired: u8,
    attributes: Attributes,
    skills: &SkillRanks,
    selected: &BTreeSet<TalentId>,
    hit_dice: &HitDiceSummary,
) -> Result<IntellectSources, BuildError> {
    let mut witness = IntellectSources {
        strength_acquired,
        classes: hit_dice.classes.keys().cloned().collect(),
        ..IntellectSources::default()
    };
    for attribute in Attribute::ALL {
        *witness.attribute_increases.get_mut(attribute) = attributes
            .get(attribute)
            .checked_sub(initial_attributes.get(attribute))
            .ok_or(BuildError::InvalidOwnership)?;
    }
    let baseline = baseline(build, initial_attributes, birth_templates, &witness)?;
    let attributes = source_attributes(baseline.attributes, &baseline.sources, build.ruleset())?;
    let flags = flags(&baseline.sources);
    let facts = PermanentFacts {
        build,
        hit_dice,
        kind: &baseline.kind,
        subtypes: &baseline.subtypes,
        skills,
        attributes,
        flags: &flags,
        sources: &baseline.sources,
    };
    witness.talents = build.ruleset().active_talents(selected, &facts);
    Ok(witness)
}

/// Standalone current witness for transition/receipt validation. Permanent
/// compilation should use `current_sources_from_facts` to reuse its existing pass.
pub(super) fn current_sources(
    build: &CreatureBuild,
    birth_templates: &[TemplateId],
) -> Result<IntellectSources, BuildError> {
    let (attributes, skills, selected) =
        super::derive::advancement_facts(build, build.initial_attributes())?;
    let mut hit_dice = HitDiceSummary {
        total: build.ledger().total_hd(),
        racial: HdCount::ZERO,
        classes: BTreeMap::new(),
    };
    for entry in build.ledger().entries() {
        crate::diagnostics::raw_hit_die_visit();
        match entry.source() {
            HdSource::Racial => {
                hit_dice.racial = hit_dice
                    .racial
                    .checked_add(1)
                    .ok_or(BuildError::Arithmetic)?
            }
            HdSource::Class(class) => {
                let count = hit_dice.classes.entry(class).or_insert(HdCount::ZERO);
                *count = count.checked_add(1).ok_or(BuildError::Arithmetic)?;
            }
        }
    }
    current_sources_from_facts(
        build,
        birth_templates,
        attributes,
        &skills,
        &selected,
        &hit_dice,
    )
}

/// Reconstruct eligibility from validated, ordinal-ordered historical ownership
/// rows, rather than accepting a saved active-talent set as authority. The caller
/// validates each row's exact identity and pool spending against its receipt.
/// Starting skills belong to immutable birth ownership; source definitions are
/// the same immutable catalog used by current derivation.
#[cfg(test)]
pub(super) fn historical_sources(
    build: &CreatureBuild,
    initial_attributes: Attributes,
    birth_templates: &[TemplateId],
    rows: &[(HdSource, HdChoices)],
) -> Result<IntellectSources, BuildError> {
    historical_sources_refs(
        build,
        initial_attributes,
        birth_templates,
        0,
        rows.iter().map(|(source, choices)| (source, choices)),
    )
}

pub(super) fn historical_sources_refs<'a>(
    build: &CreatureBuild,
    initial_attributes: Attributes,
    birth_templates: &[TemplateId],
    strength_acquired: u8,
    rows: impl ExactSizeIterator<Item = (&'a HdSource, &'a HdChoices)>,
) -> Result<IntellectSources, BuildError> {
    if rows.len() > crate::limits::HIT_DICE {
        return Err(BuildError::InvalidOwnership);
    }
    let rules = build.ruleset();
    let mut attributes = initial_attributes;
    if Attribute::ALL
        .into_iter()
        .any(|attribute| attributes.get(attribute) > rules.constants().attribute_cap)
    {
        return Err(BuildError::AttributeCap);
    }
    let [first, second] = *build.starting_skills();
    if first == second {
        return Err(BuildError::InvalidStartingSkills);
    }
    let mut skills = SkillRanks::default();
    for skill in [first, second] {
        skills = skills
            .with_rank(skill, 1)
            .map_err(|_| BuildError::RankCap)?;
    }
    let mut selected = BTreeSet::new();
    let mut hit_dice = HitDiceSummary {
        total: HdCount::from_usize(rows.len()).map_err(BuildError::from)?,
        racial: HdCount::ZERO,
        classes: BTreeMap::new(),
    };
    for (index, (source, choices)) in rows.enumerate() {
        crate::diagnostics::raw_hit_die_visit();
        crate::diagnostics::advancement_choice_visit();
        match source {
            HdSource::Racial => {
                hit_dice.racial = hit_dice
                    .racial
                    .checked_add(1)
                    .ok_or(BuildError::Arithmetic)?
            }
            HdSource::Class(class) => {
                if rules.class(class).is_none() {
                    return Err(BuildError::UnknownClass);
                }
                let count = hit_dice
                    .classes
                    .entry(class.clone())
                    .or_insert(HdCount::ZERO);
                *count = count.checked_add(1).ok_or(BuildError::Arithmetic)?;
            }
        }
        for skill in choices.skills() {
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
        if let Some(attribute) = choices.attribute {
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
        for purchase in &choices.purchases {
            purchase.selection.validate(rules)?;
            if let Some(talent) = purchase.selection.talent() {
                if !selected.insert(talent.clone()) {
                    return Err(BuildError::DuplicateTalent);
                }
                if selected.len() > crate::limits::OWNED_TALENTS {
                    return Err(BuildError::TooManyOwnedTalents);
                }
            }
        }
    }
    sources_from_facts(
        build,
        initial_attributes,
        birth_templates,
        strength_acquired,
        attributes,
        &skills,
        &selected,
        &hit_dice,
    )
}
