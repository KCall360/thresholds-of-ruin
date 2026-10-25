//! Bounded immutable chart values. No census, selection, lifecycle or delivery authority.
//! No serde decoder: bounded transport/restore adapters must call these constructors.
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Monsters,
    Food,
    Magic,
}
#[derive(Clone, Copy, Debug)]
pub struct Budget {
    pub components: usize,
    pub rows: usize,
    pub json_bytes: usize,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Invalid {
    Components,
    Position,
    Kind,
    Order,
    Appearance,
    Arithmetic,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unavailable {
    Budget,
    Projection,
}

/// Local chart coordinates only: no world/region IDs or navigation handles.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Component {
    pub alignment: crate::scope_projection::Alignment,
}
/// Species/cosmetic tuple only: never personal identity or true appearance seed.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Mark {
    Monster(crate::appearance_catalog::CreatureAppearance),
    Food,
    Magic,
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Row {
    pub component: usize,
    pub position: Option<[i64; 3]>,
    pub mark: Mark,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chart {
    kind: Kind,
    components: Vec<Component>,
    rows: Vec<Row>,
    encoded_size_bound: usize,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Candidate(CandidateValue);
#[derive(Clone, Debug, PartialEq, Eq)]
enum CandidateValue {
    Available(Arc<Chart>),
    Unavailable(Unavailable),
}
impl Candidate {
    pub fn unavailable(reason: Unavailable) -> Self {
        Self(CandidateValue::Unavailable(reason))
    }
    pub fn chart(&self) -> Option<&Chart> {
        match &self.0 {
            CandidateValue::Available(c) => Some(c),
            _ => None,
        }
    }
    pub fn unavailable_reason(&self) -> Option<Unavailable> {
        match self.0 {
            CandidateValue::Unavailable(reason) => Some(reason),
            _ => None,
        }
    }
    /// Called only AFTER complete enumeration/projection, never on a prefix.
    /// Caller must precharge Budget during collection, not allocate then check.
    /// Complete coverage is a world-builder obligation, not provable from row data.
    pub fn from_complete(
        kind: Kind,
        components: Vec<Component>,
        rows: Vec<Row>,
        budget: Budget,
    ) -> Result<Self, Invalid> {
        if components.is_empty() {
            return Err(Invalid::Components);
        }
        // Canonical order retains equal rows as distinct occurrences. Their vector
        // indices are presentation-only, never actor/item targets or navigation.
        if rows.windows(2).any(|w| w[0] > w[1]) {
            return Err(Invalid::Order);
        }
        let mut size = 128usize; // fixed chart object, kind, list keys and delimiters
        for _ in &components {
            size = add(size, 96)?;
        }
        for row in &rows {
            let c = components.get(row.component).ok_or(Invalid::Position)?;
            match (c.alignment, row.position) {
                (crate::scope_projection::Alignment::Aligned, Some(_))
                | (crate::scope_projection::Alignment::Inconsistent, None) => {}
                _ => return Err(Invalid::Position),
            }
            let mark_bytes = match (&kind, &row.mark) {
                (Kind::Monsters, Mark::Monster(a)) => {
                    if !a.valid() {
                        return Err(Invalid::Appearance);
                    }
                    add(
                        96,
                        add(
                            escaped(&a.name)?,
                            match &a.asset {
                                Some(asset) => escaped(asset)?,
                                None => 4,
                            },
                        )?,
                    )?
                }
                (Kind::Food, Mark::Food) | (Kind::Magic, Mark::Magic) => 32,
                _ => return Err(Invalid::Kind),
            };
            // Three signed i64 values at most 20 bytes each, component usize
            // decimal, full keys/separators/null, and per-occurrence overhead.
            size = add(size, add(256, mark_bytes)?)?;
        }
        if components.len() > budget.components
            || rows.len() > budget.rows
            || size > budget.json_bytes
        {
            return Ok(Self::unavailable(Unavailable::Budget));
        }
        Ok(Self(CandidateValue::Available(Arc::new(Chart {
            kind,
            components,
            rows,
            encoded_size_bound: size,
        }))))
    }
}
impl Chart {
    pub fn kind(&self) -> Kind {
        self.kind
    }
    pub fn components(&self) -> &[Component] {
        &self.components
    }
    pub fn rows(&self) -> &[Row] {
        &self.rows
    }
    pub fn encoded_size_bound(&self) -> usize {
        self.encoded_size_bound
    }
}
fn add(a: usize, b: usize) -> Result<usize, Invalid> {
    a.checked_add(b).ok_or(Invalid::Arithmetic)
}
/// Six bytes per UTF-8 source byte conservatively covers all JSON escapes,
/// including quote/backslash and non-ASCII escaped as UTF-16 surrogate pairs.
fn escaped(text: &str) -> Result<usize, Invalid> {
    add(text.len().checked_mul(6).ok_or(Invalid::Arithmetic)?, 2)
}
#[cfg(test)]
#[path = "chart/tests.rs"]
mod tests;
