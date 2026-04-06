use crate::proc::{collect_app_memory, AppMemInfo};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortColumn {
    Pss,
    Swap,
    Total,
    Procs,
    Name,
}

impl SortColumn {
    pub fn from_str_loose(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "pss" | "rss" => Some(Self::Pss),
            "swap" | "swp" => Some(Self::Swap),
            "total" | "tot" => Some(Self::Total),
            "procs" | "proc" | "nproc" | "nprocs" => Some(Self::Procs),
            "name" | "exe" | "cmd" => Some(Self::Name),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Pss => "PSS",
            Self::Swap => "SWAP",
            Self::Total => "TOTAL",
            Self::Procs => "NPROC",
            Self::Name => "COMMAND",
        }
    }

    pub const ALL: [SortColumn; 5] = [
        Self::Pss,
        Self::Swap,
        Self::Total,
        Self::Procs,
        Self::Name,
    ];
}

pub struct App {
    pub entries: Vec<AppMemInfo>,
    pub sort_col: SortColumn,
    pub sort_ascending: bool,
    pub scroll_offset: usize,
    pub total_pss: u64,
    pub total_swap: u64,
    pub total_mem: u64,
    pub total_procs: u32,
}

impl App {
    pub fn new(sort_col: SortColumn) -> Self {
        Self {
            entries: Vec::new(),
            sort_col,
            sort_ascending: false,
            scroll_offset: 0,
            total_pss: 0,
            total_swap: 0,
            total_mem: 0,
            total_procs: 0,
        }
    }

    pub fn refresh(&mut self) {
        let mut entries = collect_app_memory();
        self.sort_entries(&mut entries);
        self.total_pss = entries.iter().map(|e| e.pss_kib).sum();
        self.total_swap = entries.iter().map(|e| e.swap_kib).sum();
        self.total_mem = entries.iter().map(|e| e.total_kib).sum();
        self.total_procs = entries.iter().map(|e| e.num_procs).sum();
        self.entries = entries;
        if self.scroll_offset >= self.entries.len() {
            self.scroll_offset = self.entries.len().saturating_sub(1);
        }
    }

    fn sort_entries(&self, entries: &mut [AppMemInfo]) {
        let asc = self.sort_ascending;
        entries.sort_by(|a, b| {
            let ord = match self.sort_col {
                SortColumn::Pss => a.pss_kib.cmp(&b.pss_kib),
                SortColumn::Swap => a.swap_kib.cmp(&b.swap_kib),
                SortColumn::Total => a.total_kib.cmp(&b.total_kib),
                SortColumn::Procs => a.num_procs.cmp(&b.num_procs),
                SortColumn::Name => a.exe.cmp(&b.exe),
            };
            if asc { ord } else { ord.reverse() }
        });
    }

    pub fn cycle_sort(&mut self) {
        let idx = SortColumn::ALL
            .iter()
            .position(|&c| c == self.sort_col)
            .unwrap_or(0);
        self.sort_col = SortColumn::ALL[(idx + 1) % SortColumn::ALL.len()];
        self.sort_entries_in_place();
    }

    pub fn toggle_sort_order(&mut self) {
        self.sort_ascending = !self.sort_ascending;
        self.sort_entries_in_place();
    }

    pub fn set_sort(&mut self, col: SortColumn) {
        if self.sort_col == col {
            self.sort_ascending = !self.sort_ascending;
        } else {
            self.sort_col = col;
            self.sort_ascending = false;
        }
        self.sort_entries_in_place();
    }

    fn sort_entries_in_place(&mut self) {
        let mut entries = std::mem::take(&mut self.entries);
        self.sort_entries(&mut entries);
        self.entries = entries;
    }

    pub fn scroll_up(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_sub(1);
    }

    pub fn scroll_down(&mut self) {
        if self.scroll_offset + 1 < self.entries.len() {
            self.scroll_offset += 1;
        }
    }

    pub fn scroll_page_up(&mut self, page_size: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(page_size);
    }

    pub fn scroll_page_down(&mut self, page_size: usize) {
        self.scroll_offset = (self.scroll_offset + page_size).min(self.entries.len().saturating_sub(1));
    }

    pub fn scroll_home(&mut self) {
        self.scroll_offset = 0;
    }

    pub fn scroll_end(&mut self) {
        self.scroll_offset = self.entries.len().saturating_sub(1);
    }
}

pub fn format_mib(kib: u64) -> String {
    let mib = kib as f64 / 1024.0;
    if mib >= 1024.0 {
        format!("{:.1} GiB", mib / 1024.0)
    } else if mib >= 1.0 {
        format!("{:.1} MiB", mib)
    } else {
        format!("{:.0} KiB", kib as f64)
    }
}
