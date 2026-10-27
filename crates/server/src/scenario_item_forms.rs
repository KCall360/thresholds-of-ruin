//! Seeded physical alternatives share one magical identity. Form selection is
//! independent of simulation RNG, instance IDs and lazy region visitation.
use super::{label, require, Archetype, EquipmentSpec, Failure, Manifest};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::borrow::Cow;
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ItemForm {
    pub name: String,
    pub appearance_pool: String,
    pub equipment: EquipmentSpec,
}

pub(super) fn validate(author: &Archetype) -> Result<(), Failure> {
    if author.forms.is_empty() {
        return Ok(());
    }
    require(
        (2..=16).contains(&author.forms.len())
            && !author.stackable
            && author.projectile.is_none()
            && author.creature.is_none()
            && author.anatomy.is_none()
            && author.body.is_none()
            && author.turn_ticks.is_none()
            && author
                .forms
                .iter()
                .all(|form| label(&form.name) && label(&form.appearance_pool))
            && author
                .forms
                .iter()
                .map(|form| &form.name)
                .collect::<BTreeSet<_>>()
                .len()
                == author.forms.len(),
        "Invalid item forms: expected 2..16 distinct named physical alternatives on an item",
    )
}

pub(super) fn selected<'a>(author: &'a Archetype, key: &str, seed: u64) -> Option<&'a ItemForm> {
    if author.forms.is_empty() {
        return None;
    }
    let identity = author.identity.as_deref().unwrap_or(key);
    let mut hash = Sha256::new();
    hash.update(b"tor-item-form-v1\0");
    hash.update(seed.to_le_bytes());
    hash.update((identity.len() as u64).to_le_bytes());
    hash.update(identity.as_bytes());
    let digest = hash.finalize();
    let choice = u64::from_le_bytes(digest[..8].try_into().expect("eight hash bytes"));
    Some(&author.forms[(choice % author.forms.len() as u64) as usize])
}

pub(super) fn selected_pool<'a>(author: &'a Archetype, key: &str, seed: u64) -> Option<&'a str> {
    selected(author, key, seed)
        .map(|form| form.appearance_pool.as_str())
        .or(author.appearance_pool.as_deref())
}

pub(super) fn apply(author: &Archetype, form: &ItemForm) -> Archetype {
    let mut resolved = author.clone();
    resolved.name = Some(form.name.clone());
    resolved.appearance_pool = Some(form.appearance_pool.clone());
    resolved.equipment = Some(form.equipment.clone());
    resolved.forms.clear();
    resolved
}

/// At most sixteen alternatives per source row; aliases are folded by identity
/// by the caller. Pool capacity and affordance validation see every possibility,
/// not only the chosen seed, without constructing an exponential combination set.
pub(super) fn alternatives(manifest: &Manifest) -> Vec<(&str, Cow<'_, Archetype>)> {
    manifest
        .archetypes
        .iter()
        .flat_map(|(key, author)| {
            let ordinary = author
                .forms
                .is_empty()
                .then_some((key.as_str(), Cow::Borrowed(author)));
            ordinary.into_iter().chain(
                author
                    .forms
                    .iter()
                    .map(move |form| (key.as_str(), Cow::Owned(apply(author, form)))),
            )
        })
        .collect()
}

#[cfg(test)]
#[path = "scenario_item_form_tests.rs"]
mod tests;
