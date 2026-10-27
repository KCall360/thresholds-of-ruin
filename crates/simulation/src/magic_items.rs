//! Item-owned magic capabilities, exact-instance knowledge and one paid choice.
use crate::{ActorId, Game, GameError, IntentionId, ItemClass, ItemId, ItemLocation};
use serde::{Deserialize, Serialize};

const MAX_KNOWN_CHARGES: usize = 4096;
pub const MAX_IDENTIFICATION_ITEMS: usize = 64;

/// Requested input is distinct from the device's frozen delivery.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum DeviceAim {
    Direction {
        direction: crate::abilities::StepDirection,
    },
    SelfTarget {},
}
impl<'de> Deserialize<'de> for DeviceAim {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            Direction {
                direction: crate::abilities::StepDirection,
            },
            SelfTarget {},
        }
        Ok(match crate::bounded::named_map(d)? {
            Fields::Direction { direction } => Self::Direction { direction },
            Fields::SelfTarget {} => Self::SelfTarget {},
        })
    }
}
/// Disclosed input permission, never an unknown item's hidden delivery profile.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceActivation {
    Direction,
    SelfTarget,
    DirectionOrSelf,
}
impl<'de> Deserialize<'de> for DeviceActivation {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        match String::deserialize(d)?.as_str() {
            "direction" => Ok(Self::Direction),
            "self_target" => Ok(Self::SelfTarget),
            "direction_or_self" => Ok(Self::DirectionOrSelf),
            _ => Err(serde::de::Error::custom("invalid device activation")),
        }
    }
}

/// Recognition requires an authored trigger and a committed owner witness.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemDiscovery {
    #[default]
    SelfEffect,
    OnUse,
    DiscernibleEffect,
    Never,
}

impl<'de> Deserialize<'de> for ItemDiscovery {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        match String::deserialize(d)?.as_str() {
            "self_effect" => Ok(Self::SelfEffect),
            "on_use" => Ok(Self::OnUse),
            "discernible_effect" => Ok(Self::DiscernibleEffect),
            "never" => Ok(Self::Never),
            _ => Err(serde::de::Error::custom("invalid item discovery policy")),
        }
    }
}

