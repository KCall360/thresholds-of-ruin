use super::*;

/// Consumer-owned retention admission, NOT a source/scenario or gameplay cap.
/// No default: each eventual consumer must select and justify its own budget.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChartAssemblyBudget {
    pub components: u64,
    pub records: u64,
    pub encoded_bytes: u64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Header {
    context: StreamContext,
    branch: BranchId,
    base: ObservationBase,
    chart: String,
    kind: ChartKind,
    mode: ChartMode,
    acquired_at: u64,
    total_components: u64,
    total_records: u64,
}
impl Header {
    fn of(page: &ChartPage) -> Self {
        Self {
            context: page.context.clone(),
            branch: page.branch.clone(),
            base: page.base.clone(),
            chart: page.chart.clone(),
            kind: page.kind,
            mode: page.mode,
            acquired_at: page.acquired_at,
            total_components: page.total_components,
            total_records: page.total_records,
        }
    }
}

/// Incremental structural proof only. Retains declarations and one prior row,
/// never a full chart, observation, map, source owner or actionable referent.
/// A caller must not publish a partial stream as an available/current chart.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChartStreamValidator {
    actor: crate::ActorId,
    budget: ChartAssemblyBudget,
    header: Option<Header>,
    alignments: Vec<ChartAlignment>,
    last_row: Option<ChartRow>,
    next: u64,
    encoded_bytes: u64,
    complete: bool,
}
impl ChartStreamValidator {
    pub fn new(actor: crate::ActorId, budget: ChartAssemblyBudget) -> Self {
        Self {
            actor,
            budget,
            header: None,
            alignments: Vec::new(),
            last_row: None,
            next: 0,
            encoded_bytes: 0,
            complete: false,
        }
    }
    pub fn is_complete(&self) -> bool {
        self.complete
    }
    pub fn records_seen(&self) -> u64 {
        self.next
    }
    pub fn components_seen(&self) -> usize {
        self.alignments.len()
    }
    pub fn encoded_bytes(&self) -> u64 {
        self.encoded_bytes
    }

    /// On failure no cursor/header/declaration/row is committed. The caller's
    /// retained rows must likewise publish only after this proof succeeds.
    pub fn accept(
        &mut self,
        query: &ChartQuery,
        request_id: &str,
        reply: &ChartPageReply,
    ) -> Result<(), &'static str> {
        reply.validate_for(query, request_id)?;
        let page = &reply.page;
        let header = Header::of(page);
        if reply.context.actor != self.actor
            || self.complete
            || page.start != self.next
            || self.header.as_ref().is_some_and(|old| old != &header)
            || (self.header.is_none() && query.cursor.is_some())
        {
            return Err("chart stream was reordered, replaced or already complete");
        }
        if page.total_components > self.budget.components
            || page.total_records > self.budget.records
        {
            return Err("chart exceeds caller retention count budget");
        }
        let bytes = crate::encoded_length(reply, MAX_CHART_PAGE_BYTES)
            .map_err(|_| "chart encoding failed")?
            .ok_or("chart page exceeds byte bound")?;
        let encoded_bytes = self
            .encoded_bytes
            .checked_add(u64::try_from(bytes).map_err(|_| "chart byte count overflow")?)
            .ok_or("chart byte count overflow")?;
        if encoded_bytes > self.budget.encoded_bytes {
            return Err("chart exceeds caller retention byte budget");
        }
        let mut previous = self.last_row.as_ref();
        let mut components_added = 0usize;
        let prior_components =
            u64::try_from(self.alignments.len()).map_err(|_| "chart component count overflow")?;
        for record in &page.records {
            match record {
                ChartRecord::Component(_) => components_added += 1,
                ChartRecord::Row(row) => {
                    let alignment = if row.component < prior_components {
                        // Safe after the less-than comparison against this usize length.
                        self.alignments.get(row.component as usize).copied()
                    } else {
                        let local = usize::try_from(row.component - prior_components)
                            .map_err(|_| "chart component index exceeds platform")?;
                        match page.records.get(local) {
                            Some(ChartRecord::Component(component))
                                if component.component == row.component =>
                            {
                                Some(component.alignment)
                            }
                            _ => None,
                        }
                    }
                    .ok_or("chart row refers to an undeclared component")?;
                    if !matches!(
                        (alignment, row.position),
                        (ChartAlignment::Aligned, Some(_)) | (ChartAlignment::Inconsistent, None)
                    ) {
                        return Err("chart coordinates disagree with declared alignment");
                    }
                    if previous.is_some_and(|last| last.order_key() > row.order_key()) {
                        return Err("chart occurrences descend across a page boundary");
                    }
                    previous = Some(row);
                }
            }
        }
        // Reserve only this bounded page's declarations, never an untrusted total.
        // All validation is above this boundary; no whole-chart clone is required.
        self.alignments
            .try_reserve(components_added)
            .map_err(|_| "chart component allocation failed")?;
        let last_row = previous.cloned();
        for record in &page.records {
            if let ChartRecord::Component(component) = record {
                self.alignments.push(component.alignment);
            }
        }
        self.last_row = last_row;
        self.header = Some(header);
        self.next = page.start + page.records.len() as u64;
        self.encoded_bytes = encoded_bytes;
        self.complete = page.next.is_none();
        Ok(())
    }
}
