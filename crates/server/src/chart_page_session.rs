//! Advertised immutable chart capabilities with post-admission cursor commits.
use super::*;
use tor_protocol::chart_pages::*;
use tor_simulation::chart::{self, delivery::PageLimits};

#[derive(Clone)]
pub(super) struct ChartTransfer {
    context: StreamContext,
    actor: ActorId,
    branch: BranchId,
    base: ObservationBase,
    summary: ChartSummary,
    chart: Arc<chart::Chart>,
    next: usize,
    bytes: usize,
}
fn stale() -> Failure {
    Failure::new(ErrorCode::StaleContext, "Chart context changed")
}
fn resource() -> Failure {
    Failure::new(ErrorCode::ResourceLimit, "Chart exceeds transfer limits")
}
fn invalid() -> Failure {
    Failure::new(ErrorCode::InvalidArchive, "Invalid chart projection")
}

impl Service {
    pub(super) fn prepare_chart_page(
        &self,
        id: u64,
        query: ChartQuery,
    ) -> Result<ChartTransfer, Failure> {
        query
            .validate()
            .map_err(|_| Failure::new(ErrorCode::InvalidRequest, "Invalid chart query"))?;
        let client = self.clients.get(&id).ok_or_else(stale)?;
        let actor = client.actor.ok_or_else(stale)?;
        let observed = self.issued_presentation(id, &query.base)?;
        if client.context.as_ref() != Some(&query.context)
            || observed.branch != query.branch
            || observed.base != query.base
            || self.engine.branch() != &query.branch
        {
            return Err(stale());
        }
        let summary = observed
            .charts
            .iter()
            .find(|summary| summary.chart.as_deref() == Some(query.chart.as_str()))
            .ok_or_else(stale)?;
        if let Some(cursor) = query.cursor {
            let transfer = client.chart_transfer.as_ref().ok_or_else(stale)?;
            if transfer.context != query.context
                || transfer.actor != actor
                || transfer.branch != query.branch
                || transfer.base != query.base
                || &transfer.summary != summary
                || transfer.next as u64 != cursor
            {
                return Err(stale());
            }
            return Ok(transfer.clone());
        }
        // A current boundary reuses its prepared chart Arc. An older issued
        // boundary reconstructs from its frozen Game once for this transfer;
        // continuations above retain that Arc. Neither path learns, resamples,
        // advances the clock, or mutates authoritative Game state.
        let chart = self
            .engine
            .prepared_detection_charts_at(&observed.boundary, actor)?
            .chart(summary.kind)
            .ok_or_else(stale)?;
        Ok(ChartTransfer {
            context: query.context,
            actor,
            branch: query.branch,
            base: query.base,
            summary: summary.clone(),
            chart,
            next: 0,
            bytes: 0,
        })
    }
    pub(super) fn publish_chart_page(
        &mut self,
        id: u64,
        request_id: &str,
        mut transfer: ChartTransfer,
    ) -> Result<(), Failure> {
        let client = self.clients.get(&id).ok_or_else(stale)?;
        let observed = self.issued_presentation(id, &transfer.base)?;
        if client.context.as_ref() != Some(&transfer.context)
            || client.actor != Some(transfer.actor)
            || observed.branch != transfer.branch
            || observed.base != transfer.base
            || self.engine.branch() != &transfer.branch
            || !observed.charts.contains(&transfer.summary)
        {
            return Err(stale());
        }
        let context = self.reply_context(id).ok_or_else(stale)?;
        let limit =
            (self.outbound.capabilities().max_response_bytes as usize).min(MAX_CHART_PAGE_BYTES);
        let (page, bytes) = fit_page(&transfer, &context, request_id, limit)?;
        let next = page.next;
        let total_bytes = transfer.bytes.checked_add(bytes).ok_or_else(resource)?;
        if total_bytes as u64 > MAX_CHART_TRANSFER_BYTES {
            return Err(resource());
        }
        let message = ServerMessage::ChartPage {
            context,
            request_id: request_id.into(),
            page: Box::new(page),
        };
        if !self.admit_output(id, &message) {
            return Ok(());
        }
        if let Some(client) = self.clients.get_mut(&id) {
            client.waiting = None;
            client.chart_transfer = next.map(|next| {
                transfer.next = next as usize;
                transfer.bytes = total_bytes;
                transfer
            });
        }
        Ok(())
    }
}

fn fit_page(
    transfer: &ChartTransfer,
    context: &ReplyContext,
    request_id: &str,
    limit: usize,
) -> Result<(ChartPage, usize), Failure> {
    let limits = PageLimits {
        bytes: limit,
        ..PageLimits::canonical()
    };
    let span = transfer
        .chart
        .page_span(transfer.next, limits)
        .map_err(|_| resource())?;
    let total = transfer.chart.record_count().map_err(|_| invalid())?;
    let page = ChartPage {
        context: transfer.context.clone(),
        branch: transfer.branch.clone(),
        base: transfer.base.clone(),
        chart: transfer.summary.chart.clone().ok_or_else(invalid)?,
        kind: transfer.summary.kind,
        mode: transfer.summary.mode,
        acquired_at: transfer.summary.acquired_at.ok_or_else(invalid)?,
        total_components: transfer.chart.components().len() as u64,
        total_records: total as u64,
        start: transfer.next as u64,
        records: (transfer.next..span.end)
            .map(|ordinal| crate::chart_view::record(&transfer.chart, ordinal).ok_or_else(invalid))
            .collect::<Result<_, _>>()?,
        next: (span.end < total).then_some(span.end as u64),
    };
    page.validate().map_err(|_| invalid())?;
    // Validate the complete actual correlated envelope, including escaped IDs,
    // before admission. The independently tested canonical bound must dominate it.
    let message = ServerMessage::ChartPage {
        context: context.clone(),
        request_id: request_id.into(),
        page: Box::new(page.clone()),
    };
    let bytes = encoded_length(&message, limit)
        .map_err(|_| invalid())?
        .ok_or_else(resource)?;
    if bytes > span.encoded_bytes_bound {
        return Err(invalid());
    }
    Ok((page, bytes))
}

#[cfg(test)]
#[path = "chart_page_session_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "chart_envelope_tests.rs"]
mod envelope_tests;
