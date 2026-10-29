//! One server-authored preview pinned to the complete pending action.
use crate::ClientState;
use tor_protocol::*;

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct WeaponPreview {
    context: ReplyContext,
    action: Action,
    request_id: Option<String>,
    preview: Option<AbilityWeaponPreview>,
    projectile_binding: Option<crate::projectiles::ProjectileMenu>,
}
impl WeaponPreview {
    pub fn new(state: &ClientState, action: Action) -> Result<Self, String> {
        if !state.has_control() {
            return Err("Weapon preview requires control of your character.".into());
        }
        let Action::UseAbility {
            id,
            invocation,
            supplies,
        } = &action
        else {
            return Err("Choose an ability action.".into());
        };
        crate::abilities::invocation_action(
            &state.state().observation,
            id,
            *invocation,
            supplies.clone(),
        )?;
        let projectile_binding =
            if matches!(invocation.aim, AbilityAim::DirectionalProjectile { .. }) {
                Some(crate::projectiles::ProjectileMenu::new(state, id)?)
            } else {
                None
            };
        Ok(Self {
            projectile_binding,
            context: state.reply_context(),
            action,
            request_id: None,
            preview: None,
        })
    }
    pub fn command(&self) -> Command {
        let Action::UseAbility {
            id,
            invocation,
            supplies,
        } = &self.action
        else {
            unreachable!("validated ability action")
        };
        Command::PreviewAbilityWeapon {
            expected_revision: self.context.revision,
            ability: id.clone(),
            invocation: *invocation,
            supplies: supplies.clone(),
        }
    }
    pub fn bind_request(&mut self, id: &str) -> Result<(), String> {
        if self.request_id.is_some() || id.is_empty() {
            return Err("Weapon preview request is already bound.".into());
        }
        self.request_id = Some(id.into());
        Ok(())
    }
    pub fn current(&self, state: &ClientState) -> bool {
        state.has_control()
            && state.reply_context() == self.context
            && self
                .projectile_binding
                .as_ref()
                .is_none_or(|binding| binding.current(state))
            && match &self.action {
                Action::UseAbility {
                    id,
                    invocation,
                    supplies,
                } => crate::abilities::invocation_action(
                    &state.state().observation,
                    id,
                    *invocation,
                    supplies.clone(),
                )
                .is_ok(),
                _ => false,
            }
    }
    pub fn ready(&self) -> bool {
        self.preview.is_some()
    }
    pub fn matches_action(&self, action: &Action) -> bool {
        self.action == *action
    }
    pub fn accept(
        &mut self,
        state: &ClientState,
        context: &ReplyContext,
        request_id: &str,
        preview: &AbilityWeaponPreview,
    ) -> Result<(), String> {
        if !self.current(state)
            || context != &self.context
            || self.request_id.as_deref() != Some(request_id)
        {
            return Err("Weapon preview is stale or was not requested. Choose again.".into());
        }
        let Action::UseAbility {
            id,
            invocation,
            supplies,
        } = &self.action
        else {
            unreachable!("validated ability action")
        };
        if preview.ability != *id
            || preview.invocation != *invocation
            || preview.supplies != *supplies
        {
            return Err("Weapon preview does not match your exact action. Choose again.".into());
        }
        self.preview = Some(preview.clone());
        Ok(())
    }
    pub fn confirm(self, state: &ClientState) -> Result<Action, String> {
        if !self.current(state) || self.preview.is_none() {
            return Err("Weapon preview changed. Choose again before confirming.".into());
        }
        let Action::UseAbility {
            id,
            invocation,
            supplies,
        } = &self.action
        else {
            unreachable!("validated ability action")
        };
        crate::abilities::invocation_action(
            &state.state().observation,
            id,
            *invocation,
            supplies.clone(),
        )?;
        Ok(self.action)
    }
    pub fn lines(&self) -> Vec<String> {
        self.preview.as_ref().map_or_else(
            || vec!["Waiting for the server's exact weapon terms.".into()],
            lines,
        )
    }
}
pub fn explicit_action(action: &Action) -> bool {
    matches!(
        action,
        Action::UseAbility {
            invocation: AbilityInvocation {
                weapon: WeaponSelection::Equipped { .. },
                ..
            } | AbilityInvocation {
                aim: AbilityAim::DirectionalProjectile { .. },
                ..
            },
            ..
        }
    )
}
pub fn lines(value: &AbilityWeaponPreview) -> Vec<String> {
    let mut rows = vec![match value.phase {
        AbilityWeaponPreviewPhase::Fresh => {
            "Fresh action: start cost is paid when preparation starts; resolution cost is reserved."
                .into()
        }
        AbilityWeaponPreviewPhase::Resuming => {
            "Resuming: start cost is already paid; resolution cost is reserved. Captured costs:"
                .into()
        }
        AbilityWeaponPreviewPhase::PaidPrefix => {
            "Paid sequence: captured costs are already settled; no new resource charge.".into()
        }
    }];
    if let Some(cost) = &value.cost {
        crate::combat_diagnostics::cost_lines(&mut rows, cost);
    } else {
        rows.push("No resource charge.".into());
    }
    rows.push(format!(
        "Time to next strike: {} ticks; recovery: {} ticks.",
        value.preparation, value.recovery
    ));
    rows.push("This preview does not reserve or guarantee resources; the server checks at preparation start.".into());
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    fn state() -> ClientState {
        let mut view = crate::test_support::observation();
        let item = ItemTarget::from_digest([4; 32]);
        view.inventory.push(ItemView {
            corpse: None,
            id: item,
            class: ItemClass::Weapon,
            name: "secondary".into(),
            description: String::new(),
            quantity: 1,
            appearance: "weapon".into(),
            identified: false,
            known_currency: None,
            known_supply: None,
            asset: None,
        });
        let slot = crate::test_support::equipment_slot("weapon", "in hand", true);
        view.interactions = Some(InteractionView {
            identification: None,
            completed: vec![],
            refused: vec![],
            curse_blocked: vec![],
            slots: vec![slot.clone(); 11],
            preparation: None,
            inventory: vec![ItemInteractionView {
                item,
                slot: Some(slot),
                equipped_slot: Some(10),
                known_projectile_bonus: None,
                known_equipment: None,
                known_cursed: None,
                known_protected: None,
                drinkable: false,
                edible: false,
                readable: false,
                zappable: false,
                portable_light: None,
                device_activation: None,
                charges: None,
            }],
        });
        let snapshot=serde_json::from_value(serde_json::json!({"readiness":{"revision":"0","admission":true,"resume":[],"cancel":[]},"context":{"stream":"fixture","epoch":"0"},"actor":"1","branch":"main","cursor":{"sequence":"0","tick":"0"},"has_control":true,"intentions":[],"history":{"entries":[],"older_before":null},"state":{"snapshot":"weapon-preview-picture","wizard_game":false,"revision":"1","observation":view}})).unwrap();
        ClientState::from_snapshot(snapshot).unwrap()
    }
    fn action(state: &ClientState) -> Action {
        Action::UseAbility {
            id: "basic_melee".into(),
            invocation: AbilityInvocation {
                aim: AbilityAim::ActorTarget {
                    target: state.state().observation.visible_actors[0].id,
                },
                weapon: WeaponSelection::Equipped {
                    item: ItemTarget::from_digest([4; 32]),
                    socket: 10,
                },
            },
            supplies: SupplySelection::Carried,
        }
    }
    fn response(action: &Action) -> AbilityWeaponPreview {
        let Action::UseAbility {
            id,
            invocation,
            supplies,
        } = action
        else {
            unreachable!()
        };
        AbilityWeaponPreview {
            ability: id.clone(),
            invocation: *invocation,
            supplies: supplies.clone(),
            phase: AbilityWeaponPreviewPhase::Fresh,
            cost: Some(DiagnosticCostBundle {
                stamina: None,
                focus: Some(DiagnosticCost {
                    resource: crate::test_support::content("focus"),
                    start: 0,
                    resolution: 2,
                }),
                mana: Some(DiagnosticCost {
                    resource: crate::test_support::content("mana"),
                    start: 1,
                    resolution: 2,
                }),
            }),
            preparation: 300,
            recovery: 100,
        }
    }
    #[test]
    fn exact_action_confirmation_rejects_mismatched_echo_context_or_missing_reply() {
        let state = state();
        let action = action(&state);
        let response = response(&action);
        let context = state.reply_context();
        let mut pending = WeaponPreview::new(&state, action.clone()).unwrap();
        assert!(pending.clone().confirm(&state).is_err());
        pending.bind_request("preview").unwrap();
        assert!(pending.bind_request("again").is_err());
        assert!(pending
            .accept(&state, &context, "other", &response)
            .is_err());
        let mut stale = context.clone();
        stale.input.readiness_revision += 1;
        assert!(pending
            .accept(&state, &stale, "preview", &response)
            .is_err());
        for field in 0..4 {
            let mut wrong = response.clone();
            match field {
                0 => wrong.ability = "another".into(),
                1 => wrong.invocation.weapon = WeaponSelection::Default,
                2 => {
                    wrong.invocation.aim = AbilityAim::ActorTarget {
                        target: state.state().observation.self_target,
                    }
                }
                _ => {
                    wrong.supplies = SupplySelection::Explicit {
                        items: vec![SelectedSupply {
                            cost_ordinal: 0,
                            item: ItemTarget::from_digest([5; 32]),
                        }],
                    }
                }
            };
            assert!(pending.accept(&state, &context, "preview", &wrong).is_err());
            assert!(!pending.ready());
        }
        pending
            .accept(&state, &context, "preview", &response)
            .unwrap();
        assert!(pending.ready());
        assert_eq!(pending.confirm(&state).unwrap(), action);
    }
    #[test]
    fn confirmed_choice_is_invalidated_by_control_revision_and_exact_equipment_changes() {
        let state = state();
        let action = action(&state);
        let response = response(&action);
        let mut pending = WeaponPreview::new(&state, action).unwrap();
        pending.bind_request("preview").unwrap();
        pending
            .accept(&state, &state.reply_context(), "preview", &response)
            .unwrap();
        for change in 0..5 {
            let mut snapshot = state.reset_snapshot().unwrap();
            match change {
                0 => {
                    snapshot.has_control = false;
                    snapshot.readiness.admission = false;
                }
                1 => std::sync::Arc::make_mut(&mut snapshot.state).revision += 1,
                2 => snapshot.context.epoch += 1,
                3 => {
                    let observation =
                        &mut std::sync::Arc::make_mut(&mut snapshot.state).observation;
                    observation.inventory.clear();
                    observation.interactions.as_mut().unwrap().inventory.clear();
                }
                _ => {
                    std::sync::Arc::make_mut(&mut snapshot.state)
                        .observation
                        .interactions
                        .as_mut()
                        .unwrap()
                        .inventory[0]
                        .equipped_slot = Some(0)
                }
            }
            let changed = ClientState::from_snapshot(snapshot)
                .unwrap_or_else(|error| panic!("change {change}: {error:?}"));
            assert!(
                pending.clone().confirm(&changed).is_err(),
                "change {change}"
            );
        }
    }
    #[test]
    fn phase_text_distinguishes_captured_paid_costs_and_next_strike_time() {
        let state = state();
        let mut preview = response(&action(&state));
        for (phase, label) in [
            (AbilityWeaponPreviewPhase::Fresh, "Fresh action"),
            (AbilityWeaponPreviewPhase::Resuming, "already paid"),
            (
                AbilityWeaponPreviewPhase::PaidPrefix,
                "no new resource charge",
            ),
        ] {
            preview.phase = phase;
            let text = lines(&preview).join("\n");
            assert!(text.contains(label));
            assert!(text.contains("Time to next strike: 300 ticks; recovery: 100 ticks"));
            assert!(text.contains("Focus"));
            assert!(text.contains("Mana"));
            assert!(text.contains("does not reserve or guarantee resources"));
        }
    }
}
