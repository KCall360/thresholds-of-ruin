//! Read-only explored-map transfers with post-admission cursor commits.
use super::*;
use crate::map_memory::PreparedMapProjection;

/// The prepared appearance values and their knowledge root are immutable. A
/// continuation clones only this small handle and the shared projection.
#[derive(Clone)]
pub(super) struct MapTransfer {
    context: StreamContext,
    actor: ActorId,
    branch: BranchId,
    base: ObservationBase,
    window: MapWindow,
    id: String,
    projection: Arc<PreparedMapProjection>,
    next: usize,
    page: u16,
    bytes: usize,
}

fn stale() -> Failure {
    Failure::new(ErrorCode::StaleContext, "Explored map context changed")
}

fn invalid() -> Failure {
    Failure::new(ErrorCode::InvalidArchive, "Invalid explored map projection")
}

fn resource() -> Failure {
    Failure::new(
        ErrorCode::ResourceLimit,
        "Explored map exceeds transfer limits",
    )
}

impl Service {
    pub(super) fn prepare_map_page(
        &self,
        id: u64,
        query: MapQuery,
    ) -> Result<MapTransfer, Failure> {
        query
            .validate()
            .map_err(|_| Failure::new(ErrorCode::InvalidRequest, "Invalid explored map query"))?;
        let client = self.clients.get(&id).ok_or_else(stale)?;
        let actor = client.actor.ok_or_else(stale)?;
        let observed = client.last_observation.as_ref().ok_or_else(stale)?;
        if client.context.as_ref() != Some(&query.context)
            || observed.branch != query.branch
            || observed.base != query.base
            || self.engine.branch() != &query.branch
            || self.engine.revision(actor).ok() != Some(query.base.revision)
        {
            return Err(stale());
        }
        if let Some(cursor) = query.cursor {
            let transfer = client.map_transfer.as_ref().ok_or_else(stale)?;
            if transfer.context != query.context
                || transfer.actor != actor
                || transfer.branch != query.branch
                || transfer.base != query.base
                || transfer.window != query.window
                || transfer.id != cursor.transfer
                || transfer.next != cursor.next as usize
                || transfer.page != cursor.page
                || !self
                    .engine
                    .map_projection_is_current(actor, &transfer.projection)
            {
                return Err(stale());
            }
            return Ok(transfer.clone());
        }
        let projection = self.engine.explored_map(actor, query.window)?;
        if projection.cells.len() > MAX_MAP_TRANSFER_CELLS {
            return Err(resource());
        }
        Ok(MapTransfer {
            context: query.context,
            actor,
            branch: query.branch,
            base: query.base,
            window: query.window,
            id: uuid::Uuid::new_v4().to_string(),
            projection: Arc::new(projection),
            next: 0,
            page: 0,
            bytes: 0,
        })
    }

    pub(super) fn publish_map_page(
        &mut self,
        id: u64,
        request_id: &str,
        mut transfer: MapTransfer,
    ) -> Result<(), Failure> {
        // Readiness publication can advance the general stream cursor without
        // changing its observation base. Re-read the exact observation identity.
        let client = self.clients.get(&id).ok_or_else(stale)?;
        let observed = client.last_observation.as_ref().ok_or_else(stale)?;
        if client.context.as_ref() != Some(&transfer.context)
            || client.actor != Some(transfer.actor)
            || observed.branch != transfer.branch
            || observed.base != transfer.base
            || self.engine.branch() != &transfer.branch
            || self.engine.revision(transfer.actor).ok() != Some(transfer.base.revision)
            || !self
                .engine
                .map_projection_is_current(transfer.actor, &transfer.projection)
        {
            return Err(stale());
        }
        let context = self.reply_context(id).ok_or_else(stale)?;
        let limit =
            (self.outbound.capabilities().max_response_bytes as usize).min(MAX_MAP_PAGE_BYTES);
        let (page, bytes) = fit_page(&transfer, &context, request_id, limit)?;
        let next = page.next.clone();
        let total_bytes = transfer.bytes.checked_add(bytes).ok_or_else(resource)?;
        if total_bytes > MAX_MAP_TRANSFER_BYTES {
            return Err(resource());
        }
        let message = ServerMessage::MapPage {
            context,
            request_id: request_id.into(),
            page: Box::new(page),
        };
        if !self.admit_output(id, &message) {
            // Output rejection has disconnected and dropped all client state.
            return Ok(());
        }
        if let Some(client) = self.clients.get_mut(&id) {
            client.waiting = None;
            client.map_transfer = next.map(|next| {
                transfer.next = next.next as usize;
                transfer.page = next.page;
                transfer.bytes = total_bytes;
                transfer
            });
        }
        Ok(())
    }
}

