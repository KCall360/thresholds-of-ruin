//! One context-bound map transfer. Pages cannot change the displayed cache.
use std::collections::{BTreeMap, BTreeSet};
use tor_protocol::*;

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct MapHydrationStatus {
    pub pending: bool,
    pub complete: bool,
    pub ambiguous: bool,
    pub limited: bool,
    pub failed: bool,
    pub cells: usize,
    pub pages: usize,
    pub bytes: usize,
}

/// A validated atomic presentation handoff, never an actionable observation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MapMemoryUpdate {
    pub(crate) context: StreamContext,
    pub(crate) actor: ActorId,
    pub(crate) branch: BranchId,
    pub(crate) base: ObservationBase,
    pub(crate) window: MapWindow,
    pub(crate) cells: BTreeMap<Position, RememberedMapCell>,
    pub(crate) status: MapHydrationStatus,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Boundary {
    context: StreamContext,
    actor: ActorId,
    branch: BranchId,
    base: ObservationBase,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Assembly {
    query: MapQuery,
    request: Option<String>,
    transfer: Option<String>,
    total: Option<u32>,
    flags: Option<(bool, bool, bool)>,
    cells: BTreeMap<Position, RememberedMapCell>,
    pages: usize,
    bytes: usize,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct MapHydration {
    boundary: Option<Boundary>,
    active: Option<Assembly>,
    attempted: Option<MapWindow>,
    pub status: MapHydrationStatus,
}
impl MapHydration {
    pub fn observe(
        &mut self,
        context: StreamContext,
        actor: ActorId,
        branch: BranchId,
        base: ObservationBase,
        moved: bool,
    ) {
        let boundary = Boundary {
            context,
            actor,
            branch,
            base,
        };
        if self.boundary.as_ref() != Some(&boundary) {
            let reattached = self.boundary.as_ref().is_none_or(|old| {
                old.context != boundary.context
                    || old.actor != boundary.actor
                    || old.branch != boundary.branch
            });
            if self.active.is_some() || reattached || moved {
                self.attempted = None;
            }
            self.active = None;
            self.status.pending = false;
            if moved || reattached {
                self.status.complete = false;
            }
            self.boundary = Some(boundary);
        }
    }
    pub fn query(&mut self, window: MapWindow, needed: bool) -> Option<MapQuery> {
        if self.active.is_none() && needed && self.attempted != Some(window) {
            let boundary = self.boundary.as_ref()?;
            self.attempted = Some(window);
            self.status = MapHydrationStatus {
                pending: true,
                ..Default::default()
            };
            self.active = Some(Assembly {
                query: MapQuery {
                    context: boundary.context.clone(),
                    branch: boundary.branch.clone(),
                    base: boundary.base,
                    window,
                    cursor: None,
                },
                request: None,
                transfer: None,
                total: None,
                flags: None,
                cells: BTreeMap::new(),
                pages: 0,
                bytes: 0,
            });
        }
        self.active
            .as_ref()
            .filter(|a| a.request.is_none())
            .map(|a| a.query.clone())
    }
    pub fn bind(&mut self, query: &MapQuery, id: &str) -> Result<(), &'static str> {
        let active = self
            .active
            .as_mut()
            .ok_or("No map transfer was requested")?;
        if &active.query != query || active.request.is_some() {
            return Err("Mismatched map request");
        }
        active.request = Some(id.into());
        Ok(())
    }
    pub fn pending(&self, id: &str, query: &MapQuery) -> bool {
        self.active
            .as_ref()
            .is_some_and(|a| a.request.as_deref() == Some(id) && &a.query == query)
    }
    pub fn reject(&mut self, id: &str) -> bool {
        if !self
            .active
            .as_ref()
            .is_some_and(|a| a.request.as_deref() == Some(id))
        {
            return false;
        }
        self.active = None;
        self.status.pending = false;
        self.status.failed = true;
        self.status.complete = false;
        true
    }
    pub fn cancel(&mut self) {
        self.active = None;
        self.status.pending = false;
    }
    pub fn accept(
        &mut self,
        context: &ReplyContext,
        id: &str,
        page: &MapPage,
        bytes: usize,
    ) -> Result<Option<MapMemoryUpdate>, &'static str> {
        page.validate()?;
        let boundary = self.boundary.as_ref().ok_or("Map has no attachment")?;
        let active = self.active.as_ref().ok_or("Unsolicited map page")?;
        if active.request.as_deref() != Some(id) {
            return Err("Unsolicited or duplicate map page");
        }
        if context.input.stream != boundary.context
            || context.actor != boundary.actor
            || context.branch != boundary.branch
            || context.revision != boundary.base.revision
            || context.cursor.sequence < boundary.base.cursor.sequence
            || context.cursor.tick != boundary.base.cursor.tick
            || page.actor != boundary.actor
            || page.branch != boundary.branch
            || page.base != boundary.base
            || page.window != active.query.window
        {
            return Err("Map page belongs to another observation");
        }
        let expected = active.query.cursor.as_ref();
        if page.start as usize != active.cells.len()
            || page.page as usize != active.pages
            || expected.is_some_and(|c| {
                c.transfer != page.transfer || c.next != page.start || c.page != page.page
            })
            || active
                .transfer
                .as_ref()
                .is_some_and(|t| t != &page.transfer)
            || active.total.is_some_and(|n| n != page.total_cells)
            || active
                .flags
                .is_some_and(|flags| flags != (page.complete, page.ambiguous, page.limited))
        {
            return Err("Out-of-order or inconsistent map page");
        }
        let total_bytes = active
            .bytes
            .checked_add(bytes)
            .ok_or("Map transfer byte overflow")?;
        if bytes > MAX_MAP_PAGE_BYTES
            || total_bytes > MAX_MAP_TRANSFER_BYTES
            || active.pages >= MAX_MAP_TRANSFER_PAGES
            || active.cells.len() + page.cells.len() > MAX_MAP_TRANSFER_CELLS
        {
            return Err("Map transfer exceeds its budget");
        }
        let mut positions = BTreeSet::new();
        for cell in &page.cells {
            if active.cells.contains_key(&cell.position) || !positions.insert(cell.position) {
                return Err("Duplicate map position");
            }
        }
        // Validate a whole page before advancing continuation or touching staging.
        let active = self.active.as_mut().unwrap();
        for cell in &page.cells {
            active.cells.insert(cell.position, cell.clone());
        }
        active.transfer = Some(page.transfer.clone());
        active.total = Some(page.total_cells);
        active.flags = Some((page.complete, page.ambiguous, page.limited));
        active.bytes = total_bytes;
        active.pages += 1;
        active.request = None;
        self.status = MapHydrationStatus {
            pending: page.next.is_some(),
            complete: page.complete && page.next.is_none(),
            ambiguous: page.ambiguous,
            limited: page.limited,
            failed: false,
            cells: active.cells.len(),
            pages: active.pages,
            bytes: total_bytes,
        };
        if let Some(cursor) = &page.next {
            active.query.cursor = Some(cursor.clone());
            return Ok(None);
        }
        let active = self.active.take().unwrap();
        if !page.complete {
            return Ok(None);
        }
        // Aliases of one physical key must carry one appearance, with only their
        // presentation position differing. This never merges different positions.
        let mut seen: BTreeMap<&str, &RememberedMapCell> = BTreeMap::new();
        for cell in active.cells.values() {
            if let Some(previous) = seen.insert(&cell.key, cell) {
                let mut same = previous.clone();
                same.position = cell.position;
                if same != *cell {
                    self.status.complete = false;
                    self.status.failed = true;
                    return Err("Conflicting map appearances");
                }
            }
        }
        Ok(Some(MapMemoryUpdate {
            context: boundary.context.clone(),
            actor: boundary.actor,
            branch: boundary.branch.clone(),
            base: boundary.base,
            window: page.window,
            cells: active.cells,
            status: self.status.clone(),
        }))
    }
}
