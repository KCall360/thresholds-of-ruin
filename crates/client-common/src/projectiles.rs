//! Server-authored projectile choices, pinned to one complete owner observation.
//! This module never derives ammunition compatibility, attack terms or physics.
use crate::ClientState;
use tor_protocol::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub enum Step {
    Waiting,
    Choices,
    Direction,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct ProjectileMenu {
    context: ReplyContext,
    ability: AbilityView,
    inventory: Vec<ItemView>,
    interactions: Option<InteractionView>,
    query: Option<ProjectileChoiceQuery>,
    request_id: Option<String>,
    page: Option<ProjectileChoicePage>,
    previous: Vec<u32>,
    delivery: Option<ProjectileDeliverySelection>,
}
impl ProjectileMenu {
    pub fn new(state: &ClientState, id: &str) -> Result<Self, String> {
        let view = &state.state().observation;
        let ability = crate::abilities::available(view)
            .iter()
            .find(|a| a.id == id && a.target_kind == AbilityTargetKind::DirectionalProjectile)
            .ok_or("Choose a currently disclosed projectile ability.")?;
        if !state.has_control() {
            return Err("Projectile selection requires control of your character.".into());
        }
        Ok(Self {
            context: state.reply_context(),
            ability: ability.clone(),
            inventory: view.inventory.clone(),
            interactions: view.interactions.clone(),
            query: Some(ProjectileChoiceQuery {
                ability: id.into(),
                phase: ProjectileChoicePhase::Sources {},
                offset: 0,
            }),
            request_id: None,
            page: None,
            previous: vec![],
            delivery: None,
        })
    }
    pub fn current(&self, state: &ClientState) -> bool {
        let view = &state.state().observation;
        state.has_control()
            && state.reply_context() == self.context
            && crate::abilities::available(view).contains(&self.ability)
            && view.inventory == self.inventory
            && view.interactions == self.interactions
    }
    fn require_current(&self, state: &ClientState) -> Result<(), String> {
        if self.current(state) {
            Ok(())
        } else {
            Err("Projectile choices changed. Choose the ability again.".into())
        }
    }
    pub fn step(&self) -> Step {
        if self.delivery.is_some() {
            Step::Direction
        } else if self.page.is_some() {
            Step::Choices
        } else {
            Step::Waiting
        }
    }
    pub fn title(&self) -> String {
        let kind = match self.step() {
            Step::Direction => "direction",
            _ => match self.query.as_ref().map(|q| q.phase) {
                Some(ProjectileChoicePhase::Sources {}) => match self.choices().first() {
                    Some(ProjectileChoice::Throw { .. }) => "throw source",
                    Some(ProjectileChoice::Launcher { .. }) => "equipped launcher",
                    _ => "projectile source",
                },
                Some(ProjectileChoicePhase::Ammunition { .. }) => "compatible ammunition",
                None => "projectile source",
            },
        };
        format!("{}: choose {kind}", clean(&self.ability.name))
    }
    pub fn choices(&self) -> &[ProjectileChoice] {
        self.page.as_ref().map_or(&[], |p| p.choices.as_slice())
    }
    pub fn rows(&self) -> Vec<String> {
        match self.step() {
            Step::Waiting => vec!["Waiting for the server's choices.".into()],
            Step::Direction => vec!["Choose a direction.".into()],
            Step::Choices => {
                let mut rows = self
                    .choices()
                    .iter()
                    .map(|choice| {
                        let name = self
                            .inventory
                            .iter()
                            .find(|item| item.id == choice.item())
                            .map(|i| clean(&i.name))
                            .unwrap_or_else(|| "Unavailable item".into());
                        match choice {
                            ProjectileChoice::Throw {
                                source: ThrownSourceSelection::Equipped { socket, .. },
                            }
                            | ProjectileChoice::Launcher {
                                launcher: EquippedProjectileWeapon { socket, .. },
                            } => format!("{name} (equipped, socket {socket})"),
                            ProjectileChoice::Throw {
                                source: ThrownSourceSelection::Carried { socket, .. },
                            } => format!("{name} (carried, socket {socket})"),
                            ProjectileChoice::Ammunition { .. } => name,
                        }
                    })
                    .collect::<Vec<_>>();
                if rows.is_empty() {
                    rows.push("No choices on this page.".into());
                }
                rows
            }
        }
    }
    pub fn has_next(&self) -> bool {
        self.page.as_ref().is_some_and(|p| p.next_offset.is_some())
    }
    pub fn has_previous(&self) -> bool {
        !self.previous.is_empty()
    }
    pub fn query(&self) -> Option<&ProjectileChoiceQuery> {
        self.query.as_ref()
    }
    pub fn command(&self) -> Result<Command, String> {
        Ok(Command::ProjectileChoices {
            expected_revision: self.context.revision,
            query: self
                .query
                .clone()
                .ok_or("Choose a projectile ability first.")?,
        })
    }
    fn begin(&mut self, phase: ProjectileChoicePhase, offset: u32) -> Result<Command, String> {
        self.query = Some(ProjectileChoiceQuery {
            ability: self.ability.id.clone(),
            phase,
            offset,
        });
        self.request_id = None;
        self.page = None;
        self.delivery = None;
        self.command()
    }
    pub fn bind_request(&mut self, id: &str) -> Result<(), String> {
        if self.step() != Step::Waiting || self.request_id.is_some() || id.is_empty() {
            return Err("Projectile request was cancelled or is already bound.".into());
        }
        self.request_id = Some(id.into());
        Ok(())
    }
    pub fn accept(
        &mut self,
        state: &ClientState,
        context: &ReplyContext,
        id: &str,
        page: &ProjectileChoicePage,
    ) -> Result<(), String> {
        self.require_current(state)?;
        if self.step() != Step::Waiting
            || context != &self.context
            || self.request_id.as_deref() != Some(id)
            || !self.query.as_ref().is_some_and(|q| page.valid_for(q))
        {
            return Err(
                "Projectile reply does not match the requested choices. Choose again.".into(),
            );
        }
        for choice in &page.choices {
            validate_choice(&state.state().observation, *choice)?;
        }
        if let ProjectileChoicePhase::Ammunition { launcher } = page.query.phase {
            validate_choice(
                &state.state().observation,
                ProjectileChoice::Launcher { launcher },
            )?;
        }
        self.page = Some(page.clone());
        Ok(())
    }
    pub fn next(&mut self, state: &ClientState) -> Result<Command, String> {
        self.require_current(state)?;
        let page = self
            .page
            .as_ref()
            .ok_or("Wait for the projectile choices.")?;
        let offset = page.next_offset.ok_or("There are no later choices.")?;
        let query = page.query.clone();
        self.previous.push(query.offset);
        self.begin(query.phase, offset)
    }
    pub fn previous(&mut self, state: &ClientState) -> Result<Command, String> {
        self.require_current(state)?;
        let phase = self
            .page
            .as_ref()
            .ok_or("Wait for the projectile choices.")?
            .query
            .phase;
        let offset = self.previous.pop().ok_or("There are no earlier choices.")?;
        self.begin(phase, offset)
    }
    pub fn choose(&mut self, state: &ClientState, index: usize) -> Result<Option<Command>, String> {
        self.require_current(state)?;
        let page = self
            .page
            .as_ref()
            .ok_or("Wait for the projectile choices.")?;
        let choice = *page
            .choices
            .get(index)
            .ok_or("Choose an item from this page.")?;
        validate_choice(&state.state().observation, choice)?;
        match (page.query.phase, choice) {
            (ProjectileChoicePhase::Sources {}, ProjectileChoice::Throw { source }) => {
                self.delivery = Some(ProjectileDeliverySelection::Throw { source })
            }
            (ProjectileChoicePhase::Sources {}, ProjectileChoice::Launcher { launcher }) => {
                self.previous.clear();
                return self
                    .begin(ProjectileChoicePhase::Ammunition { launcher }, 0)
                    .map(Some);
            }
            (
                ProjectileChoicePhase::Ammunition { launcher },
                ProjectileChoice::Ammunition { item },
            ) => {
                self.delivery = Some(ProjectileDeliverySelection::Launch {
                    launcher,
                    ammunition: item,
                })
            }
            _ => return Err("Projectile choice has the wrong delivery phase.".into()),
        }
        self.page = None;
        self.request_id = None;
        Ok(None)
    }
    pub fn action(&self, state: &ClientState, direction: Direction) -> Result<Action, String> {
        self.require_current(state)?;
        let delivery = self.delivery.ok_or("Choose the projectile source first.")?;
        crate::abilities::invocation_action(
            &state.state().observation,
            &self.ability.id,
            AbilityInvocation::directional_projectile(direction, delivery),
            SupplySelection::Carried,
        )
    }
}
fn clean(name: &str) -> String {
    name.chars().filter(|c| !c.is_control()).collect()
}
/// Revalidate only disclosed identities and equipment locations. The server owns
/// knowledge eligibility, source preference, compatibility and every numeric rule.
pub fn validate_choice(view: &Observation, choice: ProjectileChoice) -> Result<(), String> {
    if !view.inventory.iter().any(|item| item.id == choice.item()) {
        return Err("The selected projectile item is no longer carried.".into());
    }
    let interactions = view
        .interactions
        .as_ref()
        .ok_or("Current equipment disclosure is unavailable.")?;
    let equipped = interactions
        .inventory
        .iter()
        .find(|row| row.item == choice.item())
        .and_then(|row| row.equipped_slot);
    let (socket, expected) = match choice {
        ProjectileChoice::Throw {
            source: ThrownSourceSelection::Equipped { socket, .. },
        }
        | ProjectileChoice::Launcher {
            launcher: EquippedProjectileWeapon { socket, .. },
        } => (Some(socket), Some(socket)),
        ProjectileChoice::Throw {
            source: ThrownSourceSelection::Carried { socket, .. },
        } => (Some(socket), None),
        ProjectileChoice::Ammunition { .. } => (None, None),
    };
    if equipped != expected
        || socket.is_some_and(|s| interactions.slots.get(usize::from(s)).is_none())
    {
        return Err("The projectile source or socket changed. Choose again.".into());
    }
    Ok(())
}
pub fn validate_delivery(
    view: &Observation,
    delivery: ProjectileDeliverySelection,
) -> Result<(), String> {
    match delivery {
        ProjectileDeliverySelection::Throw { source } => {
            validate_choice(view, ProjectileChoice::Throw { source })
        }
        ProjectileDeliverySelection::Launch {
            launcher,
            ammunition,
        } => {
            if launcher.item == ammunition {
                return Err("The launcher cannot be its own ammunition.".into());
            }
            validate_choice(view, ProjectileChoice::Launcher { launcher })?;
            validate_choice(view, ProjectileChoice::Ammunition { item: ammunition })
        }
    }
}

#[cfg(test)]
#[path = "projectile_tests.rs"]
mod tests;
