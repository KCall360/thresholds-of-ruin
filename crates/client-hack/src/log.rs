//! Wrapped rows for the ASCII message area. `ClientState::narration` stays separate.

pub const MESSAGE_WIDTH: usize = 75;
pub const UNACKED_CAP: usize = 32;
pub const SCROLLBACK_CAP: usize = 256;
pub const SKIPPED: &str = "Earlier messages were skipped.";
pub const MORE: &str = "--More--";

const PAGE_ROWS: usize = 3;
const PAGE_KEEP: usize = 2;

/// Unacknowledged rows and the acknowledged scrollback behind them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MessageLog {
    pending: Vec<String>,
    scrollback: Vec<String>,
    scrollback_open: bool,
}

impl MessageLog {
    pub fn append(&mut self, text: &str) {
        self.extend_rows(wrap(text, MESSAGE_WIDTH));
    }

    pub fn append_narration<'a>(&mut self, lines: impl IntoIterator<Item = &'a str>) {
        for line in lines {
            self.append(line);
        }
    }

    pub fn clear_for_snapshot(&mut self) {
        self.clear();
    }

    pub fn clear_for_branch_change(&mut self) {
        self.clear();
    }

    pub fn more(&self) -> bool {
        self.pending.len() > PAGE_ROWS
    }

    /// Rows painted in the three-line message area, including `--More--` when paging.
    pub fn display(&self) -> Vec<String> {
        if self.more() {
            let mut rows: Vec<String> = self.pending.iter().take(PAGE_KEEP).cloned().collect();
            rows.push(MORE.to_owned());
            rows
        } else {
            self.pending.clone()
        }
    }

    /// Move one page into scrollback. A short message stays until the player pages.
    pub fn acknowledge(&mut self) -> bool {
        if !self.more() {
            return false;
        }
        let page: Vec<String> = self.pending.drain(..PAGE_KEEP).collect();
        self.scrollback.extend(page);
        if self.scrollback.len() > SCROLLBACK_CAP {
            let extra = self.scrollback.len() - SCROLLBACK_CAP;
            self.scrollback.drain(..extra);
        }
        true
    }

    pub fn pending(&self) -> &[String] {
        &self.pending
    }

    pub fn scrollback(&self) -> &[String] {
        &self.scrollback
    }

    pub fn scrollback_open(&self) -> bool {
        self.scrollback_open
    }

    pub fn open_scrollback(&mut self) {
        self.scrollback_open = true;
    }

    pub fn close_scrollback(&mut self) {
        self.scrollback_open = false;
    }

    fn extend_rows(&mut self, rows: impl IntoIterator<Item = String>) {
        self.pending.extend(rows);
        if self.pending.len() > UNACKED_CAP {
            let drop_count = self.pending.len() - (UNACKED_CAP - 1);
            self.pending.drain(..drop_count);
            self.pending.insert(0, SKIPPED.to_owned());
        }
    }

    fn clear(&mut self) {
        self.pending.clear();
        self.scrollback.clear();
        self.scrollback_open = false;
    }
}

/// A control character becomes a space, then the line breaks into `width` columns.
pub fn wrap(text: &str, width: usize) -> Vec<String> {
    let chars: Vec<char> = text
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    chars
        .chunks(width.max(1))
        .map(|line| line.iter().collect())
        .collect()
}