/// Charge counts are not physical stack quantities. Zero is a valid empty device.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ChargeRange {
    pub minimum: u16,
    pub maximum: u16,
}
impl<'de> Deserialize<'de> for ChargeRange {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            minimum: u16,
            maximum: u16,
        }
        let f: Fields = crate::bounded::named_map(d)?;
        let range = Self {
            minimum: f.minimum,
            maximum: f.maximum,
        };
        if !range.valid() {
            return Err(serde::de::Error::custom("invalid charge range"));
        }
        Ok(range)
    }
}
impl ChargeRange {
    pub fn valid(self) -> bool {
        self.minimum <= self.maximum
    }
    pub(crate) fn sample(self, rng: &mut u64) -> Result<u16, GameError> {
        let range = crate::quantity::QuantityRange::new(
            u64::from(self.minimum) + 1,
            u64::from(self.maximum) + 1,
        )?;
        u16::try_from(range.sample(rng)? - 1).map_err(|_| GameError::ItemUnavailable)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReadableSpec {
    pub control: Option<crate::control_scrolls::ControlSpec>,
    pub descriptors: Vec<crate::content_ids::DescriptorId>,
    pub effects: Vec<crate::rules::Effect>,
    pub identify: Option<Vec<ItemClass>>,
    pub discovery: ItemDiscovery,
}
impl<'de> Deserialize<'de> for ReadableSpec {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            #[serde(deserialize_with = "crate::bounded::required_option")]
            control: Option<crate::control_scrolls::ControlSpec>,
            #[serde(deserialize_with = "read_payload_descriptors")]
            descriptors: Vec<crate::content_ids::DescriptorId>,
            #[serde(deserialize_with = "read_readable_effects")]
            effects: Vec<crate::rules::Effect>,
            #[serde(deserialize_with = "crate::bounded::required_option")]
            identify: Option<IdentificationClasses>,
            discovery: ItemDiscovery,
        }
        let f: Fields = crate::bounded::named_map(d)?;
        let spec = Self {
            control: f.control,
            descriptors: f.descriptors,
            effects: f.effects,
            identify: f.identify.map(|classes| classes.0),
            discovery: f.discovery,
        };
        if !spec.valid() {
            return Err(serde::de::Error::custom("invalid readable capability"));
        }
        Ok(spec)
    }
}
pub(crate) fn read_payload_descriptors<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Vec<crate::content_ids::DescriptorId>, D::Error> {
    Ok(crate::bounded::Bounded::<
        crate::content_ids::DescriptorId,
        { crate::limits::DESCRIPTORS_PER_EFFECT },
    >::deserialize(d)?
    .0)
}
pub(crate) fn valid_payload_descriptor_shape(
    descriptors: &[crate::content_ids::DescriptorId],
) -> bool {
    descriptors.len() <= crate::limits::DESCRIPTORS_PER_EFFECT
        && descriptors
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            == descriptors.len()
}
pub(crate) fn read_readable_effects<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Vec<crate::rules::Effect>, D::Error> {
    Ok(crate::bounded::Bounded::<crate::rules::Effect,{crate::limits::CONSUMABLE_EFFECTS}>::deserialize(d)?.0)
}
struct IdentificationClasses(Vec<ItemClass>);
impl<'de> Deserialize<'de> for IdentificationClasses {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Token(#[serde(deserialize_with = "read_identification_class")] ItemClass);
        fn read_identification_class<'de, D: serde::Deserializer<'de>>(
            d: D,
        ) -> Result<ItemClass, D::Error> {
            let token = String::deserialize(d)?;
            ItemClass::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(token))
        }
        let tokens = crate::bounded::Bounded::<Token, 14>::deserialize(d)?.0;
        let classes: Vec<_> = tokens.into_iter().map(|token| token.0).collect();
        if !valid_identification_classes(&classes) {
            return Err(serde::de::Error::custom("invalid identification class set"));
        }
        Ok(Self(classes))
    }
}
fn read_identification_classes<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Vec<ItemClass>, D::Error> {
    IdentificationClasses::deserialize(d).map(|classes| classes.0)
}
pub(crate) fn valid_identification_classes(classes: &[ItemClass]) -> bool {
    !classes.is_empty() && classes.len() <= 14 && classes.windows(2).all(|pair| pair[0] < pair[1])
}
impl ReadableSpec {
    pub fn valid(&self) -> bool {
        if self.discovery == ItemDiscovery::DiscernibleEffect {
            return false;
        }
        if let Some(control) = &self.control {
            return control.valid_shape()
                && self.identify.is_none()
                && self.effects.is_empty()
                && self.descriptors.is_empty();
        }
        if !valid_payload_descriptor_shape(&self.descriptors) {
            return false;
        }
        match &self.identify {
            Some(classes) => {
                valid_identification_classes(classes)
                    && self.descriptors.is_empty()
                    && self.effects.is_empty()
                    && self.discovery == ItemDiscovery::OnUse
            }
            None => {
                !self.effects.is_empty()
                    && self.effects.len() <= crate::limits::CONSUMABLE_EFFECTS
                    && self.effects.iter().all(|e| {
                        e.valid_shape()
                            && (!e.spatial()
                                || (self.effects.len() == 1
                                    && matches!(e, crate::rules::Effect::TeleportRandom)))
                    })
            }
        }
    }
    pub fn validate_rules(
        &self,
        rules: &crate::rules::Ruleset,
    ) -> Result<(), crate::rules::RulesetError> {
        if !self.valid()
            || self
                .control
                .as_ref()
                .is_some_and(|control| !control.valid(rules))
        {
            return Err(crate::rules::RulesetError::Bound {
                field: "readable".into(),
            });
        }
        rules.validate_payload_recipients(
            &self.descriptors,
            self.effects.iter().map(crate::rules::Effect::recipient),
        )?;
        rules.validate_effects(&self.effects)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceSpec {
    pub ability: crate::content_ids::AbilityId,
    pub initial_charges: ChargeRange,
    pub discovery: ItemDiscovery,
}
impl<'de> Deserialize<'de> for DeviceSpec {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            ability: crate::content_ids::AbilityId,
            initial_charges: ChargeRange,
            discovery: ItemDiscovery,
        }
        let f: Fields = crate::bounded::named_map(d)?;
        Ok(Self {
            ability: f.ability,
            initial_charges: f.initial_charges,
            discovery: f.discovery,
        })
    }
}
impl DeviceSpec {
    pub fn valid(&self) -> bool {
        self.initial_charges.valid()
            && matches!(
                self.discovery,
                ItemDiscovery::OnUse | ItemDiscovery::DiscernibleEffect | ItemDiscovery::Never
            )
    }
    pub fn validate_rules(
        &self,
        rules: &crate::rules::Ruleset,
    ) -> Result<(), crate::rules::RulesetError> {
        if !self.valid()
            || !crate::abilities::device_definition_valid(rules, &self.ability)
            || (self.discovery == ItemDiscovery::DiscernibleEffect
                && !rules
                    .ability(&self.ability)
                    .is_some_and(crate::abilities::discernible_device_definition_valid))
        {
            return Err(crate::rules::RulesetError::Bound {
                field: "device".into(),
            });
        }
        Ok(())
    }
}
impl crate::ItemSpec {
    pub(crate) fn magic_shape_valid(&self) -> bool {
        let exclusive = usize::from(self.equipment.is_some() || self.projectile.is_some())
            + usize::from(self.consumable.is_some())
            + usize::from(self.edible.is_some())
            + usize::from(self.readable.is_some())
            + usize::from(self.device.is_some());
        let physical_device = self.device.is_some()
            && self.projectile.is_none()
            && self.ground_scare.is_none()
            && self
                .equipment
                .as_ref()
                .is_some_and(|equipment| equipment.supports_device(self.class));
        (exclusive <= 1 || (exclusive == 2 && physical_device))
            && self
                .ground_scare
                .as_ref()
                .is_none_or(|spec| spec.valid_shape() && self.class == ItemClass::Scroll)
            && self
                .readable
                .as_ref()
                .is_none_or(|s| s.valid() && self.supply.is_none() && self.currency.is_none())
            && self.device.as_ref().is_none_or(|s| {
                s.valid() && !self.stackable && self.supply.is_none() && self.currency.is_none()
            })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct IdentificationChoice {
    pub choice: IntentionId,
    pub classes: Vec<ItemClass>,
}
impl<'de> Deserialize<'de> for IdentificationChoice {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            choice: IntentionId,
            #[serde(deserialize_with = "read_identification_classes")]
            classes: Vec<ItemClass>,
        }
        let f: Fields = crate::bounded::named_map(d)?;
        if f.choice.0 == 0 || f.choice.0 == u64::MAX || !valid_identification_classes(&f.classes) {
            return Err(serde::de::Error::custom("invalid identification choice"));
        }
        Ok(Self {
            choice: f.choice,
            classes: f.classes,
        })
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IdentificationChoiceView {
    pub choice: IntentionId,
    pub classes: Vec<ItemClass>,
    pub items: Vec<ItemId>,
}
/// A remembered balance is a fact from this actor's observation, not live access
/// to the device. Other actors' payments cannot update it or require an actor scan.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub(crate) struct ChargeKnowledge(Vec<ChargeFact>);
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
struct ChargeFact {
    item: ItemId,
    charges: u16,
}
impl<'de> Deserialize<'de> for ChargeFact {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            item: ItemId,
            charges: u16,
        }
        let f: Fields = crate::bounded::named_map(d)?;
        Ok(Self {
            item: f.item,
            charges: f.charges,
        })
    }
}
impl ChargeKnowledge {
    pub(crate) fn len(&self) -> usize {
        self.0.len()
    }
    #[cfg(test)]
    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    fn index(&self, item: ItemId) -> Result<usize, usize> {
        self.0.binary_search_by_key(&item, |f| f.item)
    }
    pub(crate) fn contains(&self, item: &ItemId) -> bool {
        self.index(*item).is_ok()
    }
    pub(crate) fn observed(&self, item: ItemId, actual: Option<u16>) -> Option<u16> {
        let fact = &self.0[self.index(item).ok()?];
        actual.filter(|balance| *balance == fact.charges)
    }
    fn insert(&mut self, item: ItemId, charges: u16) {
        match self.index(item) {
            Ok(index) => self.0[index].charges = charges,
            Err(index) => self.0.insert(index, ChargeFact { item, charges }),
        }
    }
}
pub(crate) fn read_known_charges<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<tor_world::Shared<ChargeKnowledge>, D::Error> {
    let facts = crate::bounded::Bounded::<ChargeFact, MAX_KNOWN_CHARGES>::deserialize(d)?.0;
    if facts.windows(2).any(|p| p[0].item >= p[1].item)
        || facts.iter().any(|f| f.item.0 == 0 || f.item.0 == u64::MAX)
    {
        return Err(serde::de::Error::custom("invalid charge knowledge"));
    }
    Ok(tor_world::Shared::new(ChargeKnowledge(facts)))
}

