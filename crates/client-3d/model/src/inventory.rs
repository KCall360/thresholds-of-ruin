//! Local inventory browsing, independent of actions and server admission.
use crate::{Carried, View};
use tor_protocol::{ItemClass, ItemTarget};

pub const MAX_INVENTORY_SEARCH: usize = 64;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum InventoryFilter {
    #[default]
    All,
    Gear,
    Usable,
}

impl InventoryFilter {
    fn accepts(self, item: &Carried) -> bool {
        match self {
            Self::All => true,
            Self::Gear => {
                item.equipped.is_some()
                    || item.can_equip
                    || matches!(
                        item.class,
                        ItemClass::Weapon | ItemClass::Armor | ItemClass::Ring | ItemClass::Amulet
                    )
            }
            Self::Usable => {
                item.drinkable
                    || item.edible
                    || item.readable
                    || item.device.is_some()
                    || item.portable_light.is_some()
            }
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InventoryBrowser {
    filter: InventoryFilter,
    query: String,
    selected: Option<ItemTarget>,
}

impl InventoryBrowser {
    pub fn filter(&self) -> InventoryFilter {
        self.filter
    }
    pub fn set_filter(&mut self, filter: InventoryFilter) {
        self.filter = filter;
    }
    pub fn query(&self) -> &str {
        &self.query
    }
    pub fn selected(&self) -> Option<ItemTarget> {
        self.selected
    }
    pub fn clear_selection(&mut self) {
        self.selected = None;
    }
    pub fn set_query(&mut self, query: &str) {
        self.query = query
            .chars()
            .filter(|c| !c.is_control())
            .take(MAX_INVENTORY_SEARCH)
            .collect();
    }
    pub fn carried<'a>(&'a self, view: &'a View) -> impl Iterator<Item = &'a Carried> {
        let query = self.query.trim().to_lowercase();
        view.inventory.iter().filter(move |item| {
            self.filter.accepts(item) && item.name.to_lowercase().contains(&query)
        })
    }
    pub fn select(&mut self, view: &View, target: ItemTarget) -> bool {
        if self.contains(view, target) {
            self.selected = Some(target);
            true
        } else {
            false
        }
    }
    pub fn reconcile(&mut self, view: &View) {
        if self
            .selected
            .is_some_and(|target| !self.contains(view, target))
        {
            self.selected = None;
        }
    }
    pub fn shift(&mut self, view: &View, delta: i32) {
        let targets: Vec<_> = self
            .carried(view)
            .map(|item| item.id)
            .chain(
                view.items
                    .iter()
                    .filter(|item| item.reachable)
                    .map(|item| item.id),
            )
            .collect();
        if targets.is_empty() {
            self.selected = None;
            return;
        }
        let next = self
            .selected
            .and_then(|target| targets.iter().position(|id| *id == target))
            .map_or_else(
                || if delta < 0 { targets.len() - 1 } else { 0 },
                |index| {
                    (index as i64 + i64::from(delta)).clamp(0, targets.len() as i64 - 1) as usize
                },
            );
        self.selected = Some(targets[next]);
    }
    fn contains(&self, view: &View, target: ItemTarget) -> bool {
        self.carried(view).any(|item| item.id == target)
            || view
                .items
                .iter()
                .any(|item| item.id == target && item.reachable)
    }
}
