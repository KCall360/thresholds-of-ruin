//! Independent, required named ownership records. No compiled eligibility or
//! caller-supplied grant amount is persisted.
use super::*;
use crate::creatures::expertise_learning as domain;
use crate::creatures::intellect::IntellectSources;

macro_rules! record {
    ($name:ident { $($(#[$attr:meta])* $field:ident: $ty:ty),* $(,)? }) => {
        #[derive(Clone, Debug, PartialEq, Eq, Serialize)]
        pub(super) struct $name { $($(#[$attr])* $field: $ty,)* }
        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Fields { $($(#[$attr])* $field: $ty,)* }
                let fields: Fields = named_map(d)?;
                Ok(Self { $($field: fields.$field,)* })
            }
        }
    };
}
record!(Sources {
    attribute_increases: ByAttribute<u16>,
    classes: Bounded<ClassId, { limits::HIT_DICE }>,
    talents: Bounded<TalentId, { limits::OWNED_TALENTS }>,
});
record!(HistoryRow {
    owner: crate::HdIndex,
    #[serde(deserialize_with = "named_map")]
    source: SourceRecord,
    health_seed: u64,
    #[serde(deserialize_with = "named_map")]
    choices: ChoiceRecord,
});
record!(Birth {
    attributes: ByAttribute<u16>,
    templates: Bounded<TemplateId, { limits::TEMPLATES }>,
    choices: Bounded<u16, { limits::HIT_DICE }>,
    sources: Sources,
});
record!(Acquisition {
    before_choices: Bounded<u16, { limits::HIT_DICE }>,
    after_choices: Bounded<u16, { limits::HIT_DICE }>,
    before: Sources,
    after: Sources,
});
record!(Threshold {
    acquisition: u16,
    #[serde(deserialize_with = "required_option")]
    expertise: Option<ExpertiseId>,
});
record!(Learning {
    birth: Birth,
    history: Bounded<HistoryRow, { limits::EXPERTISE_HISTORY_ROWS }>,
    acquisitions: Bounded<Acquisition, { limits::DERIVED_ATTRIBUTE as usize }>,
    thresholds: Bounded<Threshold, { limits::DERIVED_ATTRIBUTE as usize }>,
});
impl Sources {
    fn capture(value: &IntellectSources) -> Self {
        Self {
            attribute_increases: value.attribute_increases,
            classes: Bounded(value.classes.iter().cloned().collect()),
            talents: Bounded(value.talents.iter().cloned().collect()),
        }
    }
    fn restore(&self) -> Result<IntellectSources, BuildError> {
        let classes = self
            .classes
            .0
            .iter()
            .cloned()
            .collect::<std::collections::BTreeSet<_>>();
        let talents = self
            .talents
            .0
            .iter()
            .cloned()
            .collect::<std::collections::BTreeSet<_>>();
        if classes.len() != self.classes.0.len() || talents.len() != self.talents.0.len() {
            return Err(BuildError::InvalidOwnership);
        }
        Ok(IntellectSources {
            attribute_increases: self.attribute_increases,
            classes,
            talents,
        })
    }
}
impl Learning {
    pub(super) fn capture(value: &domain::IntellectLearning) -> Self {
        Self {
            birth: Birth {
                attributes: value.birth.attributes.values(),
                templates: Bounded(value.birth.templates.clone()),
                choices: Bounded(value.birth.choices.clone()),
                sources: Sources::capture(&value.birth.sources),
            },
            history: Bounded(
                value
                    .history
                    .iter()
                    .map(|row| HistoryRow {
                        owner: row.owner,
                        source: SourceRecord::capture(row.source.clone()),
                        health_seed: row.health_seed,
                        choices: ChoiceRecord::capture(&row.choices),
                    })
                    .collect(),
            ),
            acquisitions: Bounded(
                value
                    .acquisitions
                    .iter()
                    .map(|row| Acquisition {
                        before_choices: Bounded(row.before_choices.clone()),
                        after_choices: Bounded(row.after_choices.clone()),
                        before: Sources::capture(&row.before),
                        after: Sources::capture(&row.after),
                    })
                    .collect(),
            ),
            thresholds: Bounded(
                value
                    .thresholds
                    .iter()
                    .map(|row| Threshold {
                        acquisition: row.acquisition,
                        expertise: row.expertise.clone(),
                    })
                    .collect(),
            ),
        }
    }
    pub(super) fn restore(
        &self,
        build: &CreatureBuild,
    ) -> Result<domain::IntellectLearning, BuildError> {
        let value = domain::IntellectLearning {
            birth: domain::IntellectBirthWitness {
                attributes: Attributes::new(self.birth.attributes)
                    .map_err(|_| BuildError::AttributeCap)?,
                templates: self.birth.templates.0.clone(),
                choices: self.birth.choices.0.clone(),
                sources: self.birth.sources.restore()?,
            },
            history: self
                .history
                .0
                .iter()
                .map(|row| domain::IntellectChoiceWitness {
                    owner: row.owner,
                    source: row.source.restore(),
                    health_seed: row.health_seed,
                    choices: row.choices.restore(),
                })
                .collect(),
            acquisitions: self
                .acquisitions
                .0
                .iter()
                .map(|row| {
                    Ok(domain::IntellectAcquisition {
                        before_choices: row.before_choices.0.clone(),
                        after_choices: row.after_choices.0.clone(),
                        before: row.before.restore()?,
                        after: row.after.restore()?,
                    })
                })
                .collect::<Result<_, BuildError>>()?,
            thresholds: self
                .thresholds
                .0
                .iter()
                .map(|row| domain::IntellectThreshold {
                    acquisition: row.acquisition,
                    expertise: row.expertise.clone(),
                })
                .collect(),
        };
        value.validate(build)?;
        Ok(value)
    }
}