impl Game {
    pub(crate) fn identification_view(&self, actor: ActorId) -> Option<IdentificationChoiceView> {
        let pending = self.actors.get(&actor)?.identification.as_ref()?;
        Some(IdentificationChoiceView {
            choice: pending.choice,
            classes: pending.classes.clone(),
            items: self
                .items
                .at(ItemLocation::Carried(actor))
                .filter(|id| {
                    self.items
                        .get(id)
                        .is_some_and(|item| pending.classes.contains(&item.spec.class))
                })
                .take(MAX_IDENTIFICATION_ITEMS)
                .collect(),
        })
    }
    pub(crate) fn validate_identification(
        &self,
        actor: ActorId,
        choice: IntentionId,
        item: Option<ItemId>,
    ) -> Result<(), GameError> {
        let owner = self
            .actors
            .get(&actor)
            .filter(|a| a.alive())
            .ok_or(GameError::ItemUnavailable)?;
        let pending = owner
            .identification
            .as_ref()
            .filter(|p| p.choice == choice)
            .ok_or(GameError::ItemUnavailable)?;
        if let Some(id) = item {
            if !self
                .identification_view(actor)
                .is_some_and(|v| v.items.contains(&id))
                || self.items.get(&id).is_none_or(|i| {
                    i.location != ItemLocation::Carried(actor)
                        || !pending.classes.contains(&i.spec.class)
                })
            {
                return Err(GameError::ItemUnavailable);
            }
            if self
                .items
                .get(&id)
                .is_some_and(|i| i.spec.has_instance_condition())
                && !owner.known_equipment.can_learn(id)
            {
                return Err(GameError::ItemUnavailable);
            }
            if self.items.get(&id).is_some_and(|i| i.charges.is_some())
                && owner.known_charges.len() == MAX_KNOWN_CHARGES
                && !owner.known_charges.contains(&id)
            {
                return Err(GameError::ItemUnavailable);
            }
        }
        Ok(())
    }
    pub(crate) fn resolve_identification(
        &mut self,
        actor: ActorId,
        choice: IntentionId,
        item: Option<ItemId>,
    ) -> Result<(), GameError> {
        self.validate_identification(actor, choice, item)?;
        let learned = item.map(|id| {
            let i = &self.items[&id];
            (
                id,
                i.spec.identity.clone(),
                i.charges,
                i.spec.has_instance_condition().then_some(i.condition),
            )
        });
        let mut outcome = self.actor_outcome(actor)?;
        if let Some((id, identity, charges, condition)) = learned {
            if let Some(condition) = condition {
                outcome.state.known_equipment.identify(id, condition)?;
            }
            outcome.state.knowledge.insert(identity);
            if let Some(charges) = charges {
                outcome.state.known_charges.insert(id, charges);
            }
        }
        outcome.state.identification = None;
        self.apply_outcome(outcome)
    }
    pub(crate) fn magic_knowledge_valid(&self, actor: ActorId) -> bool {
        let a = &self.actors[&actor];
        self.equipment_knowledge_valid(actor)
            && a.known_charges.len() <= MAX_KNOWN_CHARGES
            && a.known_charges.0.iter().all(|fact| {
                fact.item.0 > 0
                    && fact.item.0 < self.next_item_id
                    && self.items.get(&fact.item).is_none_or(|item| {
                        item.spec.device.as_ref().is_some_and(|device| {
                            fact.charges <= device.initial_charges.maximum
                                && item.charges.is_some_and(|balance| balance <= fact.charges)
                                && a.knowledge.contains(&item.spec.identity)
                        })
                    })
            })
            && a.identification
                .as_ref()
                .is_none_or(|p| a.alive() && self.intention_issued(p.choice))
    }
    /// Update only a balance that the paying actor previously knew. Other
    /// actors retain their own old facts, never a live reference to hidden use.
    pub(crate) fn charge_payment_observed(&mut self, actor: ActorId, item: ItemId, before: u16) {
        let after = self.items.get(&item).and_then(|i| i.charges);
        let mut owner = self.actors.get_mut(&actor).expect("charge owner");
        if owner.known_charges.observed(item, Some(before)).is_some() {
            owner
                .known_charges
                .insert(item, after.expect("device charge state"));
        }
    }
}

