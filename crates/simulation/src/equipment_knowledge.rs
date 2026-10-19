//! Exact-instance remembered equipment facts. Observations never consult hidden
//! current condition to erase or refresh another actor's historical knowledge.
use crate::armor::ItemCondition;
use crate::{ActorId, EquipmentSpec, Game, GameError, ItemId};
use serde::{Deserialize, Serialize};

const MAX_EQUIPMENT_FACTS: usize = 4096;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct EquipmentNumbers {
    corrosion: u32,
    armor_penalty: u32,
    armor_enchantment: u32,
    hit_enchantment: i32,
    damage_enchantment: i32,
}
impl From<ItemCondition> for EquipmentNumbers {
    fn from(value: ItemCondition) -> Self {
        Self {
            corrosion: value.corrosion,
            armor_penalty: value.armor_penalty,
            armor_enchantment: value.armor_enchantment,
            hit_enchantment: value.hit_enchantment,
            damage_enchantment: value.damage_enchantment,
        }
    }
}
impl EquipmentNumbers {
    fn condition(self) -> ItemCondition {
        ItemCondition {
            corrosion: self.corrosion,
            armor_penalty: self.armor_penalty,
            armor_enchantment: self.armor_enchantment,
            hit_enchantment: self.hit_enchantment,
            damage_enchantment: self.damage_enchantment,
            ..ItemCondition::default()
        }
    }
}
impl<'de> Deserialize<'de> for EquipmentNumbers {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            corrosion: u32,
            armor_penalty: u32,
            armor_enchantment: u32,
            hit_enchantment: i32,
            damage_enchantment: i32,
        }
        let f: Fields = crate::bounded::named_map(d)?;
        let result = Self {
            corrosion: f.corrosion,
            armor_penalty: f.armor_penalty,
            armor_enchantment: f.armor_enchantment,
            hit_enchantment: f.hit_enchantment,
            damage_enchantment: f.damage_enchantment,
        };
        if !result.condition().valid() {
            return Err(serde::de::Error::custom("invalid equipment numbers"));
        }
        Ok(result)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
