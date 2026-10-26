//! Choosing a projectile delivery. The session owns the shared menu so each
//! query is bound to its own reply; the window sees only a prompt.
use super::{publish, transact, transact_with, Error, Event};
use tor_client_3d_model::ProjectileStep;
use tor_client_common::projectiles::{ProjectileMenu, Step};
use tor_client_common::Connection;
use tor_protocol::{Command, ServerMessage};

/// What the window shows of a projectile choice in progress.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProjectilePrompt {
    pub title: String,
    /// The current page's choices, or a note while none can be chosen.
    pub rows: Vec<String>,
    /// Whether `rows` are choices that can be taken.
    pub choosing: bool,
    /// A delivery is chosen: the next direction fires it.
    pub aiming: bool,
    pub next: bool,
    pub previous: bool,
}

fn prompt(menu: &ProjectileMenu) -> ProjectilePrompt {
    ProjectilePrompt {
        title: menu.title(),
        rows: menu.rows(),
        choosing: menu.step() == Step::Choices && !menu.choices().is_empty(),
        aiming: menu.step() == Step::Direction,
        next: menu.has_next(),
        previous: menu.has_previous(),
    }
}

fn abandon(
    menu: &mut Option<ProjectileMenu>,
    reason: String,
    tx: &std::sync::mpsc::SyncSender<Event>,
) -> Result<(), Error> {
    *menu = None;
    publish(tx, Event::Projectile(None))?;
    publish(tx, Event::Status(reason))
}

/// Drop a menu whose observation no longer matches the current state.
pub(super) fn revalidate(
    connection: &Connection,
    menu: &mut Option<ProjectileMenu>,
    tx: &std::sync::mpsc::SyncSender<Event>,
) -> Result<(), Error> {
    if menu.as_ref().is_some_and(|m| !m.current(&connection.state)) {
        return abandon(
            menu,
            "Projectile choices changed; choose the ability again.".into(),
            tx,
        );
    }
    Ok(())
}

pub(super) async fn step(
    connection: &mut Connection,
    menu: &mut Option<ProjectileMenu>,
    step: ProjectileStep,
    tx: &std::sync::mpsc::SyncSender<Event>,
) -> Result<(), Error> {
    let state = &connection.state;
    let command = match step {
        ProjectileStep::Start(id) => ProjectileMenu::new(state, &id).and_then(|m| {
            let command = m.command();
            *menu = Some(m);
            command
        }),
        ProjectileStep::Cancel => {
            *menu = None;
            return publish(tx, Event::Projectile(None));
        }
        ProjectileStep::Fire(direction) => {
            let Some(chosen) = menu.take() else {
                return abandon(menu, "Choose a projectile ability first.".into(), tx);
            };
            publish(tx, Event::Projectile(None))?;
            return match chosen.action(state, direction) {
                Ok(action) => {
                    let request = state.command_request(Command::Act {
                        expected_revision: state.state().revision,
                        action,
                    });
                    transact(connection, request, tx).await
                }
                Err(reason) => publish(tx, Event::Status(reason)),
            };
        }
        ProjectileStep::Choose(index) => match menu.as_mut() {
            None => Err("Choose a projectile ability first.".into()),
            Some(m) => match m.choose(state, index) {
                Ok(None) => return publish(tx, Event::Projectile(Some(prompt(m)))),
                Ok(Some(command)) => Ok(command),
                Err(reason) => Err(reason),
            },
        },
        ProjectileStep::Next => menu
            .as_mut()
            .ok_or_else(|| "Choose a projectile ability first.".to_string())
            .and_then(|m| m.next(state)),
        ProjectileStep::Previous => menu
            .as_mut()
            .ok_or_else(|| "Choose a projectile ability first.".to_string())
            .and_then(|m| m.previous(state)),
    };
    let command = match command {
        Ok(command) => command,
        Err(reason) => return abandon(menu, reason, tx),
    };
    query(connection, menu, command, tx).await
}

/// Ask for a page of choices and accept only the reply to this request.
async fn query(
    connection: &mut Connection,
    menu: &mut Option<ProjectileMenu>,
    command: Command,
    tx: &std::sync::mpsc::SyncSender<Event>,
) -> Result<(), Error> {
    let request = connection.state.command_request(command);
    let mut refused = None;
    let mut bound = false;
    transact_with(
        connection,
        request,
        |event| publish(tx, event),
        |connection, id, message| {
            let Some(m) = menu.as_mut() else {
                return;
            };
            if refused.is_some() {
                return;
            }
            if !bound {
                bound = true;
                if let Err(reason) = m.bind_request(id) {
                    refused = Some(reason);
                    return;
                }
            }
            if let ServerMessage::ProjectileChoices {
                context,
                request_id,
                page,
            } = message
            {
                if request_id == id {
                    if let Err(reason) = m.accept(&connection.state, context, id, page) {
                        refused = Some(reason);
                    }
                }
            }
        },
    )
    .await?;
    if let Some(reason) = refused {
        return abandon(menu, reason, tx);
    }
    match menu.as_ref() {
        Some(m) if m.step() != Step::Waiting => publish(tx, Event::Projectile(Some(prompt(m)))),
        _ => abandon(menu, "The server offered no projectile choices.".into(), tx),
    }
}