#[cfg(test)]
mod identification_format_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn discernible_device_policy_roundtrips_without_aliasing_other_policies() {
        let value = json!({"ability":"fixture_ray", "initial_charges":{"minimum":2,"maximum":2}, "discovery":"discernible_effect"});
        let device: DeviceSpec = serde_json::from_value(value.clone()).unwrap();
        assert!(device.valid());
        assert_eq!(device.discovery, ItemDiscovery::DiscernibleEffect);
        assert_eq!(serde_json::to_value(device).unwrap(), value);
        assert!(serde_json::from_value::<ItemDiscovery>(json!("discernible")).is_err());
        assert_ne!(ItemDiscovery::DiscernibleEffect, ItemDiscovery::OnUse);
        assert_ne!(ItemDiscovery::DiscernibleEffect, ItemDiscovery::Never);
    }

    #[test]
    fn runtime_identification_sets_are_required_bounded_scalar_sorted_and_unique() {
        let classes = json!(["ring", "wand"]);
        let readable = json!({"control": null, "effects": [], "descriptors": [], "identify": classes, "discovery": "on_use"});
        let choice = json!({"choice": 1, "classes": classes});
        assert!(serde_json::from_value::<ReadableSpec>(readable.clone()).is_ok());
        assert!(serde_json::from_value::<IdentificationChoice>(choice.clone()).is_ok());
        for invalid in [
            json!([]),
            json!(["ring", "ring"]),
            json!(["wand", "ring"]),
            json!([{"ring": null}]),
            json!("ring"),
            json!(vec!["ring"; 15]),
        ] {
            let mut bad_readable = readable.clone();
            bad_readable["identify"] = invalid.clone();
            assert!(serde_json::from_value::<ReadableSpec>(bad_readable).is_err());
            let mut bad_choice = choice.clone();
            bad_choice["classes"] = invalid;
            assert!(serde_json::from_value::<IdentificationChoice>(bad_choice).is_err());
        }
        let mut missing = readable;
        missing.as_object_mut().unwrap().remove("identify");
        assert!(serde_json::from_value::<ReadableSpec>(missing).is_err());
        let mut missing = choice;
        missing.as_object_mut().unwrap().remove("classes");
        assert!(serde_json::from_value::<IdentificationChoice>(missing).is_err());
    }

    #[test]
    fn oversized_runtime_identification_sequence_stops_before_decoding_excess_token() {
        let mut classes = vec![json!("ring"); 14];
        classes.push(json!({"not": "a class token"}));
        let error = serde_json::from_str::<IdentificationClasses>(
            &serde_json::to_string(&classes).unwrap(),
        )
        .err()
        .unwrap();
        assert!(error.to_string().contains("collection exceeds limit"));
    }
}
