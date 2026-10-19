//! Backend action facts, independent from transport and persistence schemas.
//! Receipt commands retain the original requested facts; queued work belongs to
//! the simulation and is checked against those facts during recovery.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    Escape {
        hold: tor_simulation::HoldId,
    },
    ChooseGrowthSource {
        basis: Box<tor_simulation::player_advancement::AdvancementBasis>,
        source: tor_simulation::earned_growth::GrowthSource,
    },
    ChooseIntellectExpertise {
        basis: Box<tor_simulation::player_advancement::AdvancementBasis>,
        choice: tor_simulation::player_advancement::IntellectExpertiseOperation,
    },
    Purchase {
        basis: Box<tor_simulation::player_advancement::AdvancementBasis>,
        purchase: tor_simulation::player_advancement::PurchaseOperation,
    },
    Respec {
        basis: Box<tor_simulation::player_advancement::AdvancementBasis>,
        replacement: tor_simulation::player_respec::ReplacementOperation,
        supplies: SupplySelection,
    },
    UseAbility {
        id: tor_simulation::content_ids::AbilityId,
        invocation: AbilityInvocation,
        supplies: SupplySelection,
    },
    Equip {
        item: u64,
        slot: u16,
    },
    Unequip {
        item: u64,
    },
    Drink {
        item: u64,
    },
    Eat {
        item: u64,
    },
    Read {
        item: u64,
    },
    Zap {
        item: u64,
        direction: Direction,
    },
    Identify {
        choice: tor_simulation::IntentionId,
        item: Option<u64>,
    },
    SetDoor {
        door: u64,
        open: bool,
    },
    Move {
        direction: Direction,
    },
    Take {
        item: u64,
        quantity: Option<u64>,
    },
    /// Private identity resolved from the apparent-object token domain. The
    /// original quantity is retained exactly for validation and receipt replay.
    TouchApparentObject {
        subject: tor_simulation::ActorId,
        quantity: Option<u64>,
    },
    Drop {
        item: u64,
        quantity: Option<u64>,
    },
    Wait,
}

