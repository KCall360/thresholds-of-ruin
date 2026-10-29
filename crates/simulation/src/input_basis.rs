//! Minimal owner operand identity, NOT a hidden-state legality fingerprint.
use crate::content_ids::SupplyId;
use crate::creatures::SourceId;
use crate::{ActorId, EquipmentSlotId, Game, HoldId, ItemId, ItemLocation};
use std::collections::BTreeMap;
use tor_world::Location;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InputBasis {
    origin: Location,
    orientation: u8,
    equipment: BTreeMap<EquipmentSlotId, ItemId>,
    default_weapon: DefaultWeaponOperand,
    move_escape: Option<HoldId>,
    inventory: Vec<InputItem>,
}

// This is ONLY the identity selected by a Default weapon request. Do not put
// SelectedAttack's numeric modifiers, provenance or hidden source IDs here.
// It mirrors capture_selected_weapon's natural/equipped operand distinction.
#[derive(Clone, Debug, PartialEq, Eq)]
enum DefaultWeaponOperand {
    Unavailable,
    Natural,
    Equipped {
        item: ItemId,
        socket: Option<EquipmentSlotId>,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct InputItem {
    id: ItemId,
    quantity: u64,
    known_supply: Option<SupplyId>,
}
impl Game {
    /// Capture only for registered input actors, at authoritative boundaries.
    /// Endpoint equality MUST feed a monotonic generation at every successful
    /// commit, not merely be compared when the session eventually publishes.
    /// Current target/condition/resource/capability checks still apply; this
    /// basis does not authorize an action and does not freeze automatic supplies.
    pub fn input_basis(&self, id: ActorId) -> Option<InputBasis> {
        let actor = self.actors.get(&id)?;
        let inventory = self
            .items
            .at(ItemLocation::Carried(id))
            .map(|id| {
                let item = &self.items[&id];
                // Matches observation.rs carried-corpse identification.
                let identified = item.corpse.is_some()
                    || !item.spec.concealed
                    || actor.knowledge.contains(&item.spec.identity);
                let known_supply = identified
                    .then(|| item.spec.supply.as_ref())
                    .flatten()
                    .and_then(|supply| self.content.rules.supply(supply))
                    .map(|supply| supply.id.clone());
                InputItem {
                    id,
                    quantity: item.quantity,
                    known_supply,
                }
            })
            .collect();
        let default_weapon =
            self.creature(id)
                .map_or(DefaultWeaponOperand::Unavailable, |creature| {
                    let selected = &creature.derived().selected_attack;
                    match selected.source {
                        SourceId::Equipment(item) => DefaultWeaponOperand::Equipped {
                            item,
                            socket: selected.socket,
                        },
                        _ => DefaultWeaponOperand::Natural,
                    }
                });
        Some(InputBasis {
            origin: actor.location,
            orientation: actor.orientation,
            equipment: actor.equipment.clone(),
            default_weapon,
            move_escape: actor.holds.links().map(|hold| hold.id).min(),
            inventory,
        })
    }
}
// Deliberately excluded:
// - raw MotionState, sub-cell displacement/remainders, active fall bookkeeping;
// - body eye/mass/anatomy and raw DerivedCreature capabilities/stat modifiers;
// - tick, health/resource/cooldown timers, conditions/source/reservation IDs;
// - raw IntentionInput: server must bind ONLY journal-filtered published
//   IntentionStatus IDs/phases plus existing Readiness permissions, not private
//   autonomous intention IDs. That binding belongs beside DisclosedReadiness.
// - item names/assets/hidden spec/charges/reservations/free_quantity.
// InputBasis must not be serialized or debug-logged into client-facing output.
// Input dependency contracts.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Item, ItemSpec, MotionState};
    use std::num::NonZeroU64;
    use tor_world::{Extent, Position, Region, RegionId, Shared, World};

    const OWNER: ActorId = ActorId(1);
    fn at(region: u64, x: i32) -> Location {
        Location {
            region: RegionId(region),
            position: Position { x, y: 1, z: 0 },
        }
    }
    fn fixture() -> Game {
        let mut world = World::new(vec![], vec![]).unwrap();
        for id in [1, 2] {
            world
                .add_region(Region {
                    id: RegionId(id),
                    name: format!("region-{id}"),
                    bounds: Extent::new(8, 8, 2).unwrap(),
                })
                .unwrap();
        }
        let content =
            crate::creatures::Content::new(crate::test_support::test_ruleset(), Default::default())
                .unwrap();
        let mut game = Game::new(world, 42, content);
        assert_eq!(
            game.spawn_actor(at(1, 2), NonZeroU64::new(100).unwrap())
                .unwrap(),
            OWNER
        );
        game.spawn_actor(at(2, 2), NonZeroU64::new(100).unwrap())
            .unwrap();
        game
    }
    fn item(owner: ActorId) -> Item {
        Item {
            condition: crate::armor::ItemCondition::default(),
            charges: None,
            relocation_elapsed: None,
            fuel: None,
            corpse: None,
            holds: vec![],
            location: ItemLocation::Carried(owner),
            quantity: 1,
            spec: Shared::new(ItemSpec::ordinary("weight".into())),
            motion: MotionState::default(),
            orientation: 0,
        }
    }
    #[test]
    fn cosmetic_remote_and_hidden_motion_changes_preserve_input_basis() {
        let mut game = fixture();
        let before = game.input_basis(OWNER).unwrap();
        game.set_actor_asset(OWNER, Some("cosmetic-new-art".into()))
            .unwrap();
        game.actors.get_mut(&ActorId(2)).unwrap().location = at(2, 3);
        {
            let mut owner = game.actors.get_mut(&OWNER).unwrap();
            owner.motion.displacement = [1, 2, 3];
            owner.motion.acceleration_remainder = [3, 2, 1];
        }
        assert_eq!(game.input_basis(OWNER), Some(before));
    }
    #[test]
    fn origin_and_rotation_change_but_endpoint_equality_does_not_solve_aba() {
        let mut game = fixture();
        let before = game.input_basis(OWNER).unwrap();
        game.actors.get_mut(&OWNER).unwrap().location = at(1, 3);
        assert_ne!(game.input_basis(OWNER), Some(before.clone()));
        game.actors.get_mut(&OWNER).unwrap().location = at(1, 2);
        assert_eq!(
            game.input_basis(OWNER),
            Some(before.clone()),
            "engine must record every committed transition with its own monotonic generation"
        );
        game.actors.get_mut(&OWNER).unwrap().orientation = 1;
        assert_ne!(game.input_basis(OWNER), Some(before));
    }
    #[test]
    fn inventory_identity_quantity_and_equipment_bind_without_cosmetic_or_secret_specs() {
        let mut game = fixture();
        game.items.insert(ItemId(10), item(OWNER));
        game.items
            .edit(ItemId(10), |item| item.spec.concealed = true)
            .unwrap();
        let before = game.input_basis(OWNER).unwrap();
        game.items
            .edit(ItemId(10), |item| {
                item.spec.name = "undisclosed replacement name".into();
                item.spec.identity = "undisclosed replacement identity".into();
                item.spec.asset = Some("other-art".into());
            })
            .unwrap();
        assert_eq!(game.input_basis(OWNER), Some(before.clone()));
        game.items
            .edit(ItemId(10), |item| item.quantity = 2)
            .unwrap();
        assert_ne!(game.input_basis(OWNER), Some(before.clone()));
        game.items
            .edit(ItemId(10), |item| item.quantity = 1)
            .unwrap();
        assert_eq!(game.input_basis(OWNER), Some(before.clone()));
        game.actors
            .get_mut(&OWNER)
            .unwrap()
            .equipment
            .insert(EquipmentSlotId(0), ItemId(10));
        assert_ne!(game.input_basis(OWNER), Some(before.clone()));
        game.actors.get_mut(&OWNER).unwrap().equipment.clear();
        game.items.remove(&ItemId(10));
        game.items.insert(ItemId(11), item(OWNER));
        assert_ne!(game.input_basis(OWNER), Some(before));
    }
    #[test]
    fn capture_is_pure_and_remote_inventory_does_not_participate() {
        let mut game = fixture();
        let original = game.clone();
        let before = game.input_basis(OWNER).unwrap();
        assert_eq!(game, original);
        game.items.insert(ItemId(20), item(ActorId(2)));
        assert_eq!(game.input_basis(OWNER), Some(before));
        assert!(game.input_basis(ActorId(999)).is_none());
    }
}
