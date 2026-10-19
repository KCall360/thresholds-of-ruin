//! Text controls shared by adventure and script mode. Choice numbers only refer
//! to the current server page; opaque sources are never reconstructed from names.
use tor_client_common::{
    projectiles::{ProjectileMenu, Step},
    ClientState,
};
use tor_protocol::*;

pub enum Progress {
    Query,
    Direction,
    Action(Action),
    Cancel,
}
pub fn input(
    menu: &mut ProjectileMenu,
    state: &ClientState,
    line: &str,
) -> Result<Option<Progress>, String> {
    if !menu.current(state) {
        return Err("Projectile choices changed. Choose the ability again.".into());
    }
    let line = line.trim().to_ascii_lowercase();
    if matches!(line.as_str(), "cancel" | "no") {
        return Ok(Some(Progress::Cancel));
    }
    if menu.step() == Step::Direction {
        return crate::parser::lexicon::parse_direction(&line)
            .map(|d| menu.action(state, d).map(|a| Some(Progress::Action(a))))
            .unwrap_or(Ok(None));
    }
    match line.as_str() {
        "next" => return menu.next(state).map(|_| Some(Progress::Query)),
        "previous" | "prev" => return menu.previous(state).map(|_| Some(Progress::Query)),
        _ => {}
    }
    if let Ok(number) = line.parse::<usize>() {
        let index = number
            .checked_sub(1)
            .ok_or("Choose a numbered item from this page.")?;
        return menu.choose(state, index).map(|query| {
            Some(if query.is_some() {
                Progress::Query
            } else {
                Progress::Direction
            })
        });
    }
    Ok(None)
}
pub fn lines(menu: &ProjectileMenu) -> Vec<String> {
    let mut lines = vec![menu.title()];
    if menu.step() == Step::Direction {
        lines.push("Which direction? Use the usual direction names, or cancel.".into());
        return lines;
    }
    lines.extend(menu.rows().into_iter().enumerate().map(|(i, row)| {
        if menu.choices().is_empty() {
            row
        } else {
            format!("{}. {row}", i + 1)
        }
    }));
    let mut controls = if menu.choices().is_empty() {
        vec!["cancel"]
    } else {
        vec!["Enter a number", "cancel"]
    };
    if menu.has_next() {
        controls.push("next");
    }
    if menu.has_previous() {
        controls.push("previous");
    }
    lines.push(controls.join("; "));
    lines
}
