//! Strict composition ownership records. Package snapshots are self-contained;
//! no source files or mutable spawn definitions are consulted during restore.
use super::{ChoiceRecord, HdRecord, SourceRecord};
use crate::bounded::{named_map, Bounded};
use crate::content_ids::{BodyFormId, SpeciesId};
use crate::creatures::body::{
    BodyBuild, BodyPackage, OwnedBodySnapshot, PersonalRestorationPolicy,
};
use crate::creatures::composition::{HdOwner, OwnedHitDie, PersonalHdId};
use crate::creatures::{BuildError, Content, HdChoices, PlannedAdvancement, RemovedAdvancement};
use crate::limits;
use crate::progression::{HdLedger, HdSource};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

// Every nested record rejects Serde's alternate positional sequence spelling.
macro_rules! record {
    ($name:ident { $( $(#[$attr:meta])* $field:ident: $ty:ty ),* $(,)? }) => {
        #[derive(Clone, Debug, PartialEq, Eq, Serialize)]
        #[serde(deny_unknown_fields)]
        pub(super) struct $name { $( $(#[$attr])* $field: $ty, )* }
        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Fields { $( $(#[$attr])* $field: $ty, )* }
                let fields: Fields = named_map(d)?;
                Ok(Self { $( $field: fields.$field, )* })
            }
        }
    };
}
record!(PackageRecord {
    id: BodyFormId,
    species: SpeciesId,
    body: GeometryRecord,
    advancement: Bounded<PackageAdvancementRecord, { limits::HIT_DICE }>,
});
record!(GeometryRecord { cells: Bounded<[i32; 3], 64>, eye: [i32; 3], mass: u32 });
record!(PackageAdvancementRecord {
    #[serde(deserialize_with = "named_map")]
    source: SourceRecord,
    #[serde(deserialize_with = "named_map")]
    choices: ChoiceRecord,
});
record!(RemovedRecord {
    hit_die: HdRecord,
    #[serde(deserialize_with = "named_map")]
    choices: ChoiceRecord,
});
record!(AcquisitionRecord {
    owner: HdOwner,
    species: SpeciesId
});
record!(InactiveRecord {
    form: BodyFormId,
    package: PackageRecord,
    rows: Bounded<RemovedRecord, { limits::HIT_DICE }>,
});
record!(PersonalRestorationEntry {
    id: PersonalHdId,
    #[serde(deserialize_with = "named_map")]
    source: SourceRecord,
    #[serde(deserialize_with = "named_map")]
    choices: ChoiceRecord,
});
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum PersonalRestorationRecord {
    ChooseAgain {},
    Authored {
        entries: Bounded<PersonalRestorationEntry, { limits::HIT_DICE }>,
    },
}
impl PersonalRestorationRecord {
    fn capture(policy: &PersonalRestorationPolicy) -> Self {
        match policy {
            PersonalRestorationPolicy::ChooseAgain => Self::ChooseAgain {},
            PersonalRestorationPolicy::Authored(plan) => Self::Authored {
                entries: Bounded(
                    plan.iter()
                        .map(|(id, row)| PersonalRestorationEntry {
                            id: *id,
                            source: SourceRecord::capture(row.source.clone()),
                            choices: ChoiceRecord::capture(&row.choices),
                        })
                        .collect(),
                ),
            },
        }
    }
    fn restore(&self) -> Result<PersonalRestorationPolicy, BuildError> {
        match self {
            Self::ChooseAgain {} => Ok(PersonalRestorationPolicy::ChooseAgain),
            Self::Authored { entries } => {
                let mut plan = BTreeMap::new();
                for row in &entries.0 {
                    if plan
                        .insert(
                            row.id,
                            PlannedAdvancement {
                                source: row.source.restore(),
                                choices: row.choices.restore(),
                            },
                        )
                        .is_some()
                    {
                        return Err(BuildError::InvalidOwnership);
                    }
                }
                Ok(PersonalRestorationPolicy::Authored(plan))
            }
        }
    }
}
record!(PersonalBodyGrantRecord {
    ordinal: u16,
    #[serde(deserialize_with = "named_map")]
    choices: ChoiceRecord,
});
record!(BodyBuildRecord {
    base: PackageRecord,
    #[serde(deserialize_with = "named_map")]
    base_initial_attributes: crate::attributes::ByAttribute<u16>,
    active: PackageRecord,
    health_root: u64,
    personal_seed_offset: u64,
    next_personal_id: u64,
    birth_personal_count: u64,
    #[serde(deserialize_with = "named_map")]
    personal_restoration: PersonalRestorationRecord,
    personal_body_grants: Bounded<PersonalBodyGrantRecord, { limits::HIT_DICE }>,
    acquisition_species: Bounded<AcquisitionRecord, { limits::CATALOG_ENTRIES * limits::HIT_DICE + limits::HIT_DICE }>,
    inactive: Bounded<InactiveRecord, { limits::CATALOG_ENTRIES }>,
    drained: Bounded<RemovedRecord, { limits::HIT_DICE }>,
});
impl PackageRecord {
    fn capture(package: &BodyPackage) -> Self {
        Self {
            id: package.id.clone(),
            species: package.species.clone(),
            body: GeometryRecord {
                cells: Bounded(package.body.cells.clone()),
                eye: package.body.eye,
                mass: package.body.mass,
            },
            advancement: Bounded(
                package
                    .advancement
                    .iter()
                    .map(|row| PackageAdvancementRecord {
                        source: SourceRecord::capture(row.source.clone()),
                        choices: ChoiceRecord::capture(&row.choices),
                    })
                    .collect(),
            ),
        }
    }
    fn restore(&self, content: &Content) -> Result<Arc<BodyPackage>, BuildError> {
        let package = BodyPackage {
            id: self.id.clone(),
            species: self.species.clone(),
            body: crate::BodySpec {
                cells: self.body.cells.0.clone(),
                eye: self.body.eye,
                mass: self.body.mass,
            },
            advancement: self
                .advancement
                .0
                .iter()
                .map(|row| PlannedAdvancement {
                    source: row.source.restore(),
                    choices: row.choices.restore(),
                })
                .collect(),
        };
        package.validate(content)?;
        // Empty catalogs support standalone builder snapshots. Once an authored
        // catalog is present, no saved definition may substitute for it.
        if content.body_packages().next().is_some()
            && content
                .body_package(&package.id)
                .is_none_or(|authored| authored.as_ref() != &package)
        {
            return Err(BuildError::InvalidDefinition);
        }
        Ok(Arc::new(package))
    }
}
impl RemovedRecord {
    fn capture(row: &RemovedAdvancement) -> Self {
        Self {
            hit_die: HdRecord::capture(&row.hit_die),
            choices: ChoiceRecord::capture(&row.choices),
        }
    }
    fn restore(&self) -> Result<RemovedAdvancement, BuildError> {
        Ok(RemovedAdvancement {
            hit_die: self.hit_die.restore(0).map_err(BuildError::from)?,
            choices: self.choices.restore(),
        })
    }
}
impl BodyBuildRecord {
    pub(super) fn capture(body: &BodyBuild) -> Self {
        Self {
            base: PackageRecord::capture(&body.base),
            base_initial_attributes: body.base_initial_attributes.values(),
            active: PackageRecord::capture(&body.active),
            health_root: body.health_root,
            personal_seed_offset: body.personal_seed_offset,
            next_personal_id: body.next_personal_id,
            birth_personal_count: body.birth_personal_count,
            personal_restoration: PersonalRestorationRecord::capture(&body.personal_restoration),
            personal_body_grants: Bounded(
                body.personal_body_grants
                    .iter()
                    .map(|(&ordinal, choices)| PersonalBodyGrantRecord {
                        ordinal,
                        choices: ChoiceRecord::capture(choices),
                    })
                    .collect(),
            ),
            acquisition_species: Bounded(
                body.acquisition_species
                    .iter()
                    .map(|(owner, species)| AcquisitionRecord {
                        owner: owner.clone(),
                        species: species.clone(),
                    })
                    .collect(),
            ),
            inactive: Bounded(
                body.inactive
                    .iter()
                    .map(|(form, snapshot)| InactiveRecord {
                        form: form.clone(),
                        package: PackageRecord::capture(&snapshot.package),
                        rows: Bounded(snapshot.rows.iter().map(RemovedRecord::capture).collect()),
                    })
                    .collect(),
            ),
            drained: Bounded(body.drained.iter().map(RemovedRecord::capture).collect()),
        }
    }
    pub(super) fn restore(
        &self,
        content: &Content,
        ledger: &HdLedger,
        choices: &[HdChoices],
        species: &SpeciesId,
    ) -> Result<BodyBuild, BuildError> {
        let base = self.base.restore(content)?;
        let active = self.active.restore(content)?;
        if (&active.species != species)
            || (base.id == active.id && base != active)
            || self.personal_seed_offset != base.advancement.len() as u64
            || self.next_personal_id == 0
            || self.birth_personal_count >= self.next_personal_id
            || ledger.health_root_seed().is_some()
            || ledger.drained().next().is_some()
            || ledger.entries().len() != choices.len()
        {
            return Err(BuildError::InvalidOwnership);
        }
        let mut acquisition_species = BTreeMap::new();
        for row in &self.acquisition_species.0 {
            if !content.catalog().species.contains_key(&row.species)
                || acquisition_species
                    .insert(row.owner.clone(), row.species.clone())
                    .is_some()
            {
                return Err(BuildError::InvalidOwnership);
            }
        }
        let mut inactive = BTreeMap::new();
        for row in &self.inactive.0 {
            if row.form == active.id
                || row.form != row.package.id
                || row.rows.0.is_empty()
                || inactive
                    .insert(
                        row.form.clone(),
                        OwnedBodySnapshot {
                            package: row.package.restore(content)?,
                            rows: row
                                .rows
                                .0
                                .iter()
                                .map(RemovedRecord::restore)
                                .collect::<Result<Vec<_>, _>>()?,
                        },
                    )
                    .is_some()
            {
                return Err(BuildError::InvalidOwnership);
            }
        }
        let mut personal_body_grants = BTreeMap::new();
        let mut previous = None;
        for row in &self.personal_body_grants.0 {
            if previous.is_some_and(|ordinal| ordinal >= row.ordinal) {
                return Err(BuildError::InvalidOwnership);
            }
            previous = Some(row.ordinal);
            personal_body_grants.insert(row.ordinal, row.choices.restore());
        }
        let body = BodyBuild {
            base,
            active,
            base_initial_attributes: crate::attributes::Attributes::new(
                self.base_initial_attributes,
            )
            .map_err(|_| BuildError::AttributeCap)?,
            health_root: self.health_root,
            personal_seed_offset: self.personal_seed_offset,
            next_personal_id: self.next_personal_id,
            birth_personal_count: self.birth_personal_count,
            personal_restoration: self.personal_restoration.restore()?,
            personal_body_grants,
            acquisition_species,
            inactive,
            drained: self
                .drained
                .0
                .iter()
                .map(RemovedRecord::restore)
                .collect::<Result<Vec<_>, _>>()?,
        };
        body.validate_personal_body_grants(content)?;
        validate(&body, content, ledger, choices)?;
        Ok(body)
    }
}
fn validate(
    body: &BodyBuild,
    content: &Content,
    ledger: &HdLedger,
    choices: &[HdChoices],
) -> Result<(), BuildError> {
    if let PersonalRestorationPolicy::Authored(plan) = &body.personal_restoration {
        if plan.len() as u64 != body.birth_personal_count {
            return Err(BuildError::InvalidOwnership);
        }
        for (ordinal, (id, declared)) in plan.iter().enumerate() {
            if id.0.get() != ordinal as u64 + 1 {
                return Err(BuildError::InvalidOwnership);
            }
            let owner = HdOwner::Personal { id: *id };
            let (die, retained_choices) = ledger
                .entries()
                .iter()
                .zip(choices)
                .chain(body.drained.iter().map(|row| (&row.hit_die, &row.choices)))
                .find(|(die, _)| die.owner() == Some(&owner))
                .ok_or(BuildError::InvalidOwnership)?;
            if die.source() != declared.source
                || declared.choices.purchase_highwater > retained_choices.purchase_highwater
            {
                return Err(BuildError::InvalidOwnership);
            }
            declared.choices.validate_purchase_ids(Some(&owner))?;
            // A retired original receipt may coexist with a later respec serial,
            // but one stable PurchaseId can never denote two different purchases.
            if declared.choices.purchases.iter().any(|purchase| {
                retained_choices
                    .purchases
                    .iter()
                    .any(|retained| retained.id == purchase.id && retained != purchase)
            }) {
                return Err(BuildError::InvalidOwnership);
            }
            OwnedHitDie::new(
                owner,
                die.clone(),
                declared.choices.purchases.clone(),
                declared.choices.attribute,
                content.rules(),
            )?;
        }
    }
    let mut owners = BTreeSet::new();
    let mut check =
        |die: &crate::progression::HitDie, choices: &HdChoices| -> Result<(), BuildError> {
            let owner = die.owner().ok_or(BuildError::InvalidOwnership)?;
            if !owners.insert(owner.clone()) {
                return Err(BuildError::InvalidOwnership);
            }
            choices.validate_purchase_ids(Some(owner))?;
            OwnedHitDie::new(
                owner.clone(),
                die.clone(),
                choices.purchases.clone(),
                choices.attribute,
                content.rules(),
            )?;
            let species_id = body
                .acquisition_species
                .get(owner)
                .ok_or(BuildError::InvalidOwnership)?;
            let species = content
                .catalog()
                .species
                .get(species_id)
                .ok_or(BuildError::UnknownSpecies)?;
            let origin = die.health_origin().ok_or(BuildError::InvalidOwnership)?;
            if !origin.templates.is_empty() || origin.kind != species.kind {
                return Err(BuildError::InvalidOwnership);
            }
            let expected_die = match die.source() {
                HdSource::Racial => content
                    .rules()
                    .creature_type(&origin.kind)
                    .map(|definition| definition.health_die),
                HdSource::Class(class) => content
                    .rules()
                    .class(&class)
                    .map(|definition| definition.health_die),
            }
            .ok_or(BuildError::InvalidDefinition)?;
            if origin.die != expected_die {
                return Err(BuildError::InvalidOwnership);
            }
            let (root, ordinal) = match owner {
                HdOwner::Personal { id } => {
                    if id.0.get() >= body.next_personal_id {
                        return Err(BuildError::InvalidOwnership);
                    }
                    (
                        body.health_root,
                        body.personal_seed_offset
                            .checked_add(id.0.get() - 1)
                            .ok_or(BuildError::Arithmetic)?,
                    )
                }
                HdOwner::Body { form, ordinal } => {
                    if die.source() != HdSource::Racial || choices.attribute.is_some() {
                        return Err(BuildError::InvalidOwnership);
                    }
                    let package = if form == &body.active.id {
                        &body.active
                    } else if form == &body.base.id {
                        &body.base
                    } else {
                        &body
                            .inactive
                            .get(form)
                            .ok_or(BuildError::InvalidOwnership)?
                            .package
                    };
                    if species_id != &package.species {
                        return Err(BuildError::InvalidOwnership);
                    }
                    let intended = package
                        .advancement
                        .get(usize::from(*ordinal))
                        .ok_or(BuildError::InvalidOwnership)?;
                    let mut intended_choices = intended.choices.clone();
                    intended_choices.enroll_owner(owner)?;
                    if die.source() != intended.source || choices != &intended_choices {
                        return Err(BuildError::InvalidOwnership);
                    }
                    let root = if form == &body.base.id {
                        body.health_root
                    } else {
                        form.as_str()
                            .bytes()
                            .fold(body.health_root ^ 0x626f64795f68645f, |seed, byte| {
                                seed.wrapping_mul(0x100000001b3) ^ u64::from(byte)
                            })
                    };
                    (root, u64::from(*ordinal))
                }
            };
            if die.health_seed() != crate::dice::random_word_at(root, ordinal) {
                return Err(BuildError::InvalidHealthRoot);
            }
            Ok(())
        };
    let mut body_count = 0;
    let mut previous_personal = 0;
    for (die, choices) in ledger.entries().iter().zip(choices) {
        match die.owner() {
            Some(HdOwner::Body { form, ordinal })
                if form == &body.active.id
                    && previous_personal == 0
                    && usize::from(*ordinal) == body_count =>
            {
                let intended = body
                    .active
                    .advancement
                    .get(body_count)
                    .ok_or(BuildError::InvalidOwnership)?;
                let mut intended_choices = intended.choices.clone();
                intended_choices.enroll_owner(die.owner().ok_or(BuildError::InvalidOwnership)?)?;
                if die.source() != intended.source || choices != &intended_choices {
                    return Err(BuildError::InvalidOwnership);
                }
                body_count += 1;
            }
            Some(HdOwner::Personal { id }) if id.0.get() > previous_personal => {
                previous_personal = id.0.get();
            }
            _ => return Err(BuildError::InvalidOwnership),
        }
        check(die, choices)?;
    }
    for (form, snapshot) in &body.inactive {
        if snapshot.rows.len() != snapshot.package.advancement.len()
            || (form == &body.base.id && snapshot.package != body.base)
        {
            return Err(BuildError::InvalidOwnership);
        }
        for (ordinal, row) in snapshot.rows.iter().enumerate() {
            if !matches!(row.hit_die.owner(), Some(HdOwner::Body { form: actual, ordinal: actual_ordinal }) if actual == form && usize::from(*actual_ordinal) == ordinal)
            {
                return Err(BuildError::InvalidOwnership);
            }
            let intended = &snapshot.package.advancement[ordinal];
            let mut intended_choices = intended.choices.clone();
            intended_choices
                .enroll_owner(row.hit_die.owner().ok_or(BuildError::InvalidOwnership)?)?;
            if row.hit_die.source() != intended.source || row.choices != intended_choices {
                return Err(BuildError::InvalidOwnership);
            }
            check(&row.hit_die, &row.choices)?;
        }
    }
    let mut drained_body_ordinals = BTreeSet::new();
    for row in &body.drained {
        if let Some(HdOwner::Body { form, ordinal }) = row.hit_die.owner() {
            if form != &body.active.id
                || usize::from(*ordinal) < body_count
                || !drained_body_ordinals.insert(usize::from(*ordinal))
            {
                return Err(BuildError::InvalidOwnership);
            }
        }
        check(&row.hit_die, &row.choices)?;
    }
    let drained_active = body.drained.iter().filter(|row| matches!(row.hit_die.owner(), Some(HdOwner::Body { form, .. }) if form == &body.active.id)).count();
    if body_count + drained_active != body.active.advancement.len()
        || drained_body_ordinals
            .into_iter()
            .ne(body_count..body.active.advancement.len())
        || owners.len() != body.acquisition_species.len()
    {
        return Err(BuildError::InvalidOwnership);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::attributes::{ManaBinding, Skill};
    use crate::content_ids::ClassId;
    use crate::creatures::{CreatureBuild, ScenarioCatalog};

    fn subject() -> CreatureBuild {
        let species = crate::test_creatures::unmodified_species("body_record_subject");
        let species_id = species.id.clone();
        let content = Content::new(
            crate::test_support::test_ruleset(),
            ScenarioCatalog {
                species: [(species.id.clone(), species)].into_iter().collect(),
                templates: BTreeMap::new(),
            },
        )
        .unwrap();
        let build = CreatureBuild::new(
            content,
            species_id.clone(),
            HdLedger::seeded(
                vec![
                    HdSource::Racial,
                    HdSource::Class(ClassId::new("warrior").unwrap()),
                ],
                42,
            )
            .unwrap(),
            ManaBinding::Intellect,
            [Skill::HeavyWeaponry, Skill::Discipline],
        )
        .unwrap();
        let package = Arc::new(BodyPackage {
            id: BodyFormId::new("original_body").unwrap(),
            species: species_id,
            body: crate::BodySpec::default(),
            advancement: vec![PlannedAdvancement {
                source: HdSource::Racial,
                choices: HdChoices::default(),
            }],
        });
        build.with_body_package(package, 42).unwrap()
    }

    #[test]
    fn authored_body_definitions_are_immutable_across_active_base_and_inactive_records() {
        let original = subject();
        let base = original.base_body_package().unwrap().clone();
        let other = Arc::new(BodyPackage {
            id: BodyFormId::new("catalogued_other_body").unwrap(),
            species: original.species_id().clone(),
            body: crate::BodySpec::default(),
            advancement: vec![PlannedAdvancement {
                source: HdSource::Racial,
                choices: HdChoices::default(),
            }],
        });
        let changed = original.replace_body_package(other.clone()).unwrap();
        let encoded = serde_json::to_value(super::super::BuildRecord::capture(&changed)).unwrap();
        let content = Content::with_body_packages(
            original.content(),
            [base.clone(), other.clone()]
                .into_iter()
                .map(|package| (package.id.clone(), package))
                .collect(),
        )
        .unwrap();
        let valid: super::super::BuildRecord = serde_json::from_value(encoded.clone()).unwrap();
        let restored = valid.restore(content.clone()).unwrap();
        assert!(restored.authored_body_packages_valid());
        assert_eq!(
            serde_json::to_value(super::super::BuildRecord::capture(&restored)).unwrap(),
            encoded
        );
        for field in ["base", "active", "inactive"] {
            let mut forged = encoded.clone();
            let package = if field == "inactive" {
                &mut forged["body_build"]["inactive"][0]["package"]
            } else {
                &mut forged["body_build"][field]
            };
            package["body"]["mass"] =
                serde_json::json!(package["body"]["mass"].as_u64().unwrap() + 1);
            let forged: super::super::BuildRecord = serde_json::from_value(forged).unwrap();
            assert!(
                forged.restore(content.clone()).is_err(),
                "same-ID {field} definition substitution"
            );
        }
        let missing = Content::with_body_packages(
            original.content(),
            [(base.id.clone(), base)].into_iter().collect(),
        )
        .unwrap();
        assert!(
            valid.restore(missing).is_err(),
            "nonempty catalog cannot omit retained active package"
        );
        // Standalone builders remain usable, but never confer Game admission.
        assert!(!changed.authored_body_packages_valid());
        let standalone = valid.restore(original.content().clone()).unwrap();
        let at = tor_world::Location {
            region: tor_world::RegionId(1),
            position: tor_world::Position { x: 1, y: 1, z: 0 },
        };
        let identity = crate::CreatureIdentity {
            name: "catalog witness".into(),
            faction: "fixture".into(),
        };
        let mut game = crate::Game::two_room_in_stone(42, original.content().clone());
        let actor = game
            .spawn_actor(at, std::num::NonZeroU64::new(100).unwrap())
            .unwrap();
        game.set_body(actor, other.body.clone()).unwrap();
        let before = game.clone();
        assert!(game
            .configure_creature(actor, identity.clone(), standalone)
            .is_err());
        assert_eq!(game, before);
        let mut game = crate::Game::two_room_in_stone(42, content);
        let actor = game
            .spawn_actor(at, std::num::NonZeroU64::new(100).unwrap())
            .unwrap();
        game.set_body(actor, other.body.clone()).unwrap();
        game.configure_creature(actor, identity, restored).unwrap();
        let mut shared = crate::checkpoint::SharedState::default();
        let saved = game.checkpoint(&mut shared);
        assert!(crate::Game::restore_checkpoint(saved, &shared, game.content().clone()).is_ok());
    }

    #[test]
    fn pending_body_return_roundtrip_requires_retained_cancellation() {
        let original = subject();
        let package = Arc::new(BodyPackage {
            id: BodyFormId::new("pending_return_body").unwrap(),
            species: original.species_id().clone(),
            body: crate::BodySpec::default(),
            advancement: vec![PlannedAdvancement {
                source: HdSource::Racial,
                choices: HdChoices::default(),
            }],
        });
        let changed = original.replace_body_package(package).unwrap();
        let content = changed.content().clone();
        let mut creature = crate::creatures::CreatureState::new(changed).unwrap();
        let mut uncancelled = creature.clone();
        creature.cancel_specials().unwrap();
        creature.set_polymorph_return_pending(true).unwrap();
        let encoded =
            serde_json::to_value(super::super::CreatureRecord::capture(&creature)).unwrap();
        let valid: super::super::CreatureRecord = serde_json::from_value(encoded.clone()).unwrap();
        let restored = valid.restore(content.clone(), vec![]).unwrap();
        assert_eq!(restored, creature);
        assert!(restored.cancelled() && restored.polymorph_return_pending());
        // Prove the positive current-record fixture before either negative.
        assert!(uncancelled.set_polymorph_return_pending(true).is_err());
        let mut invalid = encoded;
        invalid["cancelled"] = serde_json::json!(false);
        let invalid: super::super::CreatureRecord = serde_json::from_value(invalid).unwrap();
        assert!(invalid.restore(content, vec![]).is_err());
    }

    #[test]
    fn composed_build_roundtrip_retains_inactive_package_and_personal_identity() {
        let original = subject();
        let package = Arc::new(BodyPackage {
            id: BodyFormId::new("larger_body").unwrap(),
            species: original.species_id().clone(),
            body: crate::BodySpec::default(),
            advancement: vec![
                PlannedAdvancement {
                    source: HdSource::Racial,
                    choices: HdChoices::default()
                };
                2
            ],
        });
        let changed = original.replace_body_package(package).unwrap();
        let encoded = serde_json::to_value(super::super::BuildRecord::capture(&changed)).unwrap();
        let decoded: super::super::BuildRecord = serde_json::from_value(encoded).unwrap();
        let restored = decoded.restore(changed.content().clone()).unwrap();
        assert_eq!(restored, changed);
        assert_eq!(
            restored.ledger().entries().last().unwrap().owner(),
            original.ledger().entries().last().unwrap().owner()
        );
        let returned = restored
            .replace_body_package(original.base_body_package().unwrap().clone())
            .unwrap();
        assert_eq!(returned.ledger(), original.ledger());
        assert_eq!(returned.choices(), original.choices());
    }

    #[test]
    fn body_record_rejects_missing_nullable_shape_positional_maps_and_bad_ownership() {
        let original = subject();
        let encoded = serde_json::to_value(super::super::BuildRecord::capture(&original)).unwrap();
        let valid: super::super::BuildRecord = serde_json::from_value(encoded.clone()).unwrap();
        assert_eq!(valid.restore(original.content().clone()).unwrap(), original);
        let mut missing = encoded.clone();
        missing.as_object_mut().unwrap().remove("body_build");
        assert!(serde_json::from_value::<super::super::BuildRecord>(missing).is_err());
        for field in ["base", "active"] {
            let mut bad = encoded.clone();
            bad["body_build"][field] = serde_json::json!([]);
            assert!(serde_json::from_value::<super::super::BuildRecord>(bad).is_err());
        }
        let mut bad = encoded.clone();
        bad["body_build"]["base"]["body"]["cells"] = serde_json::json!(vec![[0, 0, 0]; 65]);
        assert!(serde_json::from_value::<super::super::BuildRecord>(bad).is_err());
        for (field, value) in [
            ("next_personal_id", serde_json::json!(1)),
            ("health_root", serde_json::json!(43)),
            ("personal_seed_offset", serde_json::json!(2)),
        ] {
            let mut bad = encoded.clone();
            bad["body_build"][field] = value;
            let bad: super::super::BuildRecord = serde_json::from_value(bad).unwrap();
            assert!(
                bad.restore(original.content().clone()).is_err(),
                "forged {field}"
            );
        }
        let mut bad = encoded.clone();
        let row = bad["body_build"]["acquisition_species"][0].clone();
        bad["body_build"]["acquisition_species"]
            .as_array_mut()
            .unwrap()
            .push(row);
        let bad: super::super::BuildRecord = serde_json::from_value(bad).unwrap();
        assert!(bad.restore(original.content().clone()).is_err());
        let mut bad = encoded;
        bad["body_build"] = serde_json::Value::Null;
        let bad: super::super::BuildRecord = serde_json::from_value(bad).unwrap();
        assert!(bad.restore(original.content().clone()).is_err());
    }

    #[test]
    fn personal_body_grants_require_bounded_named_ordered_valid_receipts() {
        let original = subject();
        let encoded = serde_json::to_value(super::super::BuildRecord::capture(&original)).unwrap();
        assert_eq!(
            encoded["body_build"]["personal_body_grants"],
            serde_json::json!([])
        );
        let valid: super::super::BuildRecord = serde_json::from_value(encoded.clone()).unwrap();
        assert_eq!(valid.restore(original.content().clone()).unwrap(), original);
        let mut missing = encoded.clone();
        missing["body_build"]
            .as_object_mut()
            .unwrap()
            .remove("personal_body_grants");
        assert!(serde_json::from_value::<super::super::BuildRecord>(missing).is_err());
        for value in [serde_json::Value::Null, serde_json::json!([[0, {}]])] {
            let mut bad = encoded.clone();
            bad["body_build"]["personal_body_grants"] = value;
            assert!(serde_json::from_value::<super::super::BuildRecord>(bad).is_err());
        }
        let choice = serde_json::json!({"purchase_highwater":0,"purchases":[],"attribute":null});
        for rows in [
            serde_json::json!([{"ordinal":0,"choices":choice}]),
            serde_json::json!([{"ordinal":0,"choices":choice},{"ordinal":0,"choices":choice}]),
            serde_json::json!([{"ordinal":1,"choices":choice},{"ordinal":0,"choices":choice}]),
            serde_json::json!([{"ordinal":65535,"choices":choice}]),
        ] {
            let mut bad = encoded.clone();
            bad["body_build"]["personal_body_grants"] = rows;
            let bad: super::super::BuildRecord = serde_json::from_value(bad).unwrap();
            assert!(bad.restore(original.content().clone()).is_err());
        }
    }

    #[test]
    fn personal_restoration_records_require_policy_and_exact_stable_birth_owners() {
        let mut original = subject();
        original.train(crate::HdIndex(1), Skill::Lore).unwrap();
        let id = PersonalHdId(std::num::NonZeroU64::new(1).unwrap());
        let declared = PlannedAdvancement {
            source: original.ledger().entries()[1].source(),
            choices: original.choices()[1].clone(),
        };
        let choose = serde_json::to_value(super::super::BuildRecord::capture(&original)).unwrap();
        assert_eq!(
            choose["body_build"]["personal_restoration"],
            serde_json::json!({"kind":"choose_again"})
        );
        let valid: super::super::BuildRecord = serde_json::from_value(choose.clone()).unwrap();
        assert_eq!(valid.restore(original.content().clone()).unwrap(), original);
        original.body_build.as_mut().unwrap().personal_restoration =
            PersonalRestorationPolicy::Authored([(id, declared)].into());
        let encoded = serde_json::to_value(super::super::BuildRecord::capture(&original)).unwrap();
        let valid: super::super::BuildRecord = serde_json::from_value(encoded.clone()).unwrap();
        assert_eq!(valid.restore(original.content().clone()).unwrap(), original);
        // An original serial remains a restoration receipt after a legitimate
        // respec allocates serial2. Both live and drained newer choices must save.
        let mut respec = original.clone();
        let mut purchase = respec.choices()[1].purchases[0].clone();
        purchase.id = None;
        purchase.selection = crate::creatures::PurchaseSelection::SkillRank(Skill::Athletics);
        let preview = respec
            .preview_replacement(
                crate::creatures::PurchaseLocation {
                    owner: crate::HdIndex(1),
                    index: 0,
                },
                Some(purchase),
            )
            .unwrap();
        respec.commit_replacement(preview).unwrap();
        assert_eq!(respec.choices()[1].purchase_highwater, 2);
        assert_eq!(
            super::super::BuildRecord::capture(&respec)
                .restore(respec.content().clone())
                .unwrap(),
            respec
        );
        respec.remove_latest().unwrap();
        assert_eq!(
            super::super::BuildRecord::capture(&respec)
                .restore(respec.content().clone())
                .unwrap(),
            respec
        );
        for value in [
            serde_json::Value::Null,
            serde_json::json!([]),
            serde_json::json!({"kind":"authored"}),
            serde_json::json!({"kind":"choose_again","entries":[]}),
        ] {
            let mut bad = encoded.clone();
            bad["body_build"]["personal_restoration"] = value.clone();
            assert!(
                serde_json::from_value::<super::super::BuildRecord>(bad).is_err(),
                "invalid personal restoration shape: {value}"
            );
        }
        let mut missing = encoded.clone();
        missing["body_build"]
            .as_object_mut()
            .unwrap()
            .remove("personal_restoration");
        assert!(serde_json::from_value::<super::super::BuildRecord>(missing).is_err());
        for mutation in 0..6 {
            let mut bad = encoded.clone();
            let entries = &mut bad["body_build"]["personal_restoration"]["entries"];
            match mutation {
                0 => {
                    entries.as_array_mut().unwrap().clear();
                }
                1 => {
                    let row = entries[0].clone();
                    entries.as_array_mut().unwrap().push(row);
                }
                2 => {
                    entries[0]["id"] = serde_json::json!(2);
                }
                3 => {
                    entries[0]["source"] = serde_json::json!({"kind":"racial"});
                }
                4 => {
                    entries[0]["choices"]["purchases"][0]["id"]["owner"] =
                        serde_json::json!({"kind":"body","form":"original_body","ordinal":0});
                }
                5 => {
                    entries[0]["choices"]["purchases"][0]["selection"] =
                        serde_json::json!({"kind":"skill_rank","skill":"athletics"});
                }
                _ => unreachable!(),
            }
            let bad: super::super::BuildRecord = serde_json::from_value(bad).unwrap();
            assert!(
                bad.restore(original.content().clone()).is_err(),
                "mutation {mutation}"
            );
        }
    }

    #[test]
    fn retained_body_rows_reject_forged_acquisition_die_before_reactivation() {
        let original = subject();
        let package = Arc::new(BodyPackage {
            id: BodyFormId::new("other_body").unwrap(),
            species: original.species_id().clone(),
            body: crate::BodySpec::default(),
            advancement: vec![PlannedAdvancement {
                source: HdSource::Racial,
                choices: HdChoices::default(),
            }],
        });
        let changed = original.replace_body_package(package).unwrap();
        let encoded = serde_json::to_value(super::super::BuildRecord::capture(&changed)).unwrap();
        let valid: super::super::BuildRecord = serde_json::from_value(encoded.clone()).unwrap();
        assert_eq!(valid.restore(changed.content().clone()).unwrap(), changed);
        let mut invalid = encoded;
        invalid["body_build"]["inactive"][0]["rows"][0]["hit_die"]["health_origin"]["die"] =
            serde_json::json!(10);
        let invalid: super::super::BuildRecord = serde_json::from_value(invalid).unwrap();
        assert!(invalid.restore(changed.content().clone()).is_err());

        let mut drained = original.clone();
        drained.remove_latest().unwrap();
        let encoded = serde_json::to_value(super::super::BuildRecord::capture(&drained)).unwrap();
        let valid: super::super::BuildRecord = serde_json::from_value(encoded.clone()).unwrap();
        assert_eq!(valid.restore(drained.content().clone()).unwrap(), drained);
        let mut invalid = encoded;
        // The retained Warrior class die is d10, not the body type's d8.
        invalid["body_build"]["drained"][0]["hit_die"]["health_origin"]["die"] =
            serde_json::json!(8);
        let invalid: super::super::BuildRecord = serde_json::from_value(invalid).unwrap();
        assert!(invalid.restore(drained.content().clone()).is_err());
    }
}
