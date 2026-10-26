//! A client's copy of its asset palette, and the lookup that turns a named
//! asset into the client's own look. See docs/reference/protocol.md#asset-palettes.
//!
//! The palette says which assets the server expects the client to need. A
//! client draws a thing with its asset only while the palette is current and
//! holds it; otherwise it falls back to its own look. A missed revision, or an
//! asset the palette lacks, means asking for the whole palette again.
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use tor_protocol::{Observation, PaletteBody, PaletteUpdate};

/// The palette as this connection last heard it. Nothing is acknowledged, so
/// the only repair is a `palette` request, which the caller sends whenever
/// [`Palette::apply`] or [`Palette::notice`] returns true.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Palette {
    /// The revision of the palette held; none before the first full palette.
    revision: Option<u64>,
    assets: BTreeSet<String>,
    /// A revision was missed: nothing resolves until a full palette arrives.
    stale: bool,
    /// A palette request is outstanding; a full palette answers it.
    #[serde(skip)]
    requested: bool,
    /// Assets seen missing from the palette. Each causes at most one
    /// request, so an asset the server never forecasts can't cause a loop.
    #[serde(skip)]
    asked: BTreeSet<String>,
}

impl Palette {
    pub fn revision(&self) -> Option<u64> {
        self.revision
    }

    /// The assets held, whether or not they're current.
    pub fn assets(&self) -> impl Iterator<Item = &str> {
        self.assets.iter().map(String::as_str)
    }

    /// True after a missed revision, until a full palette replaces it.
    pub fn stale(&self) -> bool {
        self.stale
    }

    /// Apply a palette message. True means ask for the whole palette: a
    /// delta didn't follow the revision held. A full palette always replaces
    /// what's held unless it's older.
    pub fn apply(&mut self, update: &PaletteUpdate) -> bool {
        match &update.body {
            PaletteBody::Full { assets } => {
                if self.revision.is_some_and(|held| update.revision <= held) {
                    return false;
                }
                self.revision = Some(update.revision);
                self.assets = assets.clone();
                self.stale = false;
                self.requested = false;
                false
            }
            PaletteBody::Delta {
                base,
                added,
                removed,
            } => {
                let follows = !self.stale
                    && self.revision == Some(*base)
                    && base.checked_add(1) == Some(update.revision);
                if !follows {
                    self.stale = true;
                    return self.ask();
                }
                for asset in removed {
                    self.assets.remove(asset);
                }
                self.assets.extend(added.iter().cloned());
                self.revision = Some(update.revision);
                false
            }
        }
    }

    /// Check the assets an observation names against the palette. True means
    /// ask for the whole palette: one is missing that hasn't been asked
    /// about. Before the first palette, nothing is asked: attaching sends one
    /// unasked, and a scenario without assets names none.
    pub fn notice<'a>(&mut self, assets: impl IntoIterator<Item = &'a str>) -> bool {
        if self.revision.is_none() {
            return false;
        }
        let mut missing = false;
        for asset in assets {
            if !self.assets.contains(asset) && !self.asked.contains(asset) {
                self.asked.insert(asset.to_owned());
                missing = true;
            }
        }
        missing && self.ask()
    }

    fn ask(&mut self) -> bool {
        !std::mem::replace(&mut self.requested, true)
    }

    /// True when a thing with this asset may be drawn with it.
    pub fn holds(&self, asset: &str) -> bool {
        !self.stale && self.assets.contains(asset)
    }

    /// The client's look for a named asset, or `None` for its own default
    /// look: when nothing is named, or the palette doesn't currently hold it.
    pub fn resolve<'t, V>(&self, table: &'t AssetTable<V>, asset: Option<&str>) -> Option<&'t V> {
        asset
            .filter(|asset| self.holds(asset))
            .and_then(|asset| table.get(asset))
    }
}

