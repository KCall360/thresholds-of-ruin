//! Read-only complete chart hydration and bounded presentation, independent of
//! observation entities, ordinary map memory, navigation and action referents.
use crate::ClientState;
use std::sync::Arc;
use tor_protocol::{chart_pages::*, *};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChartInput {
    Overview,
    Select(ChartKind),
    Next,
    Previous,
    Cancel,
}
pub fn parse(text: &str) -> Result<ChartInput, String> {
    match text.trim().to_ascii_lowercase().as_str() {
        "" | "overview" => Ok(ChartInput::Overview),
        "monster" | "monsters" => Ok(ChartInput::Select(ChartKind::Monsters)),
        "food" => Ok(ChartInput::Select(ChartKind::Food)),
        "magic" => Ok(ChartInput::Select(ChartKind::Magic)),
        "mapping" => Ok(ChartInput::Select(ChartKind::Mapping)),
        "next" => Ok(ChartInput::Next), "previous" => Ok(ChartInput::Previous),
        "cancel" => Ok(ChartInput::Cancel),
        _ => Err("Use charts, chart monsters, chart food, chart magic, chart mapping, chart next, chart previous, or chart cancel.".into()),
    }
}
pub fn kind_label(kind: ChartKind) -> &'static str {
    match kind {
        ChartKind::Monsters => "Monster detection",
        ChartKind::Food => "Food detection",
        ChartKind::Magic => "Magic detection",
        ChartKind::Mapping => "Mapping",
    }
}
pub const DISPLAY_ROWS: usize = 20;
fn canonical_budget() -> ChartAssemblyBudget {
    ChartAssemblyBudget {
        components: MAX_CHART_TRANSFER_RECORDS,
        records: MAX_CHART_TRANSFER_RECORDS,
        encoded_bytes: MAX_CHART_TRANSFER_BYTES,
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Boundary {
    context: StreamContext,
    actor: ActorId,
    branch: BranchId,
    base: ObservationBase,
}
impl Boundary {
    fn of(state: &ClientState) -> Self {
        Self {
            context: state.context().clone(),
            actor: state.state().observation.actor,
            branch: state.branch().clone(),
            base: state.observation_base(),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChartDisplay {
    pub kind: ChartKind,
    pub mode: ChartMode,
    pub acquired_at: u64,
    pub components: Vec<ChartComponent>,
    pub rows: Vec<ChartRow>,
    token: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Assembly {
    summary: ChartSummary,
    query: ChartQuery,
    pending: Option<String>,
    proof: ChartStreamValidator,
    components: Vec<ChartComponent>,
    rows: Vec<ChartRow>,
    bytes: u64,
}
/// Native-thread handoff has one shared immutable complete chart, never a copy of
/// the world or a partial set of actionable facts. Only this module constructs it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChartPresentation {
    boundary: Boundary,
    selected: Option<ChartKind>,
    display: Option<Arc<ChartDisplay>>,
    failure: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChartPages {
    budget: ChartAssemblyBudget,
    boundary: Option<Boundary>,
    selected: Option<ChartKind>,
    active: Option<Assembly>,
    display: Option<Arc<ChartDisplay>>,
    failure: Option<String>,
    offset: usize,
}
impl Default for ChartPages {
    fn default() -> Self {
        Self::with_budget(canonical_budget())
    }
}
impl ChartPages {
    pub fn with_budget(budget: ChartAssemblyBudget) -> Self {
        Self {
            budget,
            boundary: None,
            selected: None,
            active: None,
            display: None,
            failure: None,
            offset: 0,
        }
    }
    pub fn selected(&self) -> Option<ChartKind> {
        self.selected
    }
    pub fn display(&self) -> Option<&ChartDisplay> {
        self.display.as_deref()
    }
    pub fn loading(&self) -> bool {
        self.active.is_some()
    }
    pub fn clear(&mut self) {
        self.boundary = None;
        self.selected = None;
        self.active = None;
        self.display = None;
        self.failure = None;
        self.offset = 0;
    }
    /// Any new view boundary invalidates current presentation. Selection is only
    /// a UI preference; it may start a fresh pure query, never rebind old rows.
    pub fn observe(&mut self, state: &ClientState) {
        let boundary = Boundary::of(state);
        if self.boundary.as_ref() != Some(&boundary) {
            self.boundary = Some(boundary);
            self.active = None;
            self.display = None;
            self.failure = None;
            self.offset = 0;
            if let Some(kind) = self.selected {
                self.begin(kind, state);
            }
        }
    }
    fn begin(&mut self, kind: ChartKind, state: &ClientState) {
        self.active = None;
        self.display = None;
        self.failure = None;
        self.offset = 0;
        let Some(summary) = state
            .state()
            .observation
            .charts
            .iter()
            .find(|s| s.kind == kind)
        else {
            return;
        };
        let Some(token) = &summary.chart else {
            return;
        };
        self.active = Some(Assembly {
            summary: summary.clone(),
            query: ChartQuery {
                context: state.context().clone(),
                branch: state.branch().clone(),
                base: state.observation_base(),
                chart: token.clone(),
                cursor: None,
            },
            pending: None,
            proof: ChartStreamValidator::new(state.state().observation.actor, self.budget),
            components: Vec::new(),
            rows: Vec::new(),
            bytes: 0,
        });
    }
    pub fn request(
        &mut self,
        input: ChartInput,
        state: &ClientState,
    ) -> Result<Option<Request>, String> {
        self.observe(state);
        match input {
            ChartInput::Overview | ChartInput::Cancel => {
                self.selected = None;
                self.active = None;
                self.display = None;
                self.failure = None;
                self.offset = 0;
            }
            ChartInput::Select(kind) => {
                if !state
                    .state()
                    .observation
                    .charts
                    .iter()
                    .any(|summary| summary.kind == kind)
                {
                    return Err(format!("{} has not been disclosed.", kind_label(kind)));
                }
                self.selected = Some(kind);
                self.begin(kind, state);
                return Ok(self.next_query().map(|query| Request::Chart { query }));
            }
            ChartInput::Next => {
                if let Some(display) = &self.display {
                    let end = display.rows.len().saturating_sub(1) / DISPLAY_ROWS * DISPLAY_ROWS;
                    self.offset = self.offset.saturating_add(DISPLAY_ROWS).min(end);
                }
            }
            ChartInput::Previous => self.offset = self.offset.saturating_sub(DISPLAY_ROWS),
        }
        Ok(None)
    }
    pub fn next_query(&self) -> Option<ChartQuery> {
        self.active
            .as_ref()
            .filter(|a| a.pending.is_none())
            .map(|a| a.query.clone())
    }
    pub fn pending(&self, request_id: &str, query: &ChartQuery) -> bool {
        self.active
            .as_ref()
            .is_some_and(|a| a.pending.as_deref() == Some(request_id) && &a.query == query)
    }
    pub fn bind(
        &mut self,
        query: &ChartQuery,
        request_id: &str,
        state: &ClientState,
    ) -> Result<(), String> {
        self.observe(state);
        if self.active.as_ref().is_none_or(|a| &a.query != query) {
            if query.cursor.is_some() {
                return Err("Chart continuation has no matching assembly.".into());
            }
            let kind = state
                .state()
                .observation
                .charts
                .iter()
                .find(|s| s.chart.as_ref() == Some(&query.chart))
                .map(|s| s.kind)
                .ok_or("Chart token is not in the current catalog.")?;
            self.selected = Some(kind);
            self.begin(kind, state);
        }
        let active = self.active.as_mut().ok_or("No available chart sample.")?;
        if active.pending.is_some() || &active.query != query {
            return Err("Chart query does not match current view.".into());
        }
        active.pending = Some(request_id.into());
        Ok(())
    }
    pub fn reject(&mut self, request_id: &str) -> bool {
        if self
            .active
            .as_ref()
            .is_none_or(|a| a.pending.as_deref() != Some(request_id))
        {
            return false;
        }
        self.active = None;
        self.display = None;
        self.failure =
            Some("The chart page request failed; choose the chart again to retry.".into());
        true
    }
    pub fn accept(
        &mut self,
        context: &ReplyContext,
        request_id: &str,
        page: &ChartPage,
        response_bytes: usize,
        state: &ClientState,
    ) -> Result<bool, String> {
        self.observe(state);
        state
            .validate_reply_context(context)
            .map_err(|_| "Chart reply does not match current view.")?;
        let active = self.active.as_mut().ok_or("No matching chart assembly.")?;
        if active.pending.as_deref() != Some(request_id) {
            return Err("Unexpected chart request identity.".into());
        }
        if page.kind != active.summary.kind
            || page.mode != active.summary.mode
            || Some(page.acquired_at) != active.summary.acquired_at
            || Some(&page.chart) != active.summary.chart.as_ref()
            || (active.summary.status == ChartStatus::Empty)
                != (page.total_records == page.total_components)
        {
            return Err("Chart page disagrees with advertised sample.".into());
        }
        if response_bytes > MAX_CHART_PAGE_BYTES {
            return Err("Chart page exceeds complete frame limit.".into());
        }
        let bytes = active
            .bytes
            .checked_add(u64::try_from(response_bytes).map_err(|_| "Chart byte count overflow.")?)
            .ok_or("Chart byte count overflow.")?;
        if bytes > self.budget.encoded_bytes {
            return Err("Chart display exceeds admitted retention budget.".into());
        }
        let components = page
            .records
            .iter()
            .filter(|record| matches!(record, ChartRecord::Component(_)))
            .count();
        let rows = page.records.len() - components;
        active
            .components
            .try_reserve(components)
            .map_err(|_| "Chart component allocation failed.")?;
        active
            .rows
            .try_reserve(rows)
            .map_err(|_| "Chart occurrence allocation failed.")?;
        // One bounded page clone; no whole retained chart/map clone. A later
        // protocol borrowed-reply helper can remove even this bounded adapter.
        let reply = ChartPageReply {
            message_type: ChartReplyType::ChartPage,
            context: context.clone(),
            request_id: request_id.into(),
            page: page.clone(),
        };
        active.proof.accept(&active.query, request_id, &reply)?;
        for record in &page.records {
            match record {
                ChartRecord::Component(value) => active.components.push(value.clone()),
                ChartRecord::Row(value) => active.rows.push(value.clone()),
            }
        }
        active.bytes = bytes;
        active.pending = None;
        if let Some(next) = page.next {
            active.query.cursor = Some(next);
            return Ok(false);
        }
        let active = self.active.take().expect("validated active chart");
        if !active.proof.is_complete() {
            return Err("Chart ended before complete validation.".into());
        }
        self.display = Some(Arc::new(ChartDisplay {
            kind: page.kind,
            mode: page.mode,
            acquired_at: page.acquired_at,
            token: page.chart.clone(),
            components: active.components,
            rows: active.rows,
        }));
        self.failure = None;
        Ok(true)
    }
    pub fn presentation(&self) -> Option<ChartPresentation> {
        Some(ChartPresentation {
            boundary: self.boundary.clone()?,
            selected: self.selected,
            display: self.display.clone(),
            failure: self.failure.clone(),
        })
    }
    pub fn install(
        &mut self,
        update: ChartPresentation,
        state: &ClientState,
    ) -> Result<(), String> {
        self.observe(state);
        if self.boundary.as_ref() != Some(&update.boundary) || self.selected != update.selected {
            return Err("Chart presentation belongs to another view or selection.".into());
        }
        if let Some(display) = &update.display {
            let summary = state
                .state()
                .observation
                .charts
                .iter()
                .find(|s| Some(s.kind) == self.selected)
                .ok_or("Chart sample is no longer advertised.")?;
            if summary.chart.as_deref() != Some(display.token.as_str())
                || summary.mode != display.mode
                || summary.acquired_at != Some(display.acquired_at)
            {
                return Err("Chart sample changed.".into());
            }
        }
        self.active = None;
        self.display = update.display;
        self.failure = update.failure;
        Ok(())
    }
    pub fn lines(&self, state: &ClientState) -> Vec<String> {
        let summaries = &state.state().observation.charts;
        let Some(kind) = self.selected else {
            let mut lines =
                vec!["Detection charts — read only; never movement or targeting.".into()];
            for summary in summaries {
                lines.extend(summary_lines(summary));
            }
            if summaries.is_empty() {
                lines.push("No detection charts have been disclosed.".into());
            }
            return lines;
        };
        let Some(summary) = summaries.iter().find(|s| s.kind == kind) else {
            return vec!["Chart is no longer disclosed.".into()];
        };
        let mut lines = summary_lines(summary);
        if self.boundary.as_ref() != Some(&Boundary::of(state)) {
            lines.push("Chart view changed; stale rows are withheld.".into());
            return lines;
        }
        if let Some(error) = &self.failure {
            lines.push(format!("Chart display unavailable: {error}"));
            return lines;
        }
        if self.active.is_some() {
            lines.push("Loading the complete chart…".into());
            return lines;
        }
        let Some(display) = &self.display else {
            return lines;
        };
        lines.push(format!(
            "{} separate component panel(s). Coordinates never connect different panels.",
            display.components.len()
        ));
        if display.rows.is_empty() {
            lines.push("No occurrences in this captured chart.".into());
            return lines;
        }
        let end = self
            .offset
            .saturating_add(DISPLAY_ROWS)
            .min(display.rows.len());
        lines.push(format!(
            "Occurrences {}–{} of {} (chart next / previous).",
            self.offset + 1,
            end,
            display.rows.len()
        ));
        for row in display.rows.iter().skip(self.offset).take(DISPLAY_ROWS) {
            let mapping_label;
            let label = match &row.mark {
                ChartMark::Monster { name, .. } if !name.is_empty() => name.as_str(),
                ChartMark::Monster { .. } => "creature",
                ChartMark::Food => "food",
                ChartMark::Magic => "magic",
                ChartMark::Mapping { fact } => {
                    mapping_label = match fact {
                        MappingMark::WallFace {
                            normal: Some([x, y, z]),
                        } => format!("wall face (normal {x}, {y}, {z})"),
                        MappingMark::WallFace { normal: None } => {
                            "wall face (orientation unavailable)".into()
                        }
                        MappingMark::Passage => "passage".into(),
                        MappingMark::Doorway => "doorway".into(),
                        MappingMark::Stairs => "stairs presence".into(),
                        MappingMark::SecretDoor => "secret door presence".into(),
                        MappingMark::TrapPresence => "trap presence".into(),
                    };
                    &mapping_label
                }
            };
            match row.position {
                Some([x, y, z]) => lines.push(format!(
                    "Panel {} at ({x}, {y}, {z}): {label}",
                    row.component + 1
                )),
                None => lines.push(format!(
                    "Panel {} — position unavailable (inconsistent joins): {label}",
                    row.component + 1
                )),
            }
        }
        lines
    }
}
fn summary_lines(summary: &ChartSummary) -> Vec<String> {
    let mode = match summary.mode {
        ChartMode::Live => "live",
        ChartMode::Historical => "historical",
    };
    let status = match summary.status {
        ChartStatus::Inactive => "inactive",
        ChartStatus::Empty => "empty",
        ChartStatus::Available => "available",
        ChartStatus::Unavailable => "unavailable",
    };
    let captured = summary
        .acquired_at
        .map(|tick| format!(", captured at tick {tick}"))
        .unwrap_or_default();
    let mut lines = vec![format!(
        "{}: {mode}, {status}{captured}.",
        kind_label(summary.kind)
    )];
    if !summary.supported {
        lines.push(
            "Unsupported in the current scope; retained historical samples remain historical."
                .into(),
        );
    }
    if let Some(attempt) = &summary.latest_attempt {
        let status = match attempt.status {
            ChartAttemptStatus::Empty => "empty",
            ChartAttemptStatus::Positive => "positive",
            ChartAttemptStatus::Unavailable => "unavailable",
        };
        lines.push(format!(
            "Latest attempt at tick {}: {status}.",
            attempt.attempted_at
        ));
    }
    lines
}

#[cfg(test)]
#[path = "charts/tests.rs"]
mod tests;
