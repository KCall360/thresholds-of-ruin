//! Admission-owned operand identities. Not a client capability or a cached plan.
//! Requested actions stay unchanged; fresh preparation may fail, never substitute.
use crate::abilities::{FrozenWeaponSelection, Plan};
use crate::{Action, ActorId, EquipmentSlotId, Game, GameError, IntentionId, ItemId};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HumanOperandBinding {
    weapon: Option<WeaponOperand>,
    items: Vec<ItemOperand>,
    preparation: Option<PreparationOperand>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum WeaponOperand {
    Natural,
    Equipped {
        item: ItemId,
        socket: EquipmentSlotId,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
struct ItemOperand {
    cost_ordinal: u8,
    item: ItemId,
    quantity: u64,
}
impl<'de> Deserialize<'de> for ItemOperand {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            cost_ordinal: u8,
            item: ItemId,
            quantity: u64,
        }
        let f: Fields = crate::bounded::named_map(d)?;
        if f.item.0 == 0 || f.item.0 == u64::MAX || f.quantity == 0 {
            return Err(serde::de::Error::custom("invalid human item operand"));
        }
        Ok(Self {
            cost_ordinal: f.cost_ordinal,
            item: f.item,
            quantity: f.quantity,
        })
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
struct PreparationOperand {
    intention: Option<IntentionId>,
    origin: Option<IntentionId>,
}
impl<'de> Deserialize<'de> for PreparationOperand {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            #[serde(deserialize_with = "crate::bounded::required_option")]
            intention: Option<IntentionId>,
            #[serde(deserialize_with = "crate::bounded::required_option")]
            origin: Option<IntentionId>,
        }
        let f: Fields = crate::bounded::named_map(d)?;
        if [f.intention, f.origin]
            .into_iter()
            .flatten()
            .any(|id| id.0 == 0 || id.0 == u64::MAX)
        {
            return Err(serde::de::Error::custom("invalid preparation operand"));
        }
        Ok(Self {
            intention: f.intention,
            origin: f.origin,
        })
    }
}
fn read_items<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<ItemOperand>, D::Error> {
    Ok(crate::bounded::Bounded::<ItemOperand, {crate::supplies::MAX_ITEM_FRAGMENTS}>::deserialize(d)?.0)
}
impl<'de> Deserialize<'de> for HumanOperandBinding {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            #[serde(deserialize_with = "crate::bounded::required_option")]
            weapon: Option<WeaponOperand>,
            #[serde(deserialize_with = "read_items")]
            items: Vec<ItemOperand>,
            #[serde(deserialize_with = "crate::bounded::required_option")]
            preparation: Option<PreparationOperand>,
        }
        let f: Fields = crate::bounded::named_map(d)?;
        let result = Self {
            weapon: f.weapon,
            items: f.items,
            preparation: f.preparation,
        };
        if !result.shape_valid() {
            return Err(serde::de::Error::custom("invalid human operand binding"));
        }
        Ok(result)
    }
}
impl HumanOperandBinding {
    pub(crate) fn empty() -> Self {
        Self {
            weapon: None,
            items: Vec::new(),
            preparation: None,
        }
    }
    fn shape_valid(&self) -> bool {
        self.items.len() <= crate::supplies::MAX_ITEM_FRAGMENTS
            && self
                .items
                .iter()
                .all(|r| r.item.0 > 0 && r.item.0 < u64::MAX && r.quantity > 0)
            && self
                .items
                .windows(2)
                .all(|r| (r[0].cost_ordinal, r[0].item) < (r[1].cost_ordinal, r[1].item))
            && match self.weapon {
                Some(WeaponOperand::Equipped { item, socket }) => {
                    item.0 > 0 && item.0 < u64::MAX && socket.0 < 64
                }
                _ => true,
            }
            && (self.preparation.is_none() || (self.weapon.is_none() && self.items.is_empty()))
    }
    /// Shape only; does not require historical operands still be available.
    pub fn structurally_valid_for(&self, action: &Action) -> bool {
        self.shape_valid()
            && (self.weapon.is_none() || matches!(action, Action::UseAbility { .. }))
            && (self.items.is_empty()
                || matches!(
                    action,
                    Action::UseAbility { .. }
                        | Action::Respec { .. }
                        | Action::Read { .. }
                        | Action::Drink { .. }
                        | Action::Eat { .. }
                        | Action::Zap { .. }
                ))
            && (self.preparation.is_none()
                || matches!(
                    action,
                    Action::UseAbility { .. }
                        | Action::Respec { .. }
                        | Action::Read { .. }
                        | Action::Drink { .. }
                        | Action::Eat { .. }
                        | Action::Zap { .. }
                        | Action::Escape { .. }
                        | Action::Equip { .. }
                        | Action::Unequip { .. }
                        | Action::Ignite { .. }
                        | Action::Extinguish { .. }
                ))
    }
    fn weapon(plan: &Plan) -> Option<WeaponOperand> {
        match &plan.as_ability()?.weapon {
            FrozenWeaponSelection::Unselected => None,
            FrozenWeaponSelection::Natural => Some(WeaponOperand::Natural),
            FrozenWeaponSelection::Equipped { item, socket, .. } => Some(WeaponOperand::Equipped {
                item: *item,
                socket: *socket,
            }),
        }
    }
}
fn unavailable() -> GameError {
    GameError::ItemUnavailable
}
impl Game {
    pub(crate) fn capture_human_operands(
        &self,
        actor: ActorId,
        plan: Option<&Plan>,
    ) -> Result<HumanOperandBinding, GameError> {
        let Some(plan) = plan else {
            return Ok(HumanOperandBinding::empty());
        };
        if let Some(p) = self.preparation(actor).filter(|p| &p.plan == plan) {
            return Ok(HumanOperandBinding {
                weapon: None,
                items: Vec::new(),
                preparation: Some(PreparationOperand {
                    intention: p.intention,
                    origin: p.origin,
                }),
            });
        }
        let items = self
            .physical_item_allocation(plan)?
            .into_iter()
            .map(|(r, quantity)| ItemOperand {
                cost_ordinal: r.cost_ordinal,
                item: r.item,
                quantity,
            })
            .collect();
        Ok(HumanOperandBinding {
            weapon: HumanOperandBinding::weapon(plan),
            items,
            preparation: None,
        })
    }
    pub(crate) fn validate_human_operands(
        &self,
        actor: ActorId,
        plan: Option<&Plan>,
        binding: &HumanOperandBinding,
    ) -> Result<(), GameError> {
        if let Some(expected) = binding.preparation {
            let p = self
                .preparation(actor)
                .filter(|p| Some(&p.plan) == plan)
                .ok_or_else(unavailable)?;
            return if expected
                == (PreparationOperand {
                    intention: p.intention,
                    origin: p.origin,
                }) {
                Ok(())
            } else {
                Err(unavailable())
            };
        }
        if self
            .preparation(actor)
            .is_some_and(|p| Some(&p.plan) == plan)
        {
            return Err(unavailable()); // A fresh request cannot adopt newly substituted work.
        }
        let Some(plan) = plan else {
            return if binding == &HumanOperandBinding::empty() {
                Ok(())
            } else {
                Err(unavailable())
            };
        };
        if HumanOperandBinding::weapon(plan) != binding.weapon {
            return Err(unavailable());
        }
        let actual = self.item_allocation(plan)?;
        if actual.len() != binding.items.len()
            || actual.iter().zip(&binding.items).any(|((r, n), b)| {
                r.cost_ordinal != b.cost_ordinal || r.item != b.item || *n != b.quantity
            })
        {
            return Err(unavailable());
        }
        Ok(())
    }
    pub(crate) fn human_operands_checkpoint_valid(
        &self,
        binding: &HumanOperandBinding,
        action: &Action,
    ) -> bool {
        binding.structurally_valid_for(action)
            && binding
                .items
                .iter()
                .all(|row| row.item.0 < self.next_item_id)
            && match binding.weapon {
                Some(WeaponOperand::Equipped { item, .. }) => item.0 < self.next_item_id,
                _ => true,
            }
            && binding.preparation.is_none_or(|p| {
                [p.intention, p.origin]
                    .into_iter()
                    .flatten()
                    .all(|id| self.intention_issued(id))
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::abilities::AbilityInvocation;
    use crate::content_ids::{AbilityId, SupplyId};
    use crate::supplies::SupplySelection;
    use crate::{IntentionOrigin, ItemSpec};
    use std::num::NonZeroU64;
    use tor_world::{Location, Position, RegionId};

    fn supply_fixture() -> (Game, ActorId, ItemId, ItemId, Action) {
        let mut game = Game::two_room_in_stone(42, crate::test_creatures::content());
        let at = Location {
            region: RegionId(1),
            position: Position { x: 1, y: 1, z: 0 },
        };
        let owner = game.spawn_actor(at, NonZeroU64::new(1).unwrap()).unwrap();
        crate::test_creatures::configure(
            &mut game,
            owner,
            "neutral",
            crate::test_creatures::species(),
        );
        game.spawn_actor(
            Location {
                position: Position { x: 2, y: 1, z: 0 },
                ..at
            },
            NonZeroU64::new(1).unwrap(),
        )
        .unwrap();
        let mut spec = ItemSpec::ordinary("bandage".into());
        spec.supply = Some(SupplyId::new("bandage").unwrap());
        let first = game
            .place_item_stack(10, at, Some(owner), 1, spec.clone())
            .unwrap();
        let second = game.place_item_stack(11, at, Some(owner), 1, spec).unwrap();
        let action = Action::UseAbility {
            id: AbilityId::new("bind_wounds").unwrap(),
            invocation: AbilityInvocation::actor_target(owner),
            supplies: SupplySelection::Carried,
        };
        (game, owner, first, second, action)
    }
    #[test]
    fn lost_carried_operand_fails_without_spending_replacement_stack() {
        let (mut game, owner, first, second, action) = supply_fixture();
        let id = game
            .admit_intention(owner, action.clone(), IntentionOrigin::Human)
            .unwrap();
        let binding = game
            .pending_intention(owner)
            .unwrap()
            .human_operands
            .clone()
            .unwrap();
        assert_eq!(binding.items[0].item, first);
        let mut shared = crate::checkpoint::SharedState::default();
        let snapshot = game.checkpoint(&mut shared);
        let restored = Game::restore_checkpoint(snapshot, &shared, game.content().clone()).unwrap();
        assert_eq!(
            restored, game,
            "admitted operand binding survives exact checkpoint restoration"
        );
        game = restored;
        assert_eq!(
            game.pending_intention(owner).unwrap().work,
            crate::IntentionWork::Action(action)
        );
        game.items.remove(&first).unwrap();
        let before_tick = game.tick();
        let execution = game.execute_next_intention().unwrap();
        assert_eq!(execution.intention.id, id);
        assert_eq!(execution.outcome, Err(GameError::ItemUnavailable));
        assert_eq!(game.items[&second].quantity, 1);
        assert!(game.items[&second].holds.is_empty());
        assert!(game.preparation(owner).is_none());
        assert_eq!(game.tick(), before_tick);
    }
    #[test]
    fn preparation_operand_guard_tracks_issued_identity_not_progress_clock() {
        let (mut game, owner, _, _, action) = supply_fixture();
        game.admit_intention(owner, action, IntentionOrigin::Human)
            .unwrap();
        game.execute_next_intention().unwrap().outcome.unwrap();
        let plan = game.preparation(owner).unwrap().plan.clone();
        let binding = game.capture_human_operands(owner, Some(&plan)).unwrap();
        game.validate_human_operands(owner, Some(&plan), &binding)
            .unwrap();
        // Sequence progress and region hydration legitimately rebase this clock.
        // The operand guard checks identity; ordinary preparation validation
        // remains responsible for current timing and all other legality.
        game.actors
            .get_mut(&owner)
            .unwrap()
            .pending
            .as_mut()
            .unwrap()
            .started += 10;
        game.validate_human_operands(owner, Some(&plan), &binding)
            .unwrap();
        game.actors
            .get_mut(&owner)
            .unwrap()
            .pending
            .as_mut()
            .unwrap()
            .intention = None;
        assert_eq!(
            game.validate_human_operands(owner, Some(&plan), &binding),
            Err(GameError::ItemUnavailable)
        );
    }

    #[test]
    fn human_operand_binding_roundtrip_is_named_bounded_and_strict() {
        let (mut game, owner, _, _, action) = supply_fixture();
        game.admit_intention(owner, action, IntentionOrigin::Human)
            .unwrap();
        let binding = game
            .pending_intention(owner)
            .unwrap()
            .human_operands
            .clone()
            .unwrap();
        let encoded = serde_json::to_value(&binding).unwrap();
        assert_eq!(
            serde_json::from_value::<HumanOperandBinding>(encoded.clone()).unwrap(),
            binding
        );
        let mut unknown = encoded.clone();
        unknown["extra"] = serde_json::json!(true);
        assert!(serde_json::from_value::<HumanOperandBinding>(unknown).is_err());
        let mut duplicate = encoded.clone();
        duplicate["items"]
            .as_array_mut()
            .unwrap()
            .push(encoded["items"][0].clone());
        assert!(serde_json::from_value::<HumanOperandBinding>(duplicate).is_err());
        let mut zero = encoded.clone();
        zero["items"][0]["quantity"] = serde_json::json!(0);
        assert!(serde_json::from_value::<HumanOperandBinding>(zero).is_err());
        let mut tuple = encoded;
        tuple["items"][0] = serde_json::json!([0, 10, 1]);
        assert!(serde_json::from_value::<HumanOperandBinding>(tuple).is_err());
        assert!(serde_json::from_str::<HumanOperandBinding>("[null,[],null]").is_err());
    }
}