/// Resolved physical identities only; opaque target tokens never enter a journal.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum SupplySelection {
    #[default]
    Carried,
    Explicit {
        items: Vec<SelectedSupply>,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct SelectedSupply {
    pub cost_ordinal: u8,
    pub item: u64,
}

impl Action {
    pub(crate) fn advancement_basis(
        &self,
    ) -> Option<&tor_simulation::player_advancement::AdvancementBasis> {
        match self {
            Self::ChooseGrowthSource { basis, .. }
            | Self::ChooseIntellectExpertise { basis, .. }
            | Self::Purchase { basis, .. }
            | Self::Respec { basis, .. } => Some(basis),
            _ => None,
        }
    }
    pub(crate) fn is_prepared(&self) -> bool {
        matches!(
            self,
            Self::Respec { .. }
                | Self::Escape { .. }
                | Self::UseAbility { .. }
                | Self::Equip { .. }
                | Self::Unequip { .. }
                | Self::Drink { .. }
                | Self::Eat { .. }
                | Self::Read { .. }
                | Self::Zap { .. }
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    North,
    East,
    South,
    West,
    NorthEast,
    SouthEast,
    SouthWest,
    NorthWest,
    Up,
    Down,
}

/// Backend selected operands are independent from transport and save DTOs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AbilityInvocation {
    pub aim: AbilityAim,
    pub weapon: WeaponSelection,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AbilityAim {
    ActorTarget {
        target: tor_simulation::ActorId,
    },
    ContactTarget {
        target: tor_simulation::ActorId,
    },
    SelfStep {
        direction: Direction,
    },
    DirectionalRay {
        direction: Direction,
    },
    DirectionalProjectile {
        direction: Direction,
        delivery: ProjectileDeliverySelection,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WeaponSelection {
    Default,
    Equipped { item: u64, socket: u16 },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EquippedProjectileWeapon {
    pub item: u64,
    pub socket: u16,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThrownSourceSelection {
    Equipped { item: u64, socket: u16 },
    Carried { item: u64, socket: u16 },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectileDeliverySelection {
    Throw {
        source: ThrownSourceSelection,
    },
    Launch {
        launcher: EquippedProjectileWeapon,
        ammunition: u64,
    },
}
impl AbilityInvocation {
    pub fn actor_target(target: tor_simulation::ActorId) -> Self {
        Self {
            aim: AbilityAim::ActorTarget { target },
            weapon: WeaponSelection::Default,
        }
    }
    pub fn contact_target(target: tor_simulation::ActorId) -> Self {
        Self {
            aim: AbilityAim::ContactTarget { target },
            weapon: WeaponSelection::Default,
        }
    }
    pub fn self_step(direction: Direction) -> Self {
        Self {
            aim: AbilityAim::SelfStep { direction },
            weapon: WeaponSelection::Default,
        }
    }
    pub fn directional_projectile(
        direction: Direction,
        delivery: ProjectileDeliverySelection,
    ) -> Self {
        Self {
            aim: AbilityAim::DirectionalProjectile {
                direction,
                delivery,
            },
            weapon: WeaponSelection::Default,
        }
    }
    pub fn directional_ray(direction: Direction) -> Self {
        Self {
            aim: AbilityAim::DirectionalRay { direction },
            weapon: WeaponSelection::Default,
        }
    }
}
impl From<tor_simulation::ActorId> for AbilityInvocation {
    fn from(target: tor_simulation::ActorId) -> Self {
        Self::actor_target(target)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapt;
    use tor_protocol as wire;
    use tor_simulation as sim;
    use tor_world as world;

    #[test]
    fn equipped_weapon_and_every_aim_round_trip_independent_adapters() {
        let scope = crate::wire_adapter::TargetScope::new(uuid::Uuid::nil(), sim::ActorId(1));
        for aim in [
            AbilityAim::ActorTarget {
                target: sim::ActorId(2),
            },
            AbilityAim::ContactTarget {
                target: sim::ActorId(3),
            },
            AbilityAim::SelfStep {
                direction: Direction::Up,
            },
            AbilityAim::DirectionalRay {
                direction: Direction::SouthWest,
            },
        ] {
            let action = Action::UseAbility {
                id: sim::content_ids::AbilityId::new("fixture_selection").unwrap(),
                invocation: AbilityInvocation {
                    aim,
                    weapon: WeaponSelection::Equipped {
                        item: 9_007_199_254_740_993,
                        socket: 63,
                    },
                },
                supplies: Default::default(),
            };
            let native = adapt::action(&action);
            assert_eq!(adapt::recorded_action(native), Some(action.clone()));
            let encoded = crate::wire_adapter::encode_action(&action, &scope);
            assert!(matches!(&encoded, wire::Action::UseAbility {
                invocation: wire::AbilityInvocation {
                    weapon: wire::WeaponSelection::Equipped { item, socket: 63 }, ..
                }, ..
            } if *item == scope.item(sim::ItemId(9_007_199_254_740_993))));
            let wire::Action::UseAbility { invocation, .. } = encoded else {
                unreachable!()
            };
            match aim {
                AbilityAim::DirectionalProjectile { .. } => {
                    unreachable!("this fixture uses independent equipped-weapon aims")
                }
                AbilityAim::ActorTarget { target } => assert_eq!(
                    invocation.aim,
                    wire::AbilityAim::ActorTarget {
                        target: scope.actor(target)
                    }
                ),
                AbilityAim::ContactTarget { target } => assert_eq!(
                    invocation.aim,
                    wire::AbilityAim::ActorTarget {
                        target: scope.contact(target)
                    }
                ),
                AbilityAim::SelfStep { .. } => assert_eq!(
                    invocation.aim,
                    wire::AbilityAim::SelfStep {
                        direction: wire::Direction::Up
                    }
                ),
                AbilityAim::DirectionalRay { .. } => assert_eq!(
                    invocation.aim,
                    wire::AbilityAim::DirectionalRay {
                        direction: wire::Direction::SouthWest
                    }
                ),
            }
        }
    }

    #[test]
    fn ability_target_resolution_rejects_foreign_and_undisclosed_tokens_equally() {
        let engine = crate::Engine::memory(crate::Scenario::two_room(42)).unwrap();
        let observer = wire::ActorId(1);
        for ability in ["basic_melee", "fixture_ability", "fear"] {
            let target = engine.target_scope(observer).actor(sim::ActorId(1));
            let decoded = engine
                .decode_action(
                    observer,
                    &wire::Action::UseAbility {
                        id: ability.into(),
                        invocation: tor_protocol::AbilityInvocation {
                            aim: tor_protocol::AbilityAim::ActorTarget { target },
                            weapon: tor_protocol::WeaponSelection::Default,
                        },
                        supplies: Default::default(),
                    },
                )
                .unwrap();
            assert_eq!(
                decoded,
                Action::UseAbility {
                    id: sim::content_ids::AbilityId::new(ability).unwrap(),
                    invocation: crate::actions::AbilityInvocation {
                        aim: crate::actions::AbilityAim::ActorTarget {
                            target: sim::ActorId(1)
                        },
                        weapon: crate::actions::WeaponSelection::Default
                    },
                    supplies: Default::default(),
                }
            );
            let hidden = engine.target_scope(observer).actor(sim::ActorId(u64::MAX));
            let foreign = engine.target_scope(wire::ActorId(2)).actor(sim::ActorId(1));
            let hidden_error = engine
                .decode_action(
                    observer,
                    &wire::Action::UseAbility {
                        id: ability.into(),
                        invocation: tor_protocol::AbilityInvocation {
                            aim: tor_protocol::AbilityAim::ActorTarget { target: hidden },
                            weapon: tor_protocol::WeaponSelection::Default,
                        },
                        supplies: Default::default(),
                    },
                )
                .unwrap_err();
            let foreign_error = engine
                .decode_action(
                    observer,
                    &wire::Action::UseAbility {
                        id: ability.into(),
                        invocation: tor_protocol::AbilityInvocation {
                            aim: tor_protocol::AbilityAim::ActorTarget { target: foreign },
                            weapon: tor_protocol::WeaponSelection::Default,
                        },
                        supplies: Default::default(),
                    },
                )
                .unwrap_err();
            assert_eq!(hidden_error, foreign_error);
            assert_eq!(hidden_error.code, wire::ErrorCode::InvalidAction);
        }
    }

    #[test]
    fn phase5_supply_resolution_rejects_foreign_and_undisclosed_tokens_equally() {
        let engine = crate::Engine::memory(crate::Scenario::two_room(42)).unwrap();
        let observer = wire::ActorId(1);
        let scope = engine.target_scope(observer);
        let action = |item| wire::Action::UseAbility {
            id: "bind_wounds".into(),
            invocation: tor_protocol::AbilityInvocation {
                aim: tor_protocol::AbilityAim::ActorTarget {
                    target: scope.actor(sim::ActorId(1)),
                },
                weapon: tor_protocol::WeaponSelection::Default,
            },
            supplies: wire::SupplySelection::Explicit {
                items: vec![wire::SelectedSupply {
                    cost_ordinal: 0,
                    item,
                }],
            },
        };
        let decoded = engine
            .decode_action(observer, &action(scope.item(sim::ItemId(1))))
            .unwrap();
        assert_eq!(
            decoded,
            Action::UseAbility {
                id: sim::content_ids::AbilityId::new("bind_wounds").unwrap(),
                invocation: crate::actions::AbilityInvocation {
                    aim: crate::actions::AbilityAim::ActorTarget {
                        target: sim::ActorId(1)
                    },
                    weapon: crate::actions::WeaponSelection::Default
                },
                supplies: SupplySelection::Explicit {
                    items: vec![SelectedSupply {
                        cost_ordinal: 0,
                        item: 1
                    }]
                },
            }
        );
        let hidden = engine
            .decode_action(observer, &action(scope.item(sim::ItemId(u64::MAX))))
            .unwrap_err();
        let foreign = engine
            .decode_action(
                observer,
                &action(engine.target_scope(wire::ActorId(2)).item(sim::ItemId(1))),
            )
            .unwrap_err();
        assert_eq!(hidden, foreign);
        assert_eq!(hidden.code, wire::ErrorCode::InvalidAction);
    }

    #[test]
    fn phase5_supply_receipts_reconstruct_deleted_sources_and_canonical_opaque_order() {
        let engine = crate::Engine::memory(crate::Scenario::two_room(42)).unwrap();
        let observer = wire::ActorId(1);
        let scope = engine.target_scope(observer);
        let action = Action::UseAbility {
            id: sim::content_ids::AbilityId::new("bind_wounds").unwrap(),
            invocation: crate::actions::AbilityInvocation {
                aim: crate::actions::AbilityAim::ActorTarget {
                    target: sim::ActorId(1),
                },
                weapon: crate::actions::WeaponSelection::Default,
            },
            supplies: SupplySelection::Explicit {
                items: vec![
                    SelectedSupply {
                        cost_ordinal: 0,
                        item: 100,
                    },
                    SelectedSupply {
                        cost_ordinal: 0,
                        item: 101,
                    },
                ],
            },
        };
        let encoded = crate::wire_adapter::encode_action(&action, &scope);
        let wire::Action::UseAbility { ref supplies, .. } = encoded else {
            panic!("wrong action")
        };
        assert!(supplies.valid());
        let command = crate::journal::Command::AdmitIntention {
            expected_revision: 1,
            action: action.clone(),
        };
        let encoded_command = engine.encode_command(observer, command.clone()).unwrap();
        assert!(crate::wire_adapter::decode_command(&encoded_command)
            .unwrap()
            .matches(&command, &scope));
        assert!(
            engine.decode_action(observer, &encoded).is_err(),
            "a receipt is not fresh source authority"
        );
        assert_eq!(adapt::recorded_action(adapt::action(&action)), Some(action));
    }

    #[test]
    fn every_requested_direction_maps_to_its_native_axis_and_wire_name() {
        let engine = crate::Engine::memory(crate::Scenario::two_room(42)).unwrap();
        let actor = wire::ActorId(1);
        for (backend, wire, native) in [
            (
                Direction::North,
                wire::Direction::North,
                world::Direction::North,
            ),
            (
                Direction::East,
                wire::Direction::East,
                world::Direction::East,
            ),
            (
                Direction::South,
                wire::Direction::South,
                world::Direction::South,
            ),
            (
                Direction::West,
                wire::Direction::West,
                world::Direction::West,
            ),
            (
                Direction::NorthEast,
                wire::Direction::NorthEast,
                world::Direction::NorthEast,
            ),
            (
                Direction::SouthEast,
                wire::Direction::SouthEast,
                world::Direction::SouthEast,
            ),
            (
                Direction::SouthWest,
                wire::Direction::SouthWest,
                world::Direction::SouthWest,
            ),
            (
                Direction::NorthWest,
                wire::Direction::NorthWest,
                world::Direction::NorthWest,
            ),
            (Direction::Up, wire::Direction::Up, world::Direction::Up),
            (
                Direction::Down,
                wire::Direction::Down,
                world::Direction::Down,
            ),
        ] {
            let action = Action::Move { direction: backend };
            assert_eq!(adapt::action(&action), sim::Action::Move(native));
            assert_eq!(
                engine.encode_action(actor, &action),
                wire::Action::Move { direction: wire }
            );
            assert_eq!(
                engine
                    .decode_action(actor, &engine.encode_action(actor, &action))
                    .unwrap(),
                action
            );
            assert_eq!(
                adapt::recorded_action(sim::Action::Move(native)),
                Some(action)
            );
            let id = sim::content_ids::AbilityId::new("unowned_content_ray").unwrap();
            let ray = Action::UseAbility {
                id: id.clone(),
                invocation: crate::actions::AbilityInvocation {
                    aim: crate::actions::AbilityAim::DirectionalRay { direction: backend },
                    weapon: crate::actions::WeaponSelection::Default,
                },
                supplies: Default::default(),
            };
            let native_ray = sim::Action::UseAbility {
                id,
                invocation: sim::abilities::AbilityInvocation {
                    aim: sim::abilities::AbilityAim::DirectionalRay {
                        direction: native.try_into().unwrap(),
                    },
                    weapon: sim::abilities::WeaponSelection::Default,
                },
                supplies: Default::default(),
            };
            assert_eq!(adapt::action(&ray), native_ray);
            assert_eq!(adapt::recorded_action(native_ray), Some(ray.clone()));
            let encoded = engine.encode_action(actor, &ray);
            assert!(matches!(encoded, wire::Action::UseAbility {
                invocation: wire::AbilityInvocation { aim: wire::AbilityAim::DirectionalRay { direction }, weapon: wire::WeaponSelection::Default }, ..
            } if direction == wire));
            // Decoding never requires or resolves a creature in the selected ray.
            assert_eq!(engine.decode_action(actor, &encoded).unwrap(), ray.clone());
            let command = crate::journal::Command::AdmitIntention {
                expected_revision: u64::MAX,
                action: ray,
            };
            let encoded = engine.encode_command(actor, command.clone()).unwrap();
            assert!(crate::wire_adapter::decode_command(&encoded)
                .unwrap()
                .matches(&command, &engine.target_scope(actor)));
        }
        for direction in [
            world::Direction::EastUp,
            world::Direction::WestUp,
            world::Direction::NorthUp,
            world::Direction::SouthUp,
            world::Direction::EastDown,
            world::Direction::WestDown,
            world::Direction::NorthDown,
            world::Direction::SouthDown,
        ] {
            assert_eq!(adapt::recorded_action(sim::Action::Move(direction)), None);
        }
    }

    #[test]
    fn every_action_preserves_target_and_quantity_without_resolution() {
        let engine = crate::Engine::memory(crate::Scenario::two_room(42)).unwrap();
        let observer = wire::ActorId(1);
        let mut cases = vec![
            (
                Action::Equip {
                    item: u64::MAX,
                    slot: u16::MAX,
                },
                sim::Action::Equip {
                    item: sim::ItemId(u64::MAX),
                    slot: sim::EquipmentSlotId(u16::MAX),
                },
            ),
            (
                Action::Unequip { item: u64::MAX },
                sim::Action::Unequip {
                    item: sim::ItemId(u64::MAX),
                },
            ),
            (
                Action::Drink { item: u64::MAX },
                sim::Action::Drink {
                    item: sim::ItemId(u64::MAX),
                },
            ),
            (
                Action::UseAbility {
                    id: sim::content_ids::AbilityId::new("basic_melee").unwrap(),
                    invocation: crate::actions::AbilityInvocation {
                        aim: crate::actions::AbilityAim::ActorTarget {
                            target: sim::ActorId(u64::MAX),
                        },
                        weapon: crate::actions::WeaponSelection::Default,
                    },
                    supplies: Default::default(),
                },
                sim::Action::UseAbility {
                    id: sim::content_ids::AbilityId::new("basic_melee").unwrap(),
                    invocation: tor_simulation::abilities::AbilityInvocation {
                        aim: tor_simulation::abilities::AbilityAim::ActorTarget {
                            target: sim::ActorId(u64::MAX),
                        },
                        weapon: tor_simulation::abilities::WeaponSelection::Default,
                    },
                    supplies: Default::default(),
                },
            ),
            (
                Action::SetDoor {
                    door: u64::MAX,
                    open: true,
                },
                sim::Action::SetDoor {
                    door: u64::MAX,
                    open: true,
                },
            ),
            (
                Action::SetDoor {
                    door: u64::MAX,
                    open: false,
                },
                sim::Action::SetDoor {
                    door: u64::MAX,
                    open: false,
                },
            ),
            (Action::Wait, sim::Action::Wait),
        ];
        for quantity in [None, Some(0), Some(1), Some(u64::MAX)] {
            cases.push((
                Action::Take {
                    item: u64::MAX,
                    quantity,
                },
                sim::Action::Take {
                    item: sim::ItemId(u64::MAX),
                    quantity,
                },
            ));
            cases.push((
                Action::Drop {
                    item: u64::MAX,
                    quantity,
                },
                sim::Action::Drop {
                    item: sim::ItemId(u64::MAX),
                    quantity,
                },
            ));
        }
        for ability in ["power_strike", "magic_bolt", "fear", "fixture_ability"] {
            let id = sim::content_ids::AbilityId::new(ability).unwrap();
            cases.push((
                Action::UseAbility {
                    id: id.clone(),
                    invocation: crate::actions::AbilityInvocation {
                        aim: crate::actions::AbilityAim::ActorTarget {
                            target: sim::ActorId(u64::MAX),
                        },
                        weapon: crate::actions::WeaponSelection::Default,
                    },
                    supplies: Default::default(),
                },
                sim::Action::UseAbility {
                    id,
                    invocation: tor_simulation::abilities::AbilityInvocation {
                        aim: tor_simulation::abilities::AbilityAim::ActorTarget {
                            target: sim::ActorId(u64::MAX),
                        },
                        weapon: tor_simulation::abilities::WeaponSelection::Default,
                    },
                    supplies: Default::default(),
                },
            ));
        }
        for (action, native) in cases {
            assert_eq!(adapt::action(&action), native);
            assert_eq!(adapt::recorded_action(native), Some(action.clone()));
            let command = crate::journal::Command::AdmitIntention {
                expected_revision: u64::MAX,
                action,
            };
            let wire = engine.encode_command(observer, command.clone()).unwrap();
            let decoded = crate::wire_adapter::decode_command(&wire).unwrap();
            assert!(decoded.matches(&command, &engine.target_scope(observer)));
        }
    }
}
#[cfg(test)]
mod magic_item_adapter_tests {
    use super::*;
    use crate::adapt;
    use tor_simulation as sim;
    #[test]
    fn magic_item_backend_and_simulation_facts_round_trip_without_ui_or_sources() {
        let cases = [
            Action::Read { item: u64::MAX },
            Action::Zap {
                item: 71,
                direction: Direction::NorthWest,
            },
            Action::Identify {
                choice: sim::IntentionId(17),
                item: Some(71),
            },
            Action::Identify {
                choice: sim::IntentionId(17),
                item: None,
            },
        ];
        for action in cases {
            assert_eq!(
                adapt::recorded_action(adapt::action(&action)).unwrap(),
                action
            );
            assert_eq!(
                adapt::work(&action).is_some(),
                !matches!(action, Action::Identify { .. })
            );
            assert_eq!(
                action.is_prepared(),
                !matches!(action, Action::Identify { .. })
            );
        }
    }
}