struct EquipmentFact {
    item: ItemId,
    numbers: Option<EquipmentNumbers>,
    cursed: Option<bool>,
    protected: Option<bool>,
}
impl<'de> Deserialize<'de> for EquipmentFact {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            item: ItemId,
            #[serde(deserialize_with = "crate::bounded::required_option")]
            numbers: Option<EquipmentNumbers>,
            #[serde(deserialize_with = "crate::bounded::required_option")]
            cursed: Option<bool>,
            #[serde(deserialize_with = "crate::bounded::required_option")]
            protected: Option<bool>,
        }
        let f: Fields = crate::bounded::named_map(d)?;
        if f.item.0 == 0
            || f.item.0 == u64::MAX
            || (f.numbers.is_none() && f.cursed.is_none() && f.protected.is_none())
        {
            return Err(serde::de::Error::custom("invalid equipment fact"));
        }
        Ok(Self {
            item: f.item,
            numbers: f.numbers,
            cursed: f.cursed,
            protected: f.protected,
        })
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub(crate) struct EquipmentKnowledge(Vec<EquipmentFact>);
impl EquipmentKnowledge {
    fn index(&self, item: ItemId) -> Result<usize, usize> {
        self.0.binary_search_by_key(&item, |fact| fact.item)
    }
    fn fact(&self, item: ItemId) -> Option<&EquipmentFact> {
        self.index(item).ok().map(|index| &self.0[index])
    }
    pub(crate) fn can_learn(&self, item: ItemId) -> bool {
        self.index(item).is_ok() || self.0.len() < MAX_EQUIPMENT_FACTS
    }
    fn fact_mut(&mut self, item: ItemId) -> Result<&mut EquipmentFact, GameError> {
        let index = match self.index(item) {
            Ok(index) => index,
            Err(index) => {
                if !self.can_learn(item) {
                    return Err(GameError::ItemUnavailable);
                }
                self.0.insert(
                    index,
                    EquipmentFact {
                        item,
                        numbers: None,
                        cursed: None,
                        protected: None,
                    },
                );
                index
            }
        };
        Ok(&mut self.0[index])
    }
    pub(crate) fn learn_numbers(
        &mut self,
        item: ItemId,
        condition: ItemCondition,
    ) -> Result<(), GameError> {
        self.fact_mut(item)?.numbers = Some(condition.into());
        Ok(())
    }
    pub(crate) fn learn_curse(&mut self, item: ItemId, cursed: bool) -> Result<(), GameError> {
        self.fact_mut(item)?.cursed = Some(cursed);
        Ok(())
    }
    pub(crate) fn identify(
        &mut self,
        item: ItemId,
        condition: ItemCondition,
    ) -> Result<(), GameError> {
        let fact = self.fact_mut(item)?;
        fact.numbers = Some(condition.into());
        fact.cursed = Some(condition.cursed);
        fact.protected = Some(condition.protected);
        Ok(())
    }
    /// Only an observed own mutation may advance existing facts. A stale fact
    /// remains a remembered fact; it cannot acquire an unobserved intervening value.
    pub(crate) fn observe_change(
        &mut self,
        item: ItemId,
        before: ItemCondition,
        after: ItemCondition,
    ) {
        let Ok(index) = self.index(item) else {
            return;
        };
        let fact = &mut self.0[index];
        if fact.numbers == Some(before.into()) {
            fact.numbers = Some(after.into());
        }
        if fact.cursed == Some(before.cursed) {
            fact.cursed = Some(after.cursed);
        }
        if fact.protected == Some(before.protected) {
            fact.protected = Some(after.protected);
        }
    }
    pub(crate) fn curse(&self, item: ItemId) -> Option<bool> {
        self.fact(item)?.cursed
    }
    pub(crate) fn protection(&self, item: ItemId) -> Option<bool> {
        self.fact(item)?.protected
    }
    pub(crate) fn remembered_condition(&self, item: ItemId) -> Option<ItemCondition> {
        let fact = self.fact(item)?;
        Some(ItemCondition {
            cursed: fact.cursed.unwrap_or(false),
            protected: fact.protected.unwrap_or(false),
            ..fact.numbers?.condition()
        })
    }
}
pub(crate) fn read_known_equipment<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<tor_world::Shared<EquipmentKnowledge>, D::Error> {
    let facts = crate::bounded::Bounded::<EquipmentFact, MAX_EQUIPMENT_FACTS>::deserialize(d)?.0;
    if facts.windows(2).any(|pair| pair[0].item >= pair[1].item) {
        return Err(serde::de::Error::custom(
            "invalid equipment knowledge order",
        ));
    }
    Ok(tor_world::Shared::new(EquipmentKnowledge(facts)))
}
impl Game {
    pub(crate) fn known_equipment_view(&self, actor: ActorId, id: ItemId) -> Option<EquipmentSpec> {
        let owner = self.actors.get(&actor)?;
        let item = self.items.get(&id)?;
        if item.spec.concealed && !owner.knowledge.contains(&item.spec.identity) {
            return None;
        }
        let mut remembered = item.clone();
        remembered.condition = owner.known_equipment.remembered_condition(id)?;
        remembered.equipment_view()
    }
    pub(crate) fn equipment_knowledge_valid(&self, actor: ActorId) -> bool {
        let owner = &self.actors[&actor];
        owner.known_equipment.0.len() <= MAX_EQUIPMENT_FACTS
            && owner.known_equipment.0.iter().all(|fact| {
                fact.item.0 > 0
                    && fact.item.0 < self.next_item_id
                    && self.items.get(&fact.item).is_none_or(|item| {
                        if item.spec.equipment.is_none() {
                            return false;
                        }
                        fact.numbers.is_none_or(|numbers| {
                            let mut remembered = item.clone();
                            remembered.condition = numbers.condition();
                            remembered.condition_valid()
                        })
                    })
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn partial_facts_remain_private_and_hidden_changes_do_not_erase_or_refresh_memory() {
        let item = ItemId(1);
        let original = ItemCondition {
            cursed: true,
            hit_enchantment: -2,
            ..ItemCondition::default()
        };
        let mut known = EquipmentKnowledge::default();
        known.learn_curse(item, true).unwrap();
        assert!(known.remembered_condition(item).is_none());
        let changed = ItemCondition {
            cursed: false,
            hit_enchantment: -1,
            ..original
        };
        known.observe_change(item, original, changed);
        assert_eq!(known.curse(item), Some(false));
        assert!(known.remembered_condition(item).is_none());
        known.identify(item, original).unwrap();
        let hidden = ItemCondition {
            hit_enchantment: 4,
            ..changed
        };
        assert_eq!(known.remembered_condition(item), Some(original));
        known.observe_change(
            item,
            hidden,
            ItemCondition {
                hit_enchantment: 5,
                ..hidden
            },
        );
        assert_eq!(known.remembered_condition(item), Some(original));
        assert!(known.remembered_condition(ItemId(2)).is_none());
    }
    #[test]
    fn armor_number_learning_does_not_learn_curse_or_protection_and_known_changes_preserve_masks() {
        let id = ItemId(3);
        let before = ItemCondition {
            cursed: true,
            protected: true,
            armor_enchantment: 2,
            ..ItemCondition::default()
        };
        let mut facts = EquipmentKnowledge::default();
        facts.learn_numbers(id, before).unwrap();
        assert_eq!(facts.curse(id), None);
        assert_eq!(facts.protection(id), None);
        let after = ItemCondition {
            corrosion: 1,
            ..before
        };
        facts.observe_change(id, before, after);
        assert_eq!(facts.remembered_condition(id).unwrap().corrosion, 1);
        assert_eq!(facts.curse(id), None);
        assert_eq!(facts.protection(id), None);
    }
    #[test]
    fn equipment_fact_records_require_named_fields_and_strict_bounded_sorted_instances() {
        #[derive(Deserialize)]
        struct Record {
            #[serde(deserialize_with = "read_known_equipment")]
            facts: tor_world::Shared<EquipmentKnowledge>,
        }
        let mut known = EquipmentKnowledge::default();
        known.learn_curse(ItemId(1), true).unwrap();
        let fact = serde_json::to_value(&known).unwrap()[0].clone();
        for field in ["item", "numbers", "cursed", "protected"] {
            let mut value = fact.clone();
            value.as_object_mut().unwrap().remove(field);
            assert!(serde_json::from_value::<EquipmentFact>(value).is_err());
        }
        let mut extra = fact.clone();
        extra["other"] = true.into();
        assert!(serde_json::from_value::<EquipmentFact>(extra).is_err());
        assert!(
            serde_json::from_value::<EquipmentFact>(serde_json::json!([1, null, true, null]))
                .is_err()
        );
        assert!(serde_json::from_value::<Record>(
            serde_json::json!({"facts":[fact.clone(),fact.clone()]})
        )
        .is_err());
        let round: Record = serde_json::from_value(serde_json::json!({"facts":[fact]})).unwrap();
        assert_eq!(round.facts.curse(ItemId(1)), Some(true));
        assert_eq!(round.facts.remembered_condition(ItemId(1)), None);
    }
    #[test]
    fn curse_blocked_event_requires_named_exact_item_and_intention_fields() {
        let event = crate::combat::CombatEvent::ItemCurseBlocked {
            actor: ActorId(1),
            item: ItemId(2),
            intention: None,
        };
        let value = serde_json::to_value(&event).unwrap();
        assert_eq!(
            serde_json::from_value::<crate::combat::CombatEvent>(value.clone()).unwrap(),
            event
        );
        for field in ["actor", "item", "intention"] {
            let mut missing = value.clone();
            missing["ItemCurseBlocked"]
                .as_object_mut()
                .unwrap()
                .remove(field);
            assert!(serde_json::from_value::<crate::combat::CombatEvent>(missing).is_err());
        }
        assert!(
            serde_json::from_value::<crate::combat::CombatEvent>(serde_json::json!({
                "ItemCurseBlocked": [1,2,null]
            }))
            .is_err()
        );
        let mut invalid = value;
        invalid["ItemCurseBlocked"]["item"] = serde_json::json!(0);
        assert!(serde_json::from_value::<crate::combat::CombatEvent>(invalid).is_err());
    }
}