/// A client's built-in looks, by asset identifier. A lookup falls back
/// through the dotted prefixes: `terrain.floor.cave`, then `terrain.floor`,
/// then `terrain`, then nothing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssetTable<V> {
    entries: BTreeMap<&'static str, V>,
}

impl<V> AssetTable<V> {
    pub fn new(entries: impl IntoIterator<Item = (&'static str, V)>) -> Self {
        Self {
            entries: entries.into_iter().collect(),
        }
    }

    /// The entry for the asset or its longest dotted prefix.
    pub fn get(&self, asset: &str) -> Option<&V> {
        let mut key = asset;
        loop {
            if let Some(value) = self.entries.get(key) {
                return Some(value);
            }
            key = &key[..key.rfind('.')?];
        }
    }
}

/// Every disclosed asset, including own-body and corpse-body appearances.
/// Item presentation and the body represented by its remains are distinct assets.
pub fn observation_assets(observation: &Observation) -> impl Iterator<Item = &str> {
    let cells = observation.visible_cells.iter().flat_map(|cell| {
        [
            cell.asset.as_deref(),
            cell.object_presentation
                .as_ref()
                .and_then(|p| p.asset.as_deref()),
            cell.door.as_ref().and_then(|door| door.asset.as_deref()),
        ]
    });
    // A presented ground object shows only its presentation; otherwise an
    // item's own asset and, for remains, the body it represents.
    let body = |item: &'_ tor_protocol::ItemView| {
        item.corpse.as_ref().and_then(|body| body.asset.as_deref())
    };
    let ground = observation
        .ground_items
        .iter()
        .flat_map(move |ground| match &ground.presentation {
            Some(presented) => [presented.asset.as_deref(), None],
            None => [ground.item.asset.as_deref(), body(&ground.item)],
        });
    let carried = observation
        .inventory
        .iter()
        .flat_map(move |item| [item.asset.as_deref(), body(item)]);
    let items = ground.chain(carried);
    let actors = observation
        .visible_actors
        .iter()
        .map(|actor| actor.asset.as_deref());
    let own = observation
        .self_appearance
        .as_ref()
        .and_then(|body| body.asset.as_deref());
    cells.chain(items).chain(actors).chain([own]).flatten()
}