/// Count each appearance once, then add the small, fully correlated envelope.
/// This avoids cloning a large candidate page or repeatedly serializing its
/// growing cell prefix. Only the final admitted-size prefix is copied.
fn fit_page(
    transfer: &MapTransfer,
    context: &ReplyContext,
    request_id: &str,
    limit: usize,
) -> Result<(MapPage, usize), Failure> {
    #[derive(serde::Serialize)]
    struct Envelope<'a> {
        #[serde(rename = "type")]
        kind: &'static str,
        context: &'a ReplyContext,
        request_id: &'a str,
        page: &'a MapPage,
    }
    let remaining = transfer
        .projection
        .cells
        .get(transfer.next..)
        .ok_or_else(stale)?;
    if transfer.page as usize >= MAX_MAP_TRANSFER_PAGES {
        return Err(resource());
    }
    let mut page = MapPage {
        transfer: transfer.id.clone(),
        actor: transfer.actor,
        branch: transfer.branch.clone(),
        base: transfer.base,
        window: transfer.window,
        start: u32::try_from(transfer.next).map_err(|_| invalid())?,
        page: transfer.page,
        total_cells: u32::try_from(transfer.projection.cells.len()).map_err(|_| resource())?,
        complete: transfer.projection.complete,
        ambiguous: transfer.projection.ambiguous,
        limited: transfer.projection.limited,
        cells: Vec::new(),
        next: None,
    };
    let envelope_len = |page: &MapPage| {
        encoded_length(
            &Envelope {
                kind: "map_page",
                context,
                request_id,
                page,
            },
            limit,
        )
        .map_err(|_| invalid())
    };
    let mut selected = 0usize;
    let mut cell_bytes = 0usize;
    let mut page_bytes = envelope_len(&page)?.ok_or_else(resource)?;
    for cell in remaining.iter().take(MAX_MAP_PAGE_CELLS) {
        let Some(bytes) = encoded_length(cell, limit).map_err(|_| invalid())? else {
            if selected == 0 {
                return Err(resource());
            }
            break;
        };
        let count = selected + 1;
        let end = transfer.next + count;
        let next = (end < transfer.projection.cells.len()).then(|| MapPageCursor {
            transfer: transfer.id.clone(),
            next: end as u32,
            page: transfer.page + 1,
        });
        let previous_next = std::mem::replace(&mut page.next, next);
        let contents = cell_bytes.checked_add(bytes).ok_or_else(resource)?;
        let total = envelope_len(&page)?
            .and_then(|overhead| overhead.checked_add(contents))
            .and_then(|bytes| bytes.checked_add(count - 1));
        let Some(total) = total.filter(|&bytes| bytes <= limit) else {
            page.next = previous_next;
            if selected == 0 {
                return Err(resource());
            }
            break;
        };
        selected = count;
        cell_bytes = contents;
        page_bytes = total;
    }
    if page
        .next
        .as_ref()
        .is_some_and(|next| next.page as usize >= MAX_MAP_TRANSFER_PAGES)
    {
        return Err(resource());
    }
    page.cells = remaining[..selected].to_vec();
    page.validate().map_err(|_| invalid())?;
    // The actual final envelope is the authority, including JSON escaping.
    if envelope_len(&page)? != Some(page_bytes) {
        return Err(invalid());
    }
    Ok((page, page_bytes))
}

#[cfg(test)]
#[path = "map_page_session_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "map_returned_floor_tests.rs"]
mod returned_floor_tests;
