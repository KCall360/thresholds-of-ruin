//! Owner-disclosed equipment, independent of rendering and action admission.
use tor_protocol::{EquipmentSlot, ItemInteractionView, ItemTarget, Observation};

#[derive(Clone, Debug, Default)]
pub struct Equipment {
    /// Preserve socket order and duplicates: two ring sockets are two positions.
    pub slots: Vec<EquipmentSlot>,
    pub items: Vec<ItemInteractionView>,
}

impl Equipment {
    pub fn from_observation(observation: &Observation) -> Self {
        let Some(interactions) = &observation.interactions else {
            return Self::default();
        };
        let carried: std::collections::HashSet<_> =
            observation.inventory.iter().map(|item| item.id).collect();
        Self {
            slots: interactions.slots.clone(),
            items: interactions
                .inventory
                .iter()
                .filter(|item| carried.contains(&item.item))
                .cloned()
                .collect(),
        }
    }

    pub fn item(&self, target: ItemTarget) -> Option<&ItemInteractionView> {
        self.items.iter().find(|item| item.item == target)
    }

    pub fn occupant(&self, slot: u16) -> Option<ItemTarget> {
        self.items
            .iter()
            .find(|item| item.equipped_slot == Some(slot))
            .map(|item| item.item)
    }

    pub fn compatible_slots(&self, target: ItemTarget) -> impl Iterator<Item = u16> + '_ {
        let kind = self
            .item(target)
            .and_then(|item| item.slot.as_ref())
            .map(|slot| slot.id.as_str());
        self.slots
            .iter()
            .enumerate()
            .filter_map(move |(index, slot)| {
                (Some(slot.id.as_str()) == kind)
                    .then(|| u16::try_from(index).ok())
                    .flatten()
            })
    }

    /// Choose a comparison only. Equipping still uses the shared admission path.
    pub fn comparison_slot(&self, target: ItemTarget, preferred: Option<u16>) -> Option<u16> {
        if let Some(slot) = self.item(target).and_then(|item| item.equipped_slot) {
            return Some(slot);
        }
        preferred
            .filter(|slot| {
                self.compatible_slots(target)
                    .any(|candidate| candidate == *slot)
            })
            .or_else(|| {
                self.compatible_slots(target)
                    .find(|slot| self.occupant(*slot).is_some())
            })
            .or_else(|| self.compatible_slots(target).next())
    }

    pub fn has_free_slot(&self, target: ItemTarget) -> bool {
        self.compatible_slots(target)
            .any(|slot| self.occupant(slot).is_none())
    }

    /// Remembered properties are separate from unknown properties and from totals.
    pub fn facts(&self, target: ItemTarget) -> Vec<String> {
        let Some(item) = self.item(target) else {
            return vec!["Properties not known.".into()];
        };
        let mut facts = Vec::new();
        if let Some(known) = &item.known_equipment {
            if let Some(attack) = &known.attack {
                facts.push(format!(
                    "{} attack bonus: {:+}",
                    attack.skill.name, attack.bonus
                ));
                facts.push(format!(
                    "Damage enchantment: {:+}",
                    attack.damage_enchantment
                ));
                facts.push(format!(
                    "Wind-up: {} ticks  |  Recovery: {} ticks",
                    attack.wind_up, attack.recovery
                ));
                for component in &attack.damage.components {
                    let amount = match component.amount {
                        tor_protocol::DamageAmountView::Fixed { value } => value.to_string(),
                        tor_protocol::DamageAmountView::Rolled {
                            count,
                            sides,
                            bonus,
                        } => format!("{count}d{sides}{bonus:+}"),
                    };
                    let category = component.descriptor.as_ref().map_or_else(
                        || format!("{:?}", component.category),
                        |value| value.name.clone(),
                    );
                    facts.push(format!("{category} damage: {amount}"));
                }
            }
            for modifier in &known.modifiers {
                facts.extend(tor_client_common::combat_diagnostics::trace_lines(modifier));
            }
        } else {
            facts.push("Combat properties not known.".into());
        }
        if let Some(bonus) = item.known_projectile_bonus {
            facts.push(format!(
                "Projectile hit bonus: {:+}  |  Damage bonus: {:+}",
                bonus.hit, bonus.damage
            ));
        }
        if let Some(cursed) = item.known_cursed {
            facts.push(if cursed { "Cursed" } else { "Not cursed" }.into());
        }
        if let Some(protected) = item.known_protected {
            facts.push(
                if protected {
                    "Protected"
                } else {
                    "Not protected"
                }
                .into(),
            );
        }
        if facts.is_empty() {
            facts.push("No remembered modifiers.".into());
        }
        facts
    }
}