/// Frozen map recipes participate in palette repair without acquiring actions.
pub(crate) fn map_assets(cells: &[tor_protocol::RememberedMapCell]) -> impl Iterator<Item = &str> {
    cells.iter().flat_map(|cell| {
        [
            cell.asset.as_deref(),
            cell.door.as_ref().and_then(|d| d.asset.as_deref()),
            cell.object_presentation
                .as_ref()
                .and_then(|p| p.asset.as_deref()),
        ]
        .into_iter()
        .chain(cell.ground_items.iter().map(|item| {
            item.presentation
                .as_ref()
                .map_or(&item.asset, |p| &p.asset)
                .as_deref()
        }))
        .flatten()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn observation_assets_include_self_and_both_corpse_carriers() {
        use tor_protocol::{
            AppearanceSeed, CreatureAppearance, GroundItemView, ItemClass, ItemTarget, ItemView,
        };
        let appearance = |asset: &str| CreatureAppearance {
            seed: AppearanceSeed::from_digest([7; 32]),
            worn: Vec::new(),
            asset: Some(asset.into()),
        };
        let corpse = |id, asset| ItemView {
            corpse: Some(appearance(asset)),
            class: ItemClass::Corpse,
            quantity: 1,
            appearance: "remains".into(),
            identified: true,
            description: String::new(),
            id: ItemTarget::from_digest([id; 32]),
            name: "remains".into(),
            asset: Some("item.corpse".into()),
            known_supply: None,
            known_currency: None,
        };
        let mut observation = crate::test_support::observation();
        observation.self_appearance = Some(appearance("creature.self-only"));
        observation.visible_actors[0].asset = Some("creature.visible".into());
        observation.ground_items.push(GroundItemView {
            reachable: true,
            supply_reachable: false,
            item: corpse(1, "creature.ground-remains"),
            position: observation.position,
        });
        observation
            .inventory
            .push(corpse(2, "creature.carried-remains"));
        assert_eq!(
            observation_assets(&observation).collect::<BTreeSet<_>>(),
            BTreeSet::from([
                "creature.self-only",
                "creature.visible",
                "item.corpse",
                "creature.ground-remains",
                "creature.carried-remains"
            ])
        );
        observation.self_appearance.as_mut().unwrap().asset = None;
        observation.ground_items[0]
            .item
            .corpse
            .as_mut()
            .unwrap()
            .asset = None;
        observation.inventory[0].corpse = None;
        assert_eq!(
            observation_assets(&observation).collect::<BTreeSet<_>>(),
            BTreeSet::from(["creature.visible", "item.corpse"]),
            "absent appearance must not invent an asset"
        );
    }

    fn full(revision: u64, assets: &[&str]) -> PaletteUpdate {
        PaletteUpdate {
            revision,
            body: PaletteBody::Full {
                assets: assets.iter().map(|a| a.to_string()).collect(),
            },
        }
    }

    fn delta(revision: u64, base: u64, added: &[&str], removed: &[&str]) -> PaletteUpdate {
        PaletteUpdate {
            revision,
            body: PaletteBody::Delta {
                base,
                added: added.iter().map(|a| a.to_string()).collect(),
                removed: removed.iter().map(|a| a.to_string()).collect(),
            },
        }
    }

    fn held(palette: &Palette) -> Vec<&str> {
        palette.assets().collect()
    }

    #[test]
    fn a_full_palette_then_following_deltas_are_applied() {
        let mut palette = Palette::default();
        assert!(!palette.apply(&full(1, &["creature.rat", "terrain.floor.stone"])));
        assert!(!palette.apply(&delta(2, 1, &["item.coin"], &["creature.rat"])));
        assert_eq!(palette.revision(), Some(2));
        assert_eq!(held(&palette), ["item.coin", "terrain.floor.stone"]);
        assert!(!palette.stale());
    }

    #[test]
    fn a_missed_revision_asks_once_and_stops_resolving_until_a_full_palette() {
        let mut palette = Palette::default();
        palette.apply(&full(1, &["creature.rat"]));
        // Revision 2 never arrived.
        assert!(palette.apply(&delta(3, 2, &["item.coin"], &[])));
        assert!(palette.stale());
        assert!(!palette.holds("creature.rat"));
        // Already asked: later deltas and missing assets wait for the answer.
        assert!(!palette.apply(&delta(4, 3, &[], &[])));
        assert!(!palette.notice(["terrain.floor.cave"]));
        assert!(!palette.apply(&full(5, &["creature.rat", "item.coin"])));
        assert!(!palette.stale());
        assert!(palette.holds("item.coin"));
        // A gap after the answer asks again.
        assert!(palette.apply(&delta(7, 6, &[], &[])));
    }

    #[test]
    fn a_delta_before_any_full_palette_or_with_a_wrong_revision_is_a_gap() {
        let mut palette = Palette::default();
        assert!(palette.apply(&delta(2, 1, &["creature.rat"], &[])));
        assert!(!palette.holds("creature.rat"));
        let mut palette = Palette::default();
        palette.apply(&full(1, &[]));
        // The right base, but a revision that skips one.
        assert!(palette.apply(&delta(3, 1, &["creature.rat"], &[])));
        // No revision follows the last one.
        let mut palette = Palette::default();
        palette.apply(&full(u64::MAX, &[]));
        assert!(palette.apply(&delta(u64::MAX, u64::MAX, &[], &[])));
    }

    #[test]
    fn an_older_full_palette_is_ignored() {
        let mut palette = Palette::default();
        palette.apply(&full(3, &["creature.rat"]));
        assert!(!palette.apply(&full(2, &["item.coin"])));
        assert_eq!(palette.revision(), Some(3));
        assert_eq!(held(&palette), ["creature.rat"]);
    }

    #[test]
    fn a_missing_asset_asks_once_per_asset_and_only_after_a_palette() {
        let mut palette = Palette::default();
        // The attach palette is still on its way.
        assert!(!palette.notice(["creature.rat"]));
        palette.apply(&full(1, &["creature.rat"]));
        assert!(!palette.notice(["creature.rat"]));
        assert!(palette.notice(["creature.rat", "item.coin"]));
        // The answer still lacks it: no second request for the same asset.
        palette.apply(&full(2, &["creature.rat"]));
        assert!(!palette.notice(["item.coin"]));
        assert!(palette.notice(["item.gem"]));
    }

    #[test]
    fn lookups_fall_back_through_dotted_prefixes_then_to_the_default_look() {
        let table = AssetTable::new([("terrain.floor", "flagstone"), ("terrain", "rock")]);
        assert_eq!(table.get("terrain.floor.cave"), Some(&"flagstone"));
        assert_eq!(table.get("terrain.floor"), Some(&"flagstone"));
        assert_eq!(table.get("terrain.wall.cave"), Some(&"rock"));
        assert_eq!(table.get("creature.rat"), None);
        // A prefix must end at a dot.
        assert_eq!(table.get("terrainx"), None);
        assert_eq!(table.get(""), None);

        let mut palette = Palette::default();
        assert_eq!(palette.resolve(&table, Some("terrain.floor.cave")), None);
        palette.apply(&full(1, &["terrain.floor.cave", "creature.rat"]));
        assert_eq!(
            palette.resolve(&table, Some("terrain.floor.cave")),
            Some(&"flagstone")
        );
        assert_eq!(palette.resolve(&table, Some("creature.rat")), None);
        // Not in the palette: the default look, even though the table has it.
        assert_eq!(palette.resolve(&table, Some("terrain.wall.cave")), None);
        assert_eq!(palette.resolve(&table, None), None);
    }
    #[test]
    fn current_and_frozen_object_assets_request_the_cosmetic_palette_only() {
        use tor_protocol::*;
        let presentation = |name: &str| ObjectPresentation {
            name: "object".into(),
            asset: Some(name.into()),
            kind: ObjectPresentationKind::Stairs,
        };
        let mut o = crate::test_support::observation();
        let mut cell: CellView = serde_json::from_value(serde_json::json!({
            "key":"here","position":{"x":0,"y":0,"z":0},"wall":false,"lit":true,
            "door":null,"trap":null,"stairs_up":false,"stairs_down":true,"place_hint":false,
            "object_presentation":null
        }))
        .unwrap();
        cell.object_presentation = Some(presentation("item.potion.false"));
        o.visible_cells.push(cell);
        o.ground_items.push(GroundItemView {
            presentation: Some(presentation("item.wand.false")),
            reachable: false,
            supply_reachable: false,
            position: o.position,
            item: ItemView {
                corpse: None,
                class: ItemClass::Misc,
                quantity: 1,
                appearance: "object".into(),
                identified: false,
                description: String::new(),
                id: ItemTarget::from_digest([81; 32]),
                name: "object".into(),
                asset: None,
                known_supply: None,
                known_currency: None,
            },
        });
        let found: BTreeSet<_> = observation_assets(&o).collect();
        assert_eq!(
            found,
            BTreeSet::from(["item.potion.false", "item.wand.false"])
        );
        let remembered = crate::map_memory::appearance(&o.visible_cells[0], &[&o.ground_items[0]]);
        let rows = [remembered];
        assert_eq!(map_assets(&rows).collect::<BTreeSet<_>>(), found);
        let mut palette = Palette::default();
        palette.apply(&full(1, &[]));
        assert!(palette.notice(map_assets(&rows)));
        palette.apply(&full(2, &["item.potion.false", "item.wand.false"]));
        assert!(!palette.notice(observation_assets(&o)));
    }
}
