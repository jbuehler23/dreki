//! dreki-telemetry — TUI diagnostics tool for dreki games.
//!
//! Connects to a running dreki game via UDP and displays real-time metrics
//! in an interactive btop-style terminal dashboard using ratatui.
//!
//! Run a dreki game with `--features diagnostics`, then run `cargo run -p dreki-telemetry`.

use std::collections::{HashSet, VecDeque};
use std::io;
use std::net::UdpSocket;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::execute;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Sparkline};
use ratatui::Terminal;
use serde::Deserialize;

// ── Wire types (must match dreki's JSON format) ─────────────────────────

#[derive(Deserialize, Clone, Default)]
struct DiagSnapshot {
    fps: f32,
    delta_ms: f32,
    frame_count: u64,
    elapsed_secs: f32,
    entity_count: usize,
    archetype_count: usize,
    archetypes: Vec<ArchetypeInfo>,
    render: Option<RenderStats>,
    #[serde(default)]
    system_timings: Option<Vec<SystemTimingInfo>>,
    #[serde(default)]
    frame_budget: Option<FrameBudgetInfo>,
    #[serde(default)]
    entity_pool: Option<EntityPoolInfo>,
    #[serde(default)]
    assets: Option<AssetInfo>,
    #[serde(default)]
    logs: Vec<LogEntryInfo>,
}

#[derive(Deserialize, Clone, Default)]
struct ArchetypeInfo {
    entity_count: usize,
    component_names: Vec<String>,
    entities: Option<Vec<EntityInfo>>,
}

#[derive(Deserialize, Clone, Default)]
struct EntityInfo {
    id: u32,
    generation: u32,
    components: Vec<ComponentInfo>,
}

#[derive(Deserialize, Clone, Default)]
struct ComponentInfo {
    name: String,
    debug_value: String,
}

#[derive(Deserialize, Clone, Default)]
struct RenderStats {
    draw_calls: u32,
    vertices: u32,
    textures_loaded: u32,
}

#[derive(Deserialize, Clone, Default)]
struct SystemTimingInfo {
    name: String,
    duration_us: f64,
}

#[derive(Deserialize, Clone, Default)]
struct FrameBudgetInfo {
    systems_us: f64,
    render_us: f64,
}

#[derive(Deserialize, Clone, Default)]
struct EntityPoolInfo {
    total_slots: u32,
    free_count: usize,
    alive_count: usize,
    spawned_this_tick: u32,
    despawned_this_tick: u32,
    fragmentation_pct: f32,
}

#[derive(Deserialize, Clone, Default)]
struct AssetInfo {
    watched_count: usize,
    watcher_active: bool,
    pending_count: usize,
    watched_files: Vec<(String, String)>,
    reload_events: Vec<ReloadEventInfo>,
}

#[derive(Deserialize, Clone, Default)]
struct ReloadEventInfo {
    timestamp_secs: f32,
    path: String,
    kind: String,
    success: bool,
    error: Option<String>,
}

#[derive(Deserialize, Clone, Default)]
struct LogEntryInfo {
    level: String,
    target: String,
    message: String,
    timestamp_secs: f32,
}

// ── Inspect request (sent to game) ───────────────────────────────────────

#[derive(serde::Serialize)]
struct InspectRequest {
    expanded_archetypes: Vec<usize>,
}

// ── Tabs ─────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Overview,
    Systems,
    Assets,
    Logs,
}

impl Tab {
    const ALL: [Tab; 4] = [Tab::Overview, Tab::Systems, Tab::Assets, Tab::Logs];

    fn next(self) -> Self {
        match self {
            Tab::Overview => Tab::Systems,
            Tab::Systems => Tab::Assets,
            Tab::Assets => Tab::Logs,
            Tab::Logs => Tab::Overview,
        }
    }

    fn prev(self) -> Self {
        match self {
            Tab::Overview => Tab::Logs,
            Tab::Systems => Tab::Overview,
            Tab::Assets => Tab::Systems,
            Tab::Logs => Tab::Assets,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Tab::Overview => "Overview",
            Tab::Systems => "Systems",
            Tab::Assets => "Assets",
            Tab::Logs => "Logs",
        }
    }
}

// ── Sort mode ────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq)]
enum SortMode {
    CountDesc,
    CountAsc,
    Alphabetical,
}

impl SortMode {
    fn next(self) -> Self {
        match self {
            SortMode::CountDesc => SortMode::CountAsc,
            SortMode::CountAsc => SortMode::Alphabetical,
            SortMode::Alphabetical => SortMode::CountDesc,
        }
    }

    fn label(self) -> &'static str {
        match self {
            SortMode::CountDesc => "count \u{2193}",
            SortMode::CountAsc => "count \u{2191}",
            SortMode::Alphabetical => "A-Z",
        }
    }
}

// ── Tree data model ─────────────────────────────────────────────────────

#[derive(Clone)]
enum TreeRow {
    Archetype { arch_idx: usize },
    Entity { arch_idx: usize, entity_row: usize },
    Component { arch_idx: usize, entity_row: usize, comp_idx: usize },
    /// A single field line inside an expanded component (display-only).
    Field { arch_idx: usize, entity_row: usize, comp_idx: usize, field_idx: usize },
}

impl TreeRow {
    fn is_selectable(&self) -> bool {
        !matches!(self, TreeRow::Field { .. })
    }
}

// ── Input mode ──────────────────────────────────────────────────────────

#[derive(Clone, PartialEq, Eq)]
enum InputMode {
    Normal,
    Search,
}

// ── Log level filter ────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq)]
enum LogFilter {
    All,
    Debug,
    Info,
    Warn,
    Error,
}

impl LogFilter {
    fn next(self) -> Self {
        match self {
            LogFilter::All => LogFilter::Debug,
            LogFilter::Debug => LogFilter::Info,
            LogFilter::Info => LogFilter::Warn,
            LogFilter::Warn => LogFilter::Error,
            LogFilter::Error => LogFilter::All,
        }
    }

    fn label(self) -> &'static str {
        match self {
            LogFilter::All => "ALL",
            LogFilter::Debug => "DEBUG+",
            LogFilter::Info => "INFO+",
            LogFilter::Warn => "WARN+",
            LogFilter::Error => "ERROR",
        }
    }

    fn passes(self, level: &str) -> bool {
        match self {
            LogFilter::All => true,
            LogFilter::Debug => level != "TRACE",
            LogFilter::Info => matches!(level, "INFO" | "WARN" | "ERROR"),
            LogFilter::Warn => matches!(level, "WARN" | "ERROR"),
            LogFilter::Error => level == "ERROR",
        }
    }
}

// ── Accumulated log entry (persists across snapshots) ───────────────────

#[derive(Clone)]
struct LogEntry {
    level: String,
    _target: String,
    message: String,
    timestamp_secs: f32,
}

// ── Accumulated reload event (persists across snapshots) ────────────────

#[derive(Clone)]
struct AccumReloadEvent {
    timestamp_secs: f32,
    path: String,
    kind: String,
    success: bool,
    error: Option<String>,
}

// ── App state ────────────────────────────────────────────────────────────

const HISTORY_CAP: usize = 1200;
const LOG_CAP: usize = 2000;
const RELOAD_LOG_CAP: usize = 500;

struct App {
    latest: DiagSnapshot,
    fps_history: VecDeque<u64>,
    delta_history: VecDeque<u64>,
    active_tab: Tab,
    paused: bool,
    connected: bool,
    /// Socket for sending inspect requests to the game (port 9101).
    request_socket: UdpSocket,

    // Tree state (Overview tab)
    expanded_archetypes: HashSet<usize>,
    expanded_entities: HashSet<(usize, usize)>,
    expanded_components: HashSet<(usize, usize, usize)>,
    /// Cursor index into the list of selectable rows.
    cursor: usize,

    // Sort
    sort_mode: SortMode,

    // Search
    input_mode: InputMode,
    search_query: String,
    active_filter: Option<String>,

    // Logs tab state
    log_entries: Vec<LogEntry>,
    log_filter: LogFilter,
    log_auto_scroll: bool,
    log_scroll_offset: usize,

    // Assets tab state
    reload_log: Vec<AccumReloadEvent>,
}

impl App {
    fn new(request_socket: UdpSocket) -> Self {
        Self {
            latest: DiagSnapshot::default(),
            fps_history: VecDeque::with_capacity(HISTORY_CAP),
            delta_history: VecDeque::with_capacity(HISTORY_CAP),
            active_tab: Tab::Overview,
            paused: false,
            connected: false,
            request_socket,
            expanded_archetypes: HashSet::new(),
            expanded_entities: HashSet::new(),
            expanded_components: HashSet::new(),
            cursor: 0,
            sort_mode: SortMode::CountDesc,
            input_mode: InputMode::Normal,
            search_query: String::new(),
            active_filter: None,
            log_entries: Vec::new(),
            log_filter: LogFilter::Info,
            log_auto_scroll: true,
            log_scroll_offset: 0,
            reload_log: Vec::new(),
        }
    }

    fn push_snapshot(&mut self, snap: DiagSnapshot) {
        if self.paused {
            return;
        }

        if self.fps_history.len() >= HISTORY_CAP {
            self.fps_history.pop_front();
        }
        self.fps_history.push_back(snap.fps.round().max(0.0) as u64);

        if self.delta_history.len() >= HISTORY_CAP {
            self.delta_history.pop_front();
        }
        self.delta_history
            .push_back((snap.delta_ms * 1000.0).round().max(0.0) as u64);

        // Accumulate log entries.
        for log in &snap.logs {
            self.log_entries.push(LogEntry {
                level: log.level.clone(),
                _target: log.target.clone(),
                message: log.message.clone(),
                timestamp_secs: log.timestamp_secs,
            });
        }
        // Cap log entries.
        if self.log_entries.len() > LOG_CAP {
            let excess = self.log_entries.len() - LOG_CAP;
            self.log_entries.drain(..excess);
        }

        // Accumulate reload events.
        if let Some(assets) = &snap.assets {
            for ev in &assets.reload_events {
                self.reload_log.push(AccumReloadEvent {
                    timestamp_secs: ev.timestamp_secs,
                    path: ev.path.clone(),
                    kind: ev.kind.clone(),
                    success: ev.success,
                    error: ev.error.clone(),
                });
            }
            if self.reload_log.len() > RELOAD_LOG_CAP {
                let excess = self.reload_log.len() - RELOAD_LOG_CAP;
                self.reload_log.drain(..excess);
            }
        }

        self.latest = snap;
        self.connected = true;

        // Clamp cursor to selectable count.
        let (_, selectable) = self.build_tree_rows();
        if !selectable.is_empty() && self.cursor >= selectable.len() {
            self.cursor = selectable.len() - 1;
        }
    }

    fn send_expand_request(&self) {
        let expanded: Vec<usize> = self.expanded_archetypes.iter().copied().collect();
        let req = InspectRequest {
            expanded_archetypes: expanded,
        };
        if let Ok(json) = serde_json::to_vec(&req) {
            let _ = self.request_socket.send(&json);
        }
    }

    /// Get the ordered list of archetype indices after applying filter + sort.
    fn filtered_sorted_archetypes(&self) -> Vec<usize> {
        let mut indices: Vec<usize> = (0..self.latest.archetypes.len())
            .filter(|&i| {
                if let Some(filter) = &self.active_filter {
                    let lower = filter.to_lowercase();
                    let arch = &self.latest.archetypes[i];
                    // Match against component names.
                    let names_match = arch
                        .component_names
                        .iter()
                        .any(|n| n.to_lowercase().contains(&lower));
                    // Match against entity IDs.
                    let entity_match = arch.entities.as_ref().is_some_and(|ents| {
                        ents.iter()
                            .any(|e| format!("{}", e.id).contains(&lower))
                    });
                    names_match || entity_match
                } else {
                    true
                }
            })
            .collect();

        match self.sort_mode {
            SortMode::CountDesc => {
                indices.sort_by(|&a, &b| {
                    self.latest.archetypes[b]
                        .entity_count
                        .cmp(&self.latest.archetypes[a].entity_count)
                });
            }
            SortMode::CountAsc => {
                indices.sort_by(|&a, &b| {
                    self.latest.archetypes[a]
                        .entity_count
                        .cmp(&self.latest.archetypes[b].entity_count)
                });
            }
            SortMode::Alphabetical => {
                indices.sort_by(|&a, &b| {
                    let name_a = self.latest.archetypes[a]
                        .component_names
                        .first()
                        .map(|s| s.as_str())
                        .unwrap_or("");
                    let name_b = self.latest.archetypes[b]
                        .component_names
                        .first()
                        .map(|s| s.as_str())
                        .unwrap_or("");
                    name_a.cmp(name_b)
                });
            }
        }

        indices
    }

    /// Build the flattened tree rows and a parallel vec of indices into that
    /// list that are selectable (archetype + entity rows).
    fn build_tree_rows(&self) -> (Vec<TreeRow>, Vec<usize>) {
        let arch_indices = self.filtered_sorted_archetypes();
        let mut all_rows = Vec::new();
        let mut selectable = Vec::new();

        for &arch_idx in &arch_indices {
            selectable.push(all_rows.len());
            all_rows.push(TreeRow::Archetype { arch_idx });

            if self.expanded_archetypes.contains(&arch_idx) {
                let arch = &self.latest.archetypes[arch_idx];
                if let Some(entities) = &arch.entities {
                    for (entity_row, ent) in entities.iter().enumerate() {
                        selectable.push(all_rows.len());
                        all_rows.push(TreeRow::Entity { arch_idx, entity_row });

                        if self.expanded_entities.contains(&(arch_idx, entity_row)) {
                            for (comp_idx, comp) in ent.components.iter().enumerate() {
                                selectable.push(all_rows.len());
                                all_rows.push(TreeRow::Component {
                                    arch_idx,
                                    entity_row,
                                    comp_idx,
                                });

                                if self.expanded_components.contains(&(arch_idx, entity_row, comp_idx)) {
                                    let fields = parse_debug_fields(&comp.debug_value);
                                    for field_idx in 0..fields.len() {
                                        all_rows.push(TreeRow::Field {
                                            arch_idx,
                                            entity_row,
                                            comp_idx,
                                            field_idx,
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        (all_rows, selectable)
    }

    /// Count log entries by level.
    fn log_counts(&self) -> (usize, usize, usize, usize, usize) {
        let (mut t, mut d, mut i, mut w, mut e) = (0, 0, 0, 0, 0);
        for log in &self.log_entries {
            match log.level.as_str() {
                "TRACE" => t += 1,
                "DEBUG" => d += 1,
                "INFO" => i += 1,
                "WARN" => w += 1,
                "ERROR" => e += 1,
                _ => {}
            }
        }
        (t, d, i, w, e)
    }

    /// Get filtered log entries.
    fn filtered_logs(&self) -> Vec<&LogEntry> {
        self.log_entries
            .iter()
            .filter(|e| self.log_filter.passes(&e.level))
            .collect()
    }
}

// ── Main ─────────────────────────────────────────────────────────────────

fn main() -> io::Result<()> {
    let recv_socket = UdpSocket::bind("127.0.0.1:9100")
        .expect("Failed to bind UDP port 9100 — is another dreki-telemetry running?");
    recv_socket
        .set_nonblocking(true)
        .expect("Failed to set non-blocking");

    let send_socket = UdpSocket::bind("127.0.0.1:0").expect("Failed to bind ephemeral port");
    send_socket
        .connect("127.0.0.1:9101")
        .expect("Failed to connect to port 9101");

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(send_socket);
    let mut buf = [0u8; 65536];

    loop {
        // Drain all pending datagrams.
        loop {
            match recv_socket.recv(&mut buf) {
                Ok(n) => {
                    if let Ok(snap) = serde_json::from_slice::<DiagSnapshot>(&buf[..n]) {
                        app.push_snapshot(snap);
                    }
                }
                Err(_) => break,
            }
        }

        terminal.draw(|f| ui(f, &app))?;

        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                if handle_key(&mut app, key) {
                    break;
                }
            }
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

// ── Key handling ─────────────────────────────────────────────────────────

/// Returns `true` if the app should quit.
fn handle_key(app: &mut App, key: KeyEvent) -> bool {
    // Search mode input (Overview tab only).
    if app.input_mode == InputMode::Search {
        match key.code {
            KeyCode::Esc => {
                app.input_mode = InputMode::Normal;
                app.search_query.clear();
            }
            KeyCode::Enter => {
                app.input_mode = InputMode::Normal;
                if app.search_query.is_empty() {
                    app.active_filter = None;
                } else {
                    app.active_filter = Some(app.search_query.clone());
                }
                app.search_query.clear();
                // Clamp cursor after filter change.
                let (_, selectable) = app.build_tree_rows();
                if !selectable.is_empty() && app.cursor >= selectable.len() {
                    app.cursor = selectable.len() - 1;
                }
                if selectable.is_empty() {
                    app.cursor = 0;
                }
            }
            KeyCode::Backspace => {
                app.search_query.pop();
            }
            KeyCode::Char(c) => {
                app.search_query.push(c);
            }
            _ => {}
        }
        return false;
    }

    // Normal mode.
    match key.code {
        KeyCode::Char('q') => return true,
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => return true,
        KeyCode::Char('p') => app.paused = !app.paused,

        // Tab switching with number keys.
        KeyCode::Char('1') => app.active_tab = Tab::Overview,
        KeyCode::Char('2') => app.active_tab = Tab::Systems,
        KeyCode::Char('3') => app.active_tab = Tab::Assets,
        KeyCode::Char('4') => app.active_tab = Tab::Logs,

        // Tab cycling.
        KeyCode::Tab => {
            app.active_tab = if key.modifiers.contains(KeyModifiers::SHIFT) {
                app.active_tab.prev()
            } else {
                app.active_tab.next()
            };
        }

        // Overview tab keys.
        KeyCode::Char('s') if app.active_tab == Tab::Overview => {
            app.sort_mode = app.sort_mode.next();
            let (_, selectable) = app.build_tree_rows();
            if !selectable.is_empty() && app.cursor >= selectable.len() {
                app.cursor = selectable.len() - 1;
            }
        }
        KeyCode::Char('/') if app.active_tab == Tab::Overview => {
            if app.active_filter.is_some() {
                app.active_filter = None;
                let (_, selectable) = app.build_tree_rows();
                if !selectable.is_empty() && app.cursor >= selectable.len() {
                    app.cursor = selectable.len() - 1;
                }
                if selectable.is_empty() {
                    app.cursor = 0;
                }
            } else {
                app.input_mode = InputMode::Search;
                app.search_query.clear();
            }
        }
        KeyCode::Up if app.active_tab == Tab::Overview => {
            if app.cursor > 0 {
                app.cursor -= 1;
            }
        }
        KeyCode::Down if app.active_tab == Tab::Overview => {
            let (_, selectable) = app.build_tree_rows();
            if app.cursor + 1 < selectable.len() {
                app.cursor += 1;
            }
        }
        KeyCode::Enter | KeyCode::Right if app.active_tab == Tab::Overview => {
            toggle_expand(app);
        }
        KeyCode::Left if app.active_tab == Tab::Overview => {
            collapse_or_parent(app);
        }
        KeyCode::Esc if app.active_tab == Tab::Overview => {
            app.expanded_archetypes.clear();
            app.expanded_entities.clear();
            app.expanded_components.clear();
            app.cursor = 0;
            app.send_expand_request();
        }

        // Logs tab keys.
        KeyCode::Char('l') if app.active_tab == Tab::Logs => {
            app.log_filter = app.log_filter.next();
        }
        KeyCode::Char('g') if app.active_tab == Tab::Logs => {
            app.log_auto_scroll = !app.log_auto_scroll;
        }
        KeyCode::Up if app.active_tab == Tab::Logs => {
            app.log_auto_scroll = false;
            if app.log_scroll_offset > 0 {
                app.log_scroll_offset -= 1;
            }
        }
        KeyCode::Down if app.active_tab == Tab::Logs => {
            app.log_auto_scroll = false;
            app.log_scroll_offset += 1;
        }

        _ => {}
    }
    false
}

fn toggle_expand(app: &mut App) {
    let (all_rows, selectable) = app.build_tree_rows();
    if selectable.is_empty() {
        return;
    }
    let row_idx = selectable[app.cursor];
    match &all_rows[row_idx] {
        TreeRow::Archetype { arch_idx } => {
            let arch_idx = *arch_idx;
            if app.expanded_archetypes.contains(&arch_idx) {
                app.expanded_archetypes.remove(&arch_idx);
                app.expanded_entities.retain(|(a, _)| *a != arch_idx);
                app.expanded_components.retain(|(a, _, _)| *a != arch_idx);
            } else {
                app.expanded_archetypes.insert(arch_idx);
            }
            app.send_expand_request();
        }
        TreeRow::Entity { arch_idx, entity_row } => {
            let key = (*arch_idx, *entity_row);
            if app.expanded_entities.contains(&key) {
                app.expanded_entities.remove(&key);
                app.expanded_components.retain(|(a, e, _)| !(*a == *arch_idx && *e == *entity_row));
            } else {
                app.expanded_entities.insert(key);
            }
        }
        TreeRow::Component { arch_idx, entity_row, comp_idx } => {
            let key = (*arch_idx, *entity_row, *comp_idx);
            if app.expanded_components.contains(&key) {
                app.expanded_components.remove(&key);
            } else {
                app.expanded_components.insert(key);
            }
        }
        TreeRow::Field { .. } => {}
    }
}

fn collapse_or_parent(app: &mut App) {
    let (all_rows, selectable) = app.build_tree_rows();
    if selectable.is_empty() {
        return;
    }
    let row_idx = selectable[app.cursor];
    match &all_rows[row_idx] {
        TreeRow::Archetype { arch_idx } => {
            let arch_idx = *arch_idx;
            if app.expanded_archetypes.contains(&arch_idx) {
                app.expanded_archetypes.remove(&arch_idx);
                app.expanded_entities.retain(|(a, _)| *a != arch_idx);
                app.expanded_components.retain(|(a, _, _)| *a != arch_idx);
                app.send_expand_request();
            }
        }
        TreeRow::Entity { arch_idx, entity_row } => {
            let key = (*arch_idx, *entity_row);
            if app.expanded_entities.contains(&key) {
                app.expanded_entities.remove(&key);
                app.expanded_components.retain(|(a, e, _)| !(*a == *arch_idx && *e == *entity_row));
            } else {
                // Jump to parent archetype.
                let parent_arch = *arch_idx;
                let (new_rows, new_sel) = app.build_tree_rows();
                for (si, &ri) in new_sel.iter().enumerate() {
                    if let TreeRow::Archetype { arch_idx: a } = &new_rows[ri] {
                        if *a == parent_arch {
                            app.cursor = si;
                            break;
                        }
                    }
                }
            }
        }
        TreeRow::Component { arch_idx, entity_row, comp_idx } => {
            let key = (*arch_idx, *entity_row, *comp_idx);
            if app.expanded_components.contains(&key) {
                app.expanded_components.remove(&key);
            } else {
                // Jump to parent entity.
                let (pa, pe) = (*arch_idx, *entity_row);
                let (new_rows, new_sel) = app.build_tree_rows();
                for (si, &ri) in new_sel.iter().enumerate() {
                    if let TreeRow::Entity { arch_idx: a, entity_row: e } = &new_rows[ri] {
                        if *a == pa && *e == pe {
                            app.cursor = si;
                            break;
                        }
                    }
                }
            }
        }
        TreeRow::Field { .. } => {}
    }
}

// ── UI rendering ─────────────────────────────────────────────────────────

fn ui(f: &mut ratatui::Frame, app: &App) {
    let has_search_bar = app.input_mode == InputMode::Search;
    let mut constraints = vec![
        Constraint::Length(3), // header
        Constraint::Length(1), // tab bar
        Constraint::Min(6),   // tab content
        Constraint::Length(3), // render stats
    ];
    if has_search_bar {
        constraints.push(Constraint::Length(1)); // search bar
    }
    constraints.push(Constraint::Length(1)); // help bar

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(f.area());

    draw_header(f, app, chunks[0]);
    draw_tab_bar(f, app, chunks[1]);

    // Dispatch to active tab.
    match app.active_tab {
        Tab::Overview => draw_overview_tab(f, app, chunks[2]),
        Tab::Systems => draw_systems_tab(f, app, chunks[2]),
        Tab::Assets => draw_assets_tab(f, app, chunks[2]),
        Tab::Logs => draw_logs_tab(f, app, chunks[2]),
    }

    draw_render_panel(f, app, chunks[3]);

    if has_search_bar {
        draw_search_bar(f, app, chunks[4]);
        draw_help_bar(f, app, chunks[5]);
    } else {
        draw_help_bar(f, app, chunks[4]);
    }
}

fn draw_header(f: &mut ratatui::Frame, app: &App, area: Rect) {
    let s = &app.latest;
    let status = if app.paused {
        " PAUSED "
    } else if app.connected {
        " LIVE "
    } else {
        " WAITING "
    };
    let status_color = if app.paused {
        Color::Yellow
    } else if app.connected {
        Color::Green
    } else {
        Color::DarkGray
    };

    let text = Line::from(vec![
        Span::styled(
            format!(" {} ", status),
            Style::default().bg(status_color).fg(Color::Black),
        ),
        Span::raw("  "),
        Span::styled("FPS: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("{:.1}", s.fps),
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ),
        Span::raw("  |  "),
        Span::styled("Frame: ", Style::default().fg(Color::DarkGray)),
        Span::styled(format!("{}", s.frame_count), Style::default().fg(Color::White)),
        Span::raw("  |  "),
        Span::styled("\u{0394}: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("{:.1}ms", s.delta_ms),
            Style::default().fg(Color::White),
        ),
        Span::raw("  |  "),
        Span::styled("Up: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format_uptime(s.elapsed_secs),
            Style::default().fg(Color::White),
        ),
    ]);

    let block = Block::default()
        .title(" dreki-telemetry ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    let paragraph = Paragraph::new(text).block(block);
    f.render_widget(paragraph, area);
}

fn draw_tab_bar(f: &mut ratatui::Frame, app: &App, area: Rect) {
    let mut spans = vec![Span::raw(" ")];
    for (i, tab) in Tab::ALL.iter().enumerate() {
        let num = format!(" {} ", i + 1);
        let label = format!("{} ", tab.label());
        if *tab == app.active_tab {
            spans.push(Span::styled(
                num,
                Style::default().bg(Color::Cyan).fg(Color::Black).add_modifier(Modifier::BOLD),
            ));
            spans.push(Span::styled(
                label,
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            ));
        } else {
            spans.push(Span::styled(
                num,
                Style::default().fg(Color::DarkGray),
            ));
            spans.push(Span::styled(
                label,
                Style::default().fg(Color::DarkGray),
            ));
        }
        spans.push(Span::raw("  "));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

// ── Overview Tab ─────────────────────────────────────────────────────────

fn draw_overview_tab(f: &mut ratatui::Frame, app: &App, area: Rect) {
    // Split into: sparklines, entity pool line, ECS tree
    let has_pool = app.latest.entity_pool.is_some();
    let mut constraints = vec![
        Constraint::Length(8), // sparklines
    ];
    if has_pool {
        constraints.push(Constraint::Length(1)); // entity pool line
    }
    constraints.push(Constraint::Min(4)); // ECS tree

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    draw_sparklines(f, app, chunks[0]);

    if has_pool {
        draw_entity_pool_line(f, app, chunks[1]);
        draw_ecs_panel(f, app, chunks[2]);
    } else {
        draw_ecs_panel(f, app, chunks[1]);
    }
}

fn draw_sparklines(f: &mut ratatui::Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    let fps_data: Vec<u64> = app.fps_history.iter().copied().collect();
    let (fps_min, fps_avg, fps_max) = stats(&fps_data);
    let fps_block = Block::default()
        .title(" FPS History ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));

    let inner = fps_block.inner(chunks[0]);
    f.render_widget(fps_block, chunks[0]);
    if inner.height >= 2 {
        let spark_area = Rect { height: inner.height - 1, ..inner };
        let stats_area = Rect {
            y: inner.y + inner.height - 1,
            height: 1,
            ..inner
        };
        let sparkline = Sparkline::default()
            .data(&fps_data)
            .style(Style::default().fg(Color::Green));
        f.render_widget(sparkline, spark_area);
        let stats_text = Line::from(vec![Span::styled(
            format!("min: {:.0}  avg: {:.0}  max: {:.0}", fps_min, fps_avg, fps_max),
            Style::default().fg(Color::DarkGray),
        )]);
        f.render_widget(Paragraph::new(stats_text), stats_area);
    }

    let delta_data: Vec<u64> = app.delta_history.iter().copied().collect();
    let (d_min, d_avg, d_max) = stats(&delta_data);
    let delta_block = Block::default()
        .title(" Delta Time ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));

    let inner = delta_block.inner(chunks[1]);
    f.render_widget(delta_block, chunks[1]);
    if inner.height >= 2 {
        let spark_area = Rect { height: inner.height - 1, ..inner };
        let stats_area = Rect {
            y: inner.y + inner.height - 1,
            height: 1,
            ..inner
        };
        let sparkline = Sparkline::default()
            .data(&delta_data)
            .style(Style::default().fg(Color::Yellow));
        f.render_widget(sparkline, spark_area);
        let stats_text = Line::from(vec![Span::styled(
            format!(
                "min: {:.1}ms  avg: {:.1}ms  max: {:.1}ms",
                d_min / 1000.0,
                d_avg / 1000.0,
                d_max / 1000.0
            ),
            Style::default().fg(Color::DarkGray),
        )]);
        f.render_widget(Paragraph::new(stats_text), stats_area);
    }
}

fn draw_entity_pool_line(f: &mut ratatui::Frame, app: &App, area: Rect) {
    let pool = match &app.latest.entity_pool {
        Some(p) => p,
        None => return,
    };

    let frag = pool.fragmentation_pct;
    let frag_color = if frag < 25.0 {
        Color::Green
    } else if frag < 50.0 {
        Color::Yellow
    } else {
        Color::Red
    };

    // Build a small bar showing alive/total ratio.
    let bar_width: usize = 16;
    let filled = if pool.total_slots > 0 {
        ((pool.alive_count as f32 / pool.total_slots as f32) * bar_width as f32).round() as usize
    } else {
        0
    };
    let empty = bar_width.saturating_sub(filled);
    let bar: String = format!(
        "{}{}",
        "\u{2588}".repeat(filled),
        "\u{2591}".repeat(empty)
    );

    let mut spans = vec![
        Span::styled("  Pool: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("{}", pool.total_slots),
            Style::default().fg(Color::White),
        ),
        Span::styled(" slots  Free: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("{}", pool.free_count),
            Style::default().fg(Color::White),
        ),
        Span::styled("  Frag: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("{:.0}%", frag),
            Style::default().fg(frag_color),
        ),
        Span::raw("  "),
        Span::styled(bar, Style::default().fg(frag_color)),
    ];

    if pool.spawned_this_tick > 0 || pool.despawned_this_tick > 0 {
        spans.push(Span::raw("  "));
        if pool.spawned_this_tick > 0 {
            spans.push(Span::styled(
                format!("+{}", pool.spawned_this_tick),
                Style::default().fg(Color::Green),
            ));
        }
        if pool.despawned_this_tick > 0 {
            spans.push(Span::styled(
                format!("/-{}", pool.despawned_this_tick),
                Style::default().fg(Color::Red),
            ));
        }
        spans.push(Span::styled(" this tick", Style::default().fg(Color::DarkGray)));
    }

    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn draw_ecs_panel(f: &mut ratatui::Frame, app: &App, area: Rect) {
    let mut title_parts = format!(
        " ECS  Entities: {}  Archetypes: {}  sort: {}",
        app.latest.entity_count,
        app.latest.archetype_count,
        app.sort_mode.label(),
    );
    if let Some(filter) = &app.active_filter {
        title_parts.push_str(&format!("  filter: \"{}\"", filter));
    }
    title_parts.push(' ');

    let block = Block::default()
        .title(title_parts)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let (all_rows, selectable) = app.build_tree_rows();
    if all_rows.is_empty() {
        let msg = if app.active_filter.is_some() {
            "  No matching archetypes"
        } else {
            "  No archetypes"
        };
        let p = Paragraph::new(msg).style(Style::default().fg(Color::DarkGray));
        f.render_widget(p, inner);
        return;
    }

    let visible_height = inner.height as usize;

    // Find which row index the cursor points at.
    let cursor_row_idx = if !selectable.is_empty() && app.cursor < selectable.len() {
        selectable[app.cursor]
    } else {
        0
    };

    // Scroll: keep cursor centered.
    let scroll_offset = if cursor_row_idx >= visible_height / 2 {
        let offset = cursor_row_idx - visible_height / 2;
        let max_offset = all_rows.len().saturating_sub(visible_height);
        offset.min(max_offset)
    } else {
        0
    };

    let mut lines: Vec<Line> = Vec::with_capacity(visible_height);
    for (row_i, row) in all_rows.iter().enumerate().skip(scroll_offset).take(visible_height) {
        let is_cursor = row_i == cursor_row_idx && row.is_selectable();
        let line = match row {
            TreeRow::Archetype { arch_idx } => {
                let arch = &app.latest.archetypes[*arch_idx];
                let expanded = app.expanded_archetypes.contains(arch_idx);
                let arrow = if expanded { "\u{25BC}" } else { "\u{25B6}" };
                let names = arch.component_names.join(", ");
                let entity_label = if arch.entity_count == 1 { "entity" } else { "entities" };
                let cursor_marker = if is_cursor { "> " } else { "  " };
                Line::from(vec![
                    Span::styled(
                        cursor_marker.to_string(),
                        Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!("{} ", arrow),
                        Style::default().fg(Color::Yellow),
                    ),
                    Span::styled(
                        format!("[{}]", names),
                        if is_cursor {
                            Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
                        } else {
                            Style::default().fg(Color::White)
                        },
                    ),
                    Span::raw("  "),
                    Span::styled(
                        format!("{} {}", arch.entity_count, entity_label),
                        Style::default().fg(Color::DarkGray),
                    ),
                ])
            }
            TreeRow::Entity { arch_idx, entity_row } => {
                let arch = &app.latest.archetypes[*arch_idx];
                if let Some(entities) = &arch.entities {
                    if let Some(ent) = entities.get(*entity_row) {
                        let expanded = app.expanded_entities.contains(&(*arch_idx, *entity_row));
                        let arrow = if expanded { "\u{25BC}" } else { "\u{25B6}" };
                        let cursor_marker = if is_cursor { "> " } else { "  " };
                        Line::from(vec![
                            Span::styled(
                                cursor_marker.to_string(),
                                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
                            ),
                            Span::raw("    "),
                            Span::styled(
                                format!("{} ", arrow),
                                Style::default().fg(Color::Yellow),
                            ),
                            Span::styled(
                                format!("Entity({}, gen={})", ent.id, ent.generation),
                                if is_cursor {
                                    Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
                                } else {
                                    Style::default().fg(Color::White)
                                },
                            ),
                        ])
                    } else {
                        Line::raw("")
                    }
                } else {
                    Line::raw("")
                }
            }
            TreeRow::Component { arch_idx, entity_row, comp_idx } => {
                let arch = &app.latest.archetypes[*arch_idx];
                if let Some(entities) = &arch.entities {
                    if let Some(ent) = entities.get(*entity_row) {
                        if let Some(comp) = ent.components.get(*comp_idx) {
                            let expanded = app.expanded_components.contains(&(*arch_idx, *entity_row, *comp_idx));
                            let fields = parse_debug_fields(&comp.debug_value);
                            let has_fields = !fields.is_empty();
                            let cursor_marker = if is_cursor { "> " } else { "  " };

                            if expanded && has_fields {
                                Line::from(vec![
                                    Span::styled(
                                        cursor_marker.to_string(),
                                        Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
                                    ),
                                    Span::raw("        "),
                                    Span::styled(
                                        "\u{25BC} ",
                                        Style::default().fg(Color::Yellow),
                                    ),
                                    Span::styled(
                                        comp.name.clone(),
                                        if is_cursor {
                                            Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
                                        } else {
                                            Style::default().fg(Color::Green)
                                        },
                                    ),
                                ])
                            } else if has_fields {
                                let preview = compact_preview(&comp.debug_value, 60);
                                Line::from(vec![
                                    Span::styled(
                                        cursor_marker.to_string(),
                                        Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
                                    ),
                                    Span::raw("        "),
                                    Span::styled(
                                        "\u{25B6} ",
                                        Style::default().fg(Color::Yellow),
                                    ),
                                    Span::styled(
                                        comp.name.clone(),
                                        if is_cursor {
                                            Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
                                        } else {
                                            Style::default().fg(Color::Green)
                                        },
                                    ),
                                    Span::raw("  "),
                                    Span::styled(
                                        preview,
                                        Style::default().fg(Color::DarkGray),
                                    ),
                                ])
                            } else {
                                Line::from(vec![
                                    Span::styled(
                                        cursor_marker.to_string(),
                                        Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
                                    ),
                                    Span::raw("          "),
                                    Span::styled(
                                        comp.name.clone(),
                                        if is_cursor {
                                            Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
                                        } else {
                                            Style::default().fg(Color::Green)
                                        },
                                    ),
                                    Span::styled(
                                        format!(": {}", comp.debug_value),
                                        Style::default().fg(Color::DarkGray),
                                    ),
                                ])
                            }
                        } else {
                            Line::raw("")
                        }
                    } else {
                        Line::raw("")
                    }
                } else {
                    Line::raw("")
                }
            }
            TreeRow::Field { arch_idx, entity_row, comp_idx, field_idx } => {
                let arch = &app.latest.archetypes[*arch_idx];
                if let Some(entities) = &arch.entities {
                    if let Some(ent) = entities.get(*entity_row) {
                        if let Some(comp) = ent.components.get(*comp_idx) {
                            let fields = parse_debug_fields(&comp.debug_value);
                            if let Some((name, value)) = fields.get(*field_idx) {
                                Line::from(vec![
                                    Span::raw("                "),
                                    Span::styled(
                                        format!("{}: ", name),
                                        Style::default().fg(Color::Yellow),
                                    ),
                                    Span::styled(
                                        value.clone(),
                                        Style::default().fg(Color::White),
                                    ),
                                ])
                            } else {
                                Line::raw("")
                            }
                        } else {
                            Line::raw("")
                        }
                    } else {
                        Line::raw("")
                    }
                } else {
                    Line::raw("")
                }
            }
        };
        lines.push(line);
    }

    let paragraph = Paragraph::new(lines);
    f.render_widget(paragraph, inner);
}

// ── Systems Tab ──────────────────────────────────────────────────────────

fn draw_systems_tab(f: &mut ratatui::Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(4)])
        .split(area);

    // Frame budget gauge.
    draw_frame_budget(f, app, chunks[0]);
    // Per-system timing bars.
    draw_system_timings(f, app, chunks[1]);
}

fn draw_frame_budget(f: &mut ratatui::Frame, app: &App, area: Rect) {
    let block = Block::default()
        .title(" Frame Budget (16.6ms target) ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(area);
    f.render_widget(block, area);

    if let Some(fb) = &app.latest.frame_budget {
        let total_ms = (fb.systems_us + fb.render_us) / 1000.0;
        let systems_ms = fb.systems_us / 1000.0;
        let render_ms = fb.render_us / 1000.0;
        let pct = total_ms / 16.6;

        let bar_color = if pct < 0.8 {
            Color::Green
        } else if pct <= 1.0 {
            Color::Yellow
        } else {
            Color::Red
        };

        let bar_width = inner.width.saturating_sub(2) as usize;
        let filled = ((pct.min(1.5) / 1.5) * bar_width as f64).round() as usize;
        let empty = bar_width.saturating_sub(filled);
        let bar: String = format!(
            "{}{}",
            "\u{2588}".repeat(filled),
            "\u{2591}".repeat(empty)
        );

        let text = Line::from(vec![
            Span::raw(" "),
            Span::styled(
                format!("{:.1}ms", total_ms),
                Style::default().fg(bar_color).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(" (systems: {:.1}ms | render: {:.1}ms) ", systems_ms, render_ms),
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled(bar, Style::default().fg(bar_color)),
        ]);
        f.render_widget(Paragraph::new(text), inner);
    } else {
        let text = Span::styled(
            "  Waiting for frame budget data...",
            Style::default().fg(Color::DarkGray),
        );
        f.render_widget(Paragraph::new(text), inner);
    }
}

fn draw_system_timings(f: &mut ratatui::Frame, app: &App, area: Rect) {
    let block = Block::default()
        .title(" System Timings ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let timings = match &app.latest.system_timings {
        Some(t) if !t.is_empty() => t,
        _ => {
            let text = Span::styled(
                "  No system timing data",
                Style::default().fg(Color::DarkGray),
            );
            f.render_widget(Paragraph::new(text), inner);
            return;
        }
    };

    // Sort by duration descending.
    let mut sorted: Vec<&SystemTimingInfo> = timings.iter().collect();
    sorted.sort_by(|a, b| b.duration_us.partial_cmp(&a.duration_us).unwrap_or(std::cmp::Ordering::Equal));

    let max_dur = sorted.first().map(|t| t.duration_us).unwrap_or(1.0).max(1.0);
    let name_col_width = sorted.iter().map(|t| t.name.len()).max().unwrap_or(10).min(30);
    let bar_max_width = inner.width.saturating_sub(name_col_width as u16 + 12) as usize;

    let visible = inner.height as usize;
    let mut lines: Vec<Line> = Vec::with_capacity(visible);

    for timing in sorted.iter().take(visible) {
        let ms = timing.duration_us / 1000.0;
        let bar_color = if ms < 2.0 {
            Color::Green
        } else if ms < 5.0 {
            Color::Yellow
        } else {
            Color::Red
        };

        let bar_len = ((timing.duration_us / max_dur) * bar_max_width as f64).round() as usize;
        let bar: String = "\u{2588}".repeat(bar_len.max(1));

        let padded_name = format!("  {:width$}", timing.name, width = name_col_width);
        lines.push(Line::from(vec![
            Span::styled(padded_name, Style::default().fg(Color::White)),
            Span::styled(
                format!(" {:>6.1}ms ", ms),
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled(bar, Style::default().fg(bar_color)),
        ]));
    }

    f.render_widget(Paragraph::new(lines), inner);
}

// ── Assets Tab ───────────────────────────────────────────────────────────

fn draw_assets_tab(f: &mut ratatui::Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(6), Constraint::Min(4)])
        .split(area);

    draw_watched_assets(f, app, chunks[0]);
    draw_reload_log(f, app, chunks[1]);
}

fn draw_watched_assets(f: &mut ratatui::Frame, app: &App, area: Rect) {
    let assets = &app.latest.assets;
    let watched_count = assets.as_ref().map(|a| a.watched_count).unwrap_or(0);

    let block = Block::default()
        .title(format!(" Watched Assets ({}) ", watched_count))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let Some(assets) = assets else {
        let text = Span::styled(
            "  Waiting for asset data...",
            Style::default().fg(Color::DarkGray),
        );
        f.render_widget(Paragraph::new(text), inner);
        return;
    };

    let mut lines: Vec<Line> = Vec::new();

    // Watcher status line.
    let (status_label, status_color) = if assets.watcher_active {
        ("ACTIVE", Color::Green)
    } else {
        ("INACTIVE", Color::Red)
    };
    lines.push(Line::from(vec![
        Span::styled("  Watcher: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!(" {} ", status_label),
            Style::default().bg(status_color).fg(Color::Black),
        ),
        Span::styled("   Pending: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("{}", assets.pending_count),
            Style::default().fg(Color::White),
        ),
    ]));

    // Group watched files by kind.
    let mut by_kind: std::collections::HashMap<&str, Vec<&str>> = std::collections::HashMap::new();
    for (kind, filename) in &assets.watched_files {
        by_kind.entry(kind.as_str()).or_default().push(filename.as_str());
    }
    let mut kinds: Vec<&str> = by_kind.keys().copied().collect();
    kinds.sort();

    for kind in kinds {
        let files = &by_kind[kind];
        lines.push(Line::from(vec![
            Span::styled(
                format!("  {:12}", kind),
                Style::default().fg(Color::Yellow),
            ),
            Span::styled(
                files.join(", "),
                Style::default().fg(Color::White),
            ),
        ]));
    }

    f.render_widget(Paragraph::new(lines), inner);
}

fn draw_reload_log(f: &mut ratatui::Frame, app: &App, area: Rect) {
    let block = Block::default()
        .title(format!(" Reload Log ({}) ", app.reload_log.len()))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(area);
    f.render_widget(block, area);

    if app.reload_log.is_empty() {
        let text = Span::styled(
            "  No reloads yet",
            Style::default().fg(Color::DarkGray),
        );
        f.render_widget(Paragraph::new(text), inner);
        return;
    }

    let visible = inner.height as usize;
    let start = app.reload_log.len().saturating_sub(visible);
    let mut lines: Vec<Line> = Vec::with_capacity(visible);

    for ev in app.reload_log.iter().skip(start) {
        let (badge, badge_color) = if ev.success {
            (" OK ", Color::Green)
        } else {
            ("FAIL", Color::Red)
        };

        let mut spans = vec![
            Span::styled(
                format!("  [{:>6.1}s]  ", ev.timestamp_secs),
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled(
                format!("{} ", badge),
                Style::default().fg(badge_color).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("{} ({})", ev.path, ev.kind),
                Style::default().fg(Color::White),
            ),
        ];

        if let Some(err) = &ev.error {
            spans.push(Span::styled(
                format!(" - \"{}\"", err),
                Style::default().fg(Color::Red),
            ));
        }

        lines.push(Line::from(spans));
    }

    f.render_widget(Paragraph::new(lines), inner);
}

// ── Logs Tab ─────────────────────────────────────────────────────────────

fn draw_logs_tab(f: &mut ratatui::Frame, app: &App, area: Rect) {
    let (t, d, i, w, e) = app.log_counts();
    let scroll_label = if app.log_auto_scroll { "auto" } else { "manual" };

    let block = Block::default()
        .title(format!(
            " Logs [{}]  T:{} D:{} I:{} W:{} E:{}  scroll:{} ",
            app.log_filter.label(), t, d, i, w, e, scroll_label,
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let filtered = app.filtered_logs();
    if filtered.is_empty() {
        let text = Span::styled(
            "  No log messages",
            Style::default().fg(Color::DarkGray),
        );
        f.render_widget(Paragraph::new(text), inner);
        return;
    }

    let visible = inner.height as usize;
    let total = filtered.len();

    let offset = if app.log_auto_scroll {
        total.saturating_sub(visible)
    } else {
        app.log_scroll_offset.min(total.saturating_sub(visible))
    };

    let mut lines: Vec<Line> = Vec::with_capacity(visible);
    for entry in filtered.iter().skip(offset).take(visible) {
        let level_color = match entry.level.as_str() {
            "TRACE" => Color::DarkGray,
            "DEBUG" => Color::Gray,
            "INFO" => Color::Cyan,
            "WARN" => Color::Yellow,
            "ERROR" => Color::Red,
            _ => Color::White,
        };

        lines.push(Line::from(vec![
            Span::styled(
                format!("  [{:>6.1}s] ", entry.timestamp_secs),
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled(
                format!("{:<5} ", entry.level),
                Style::default().fg(level_color).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                entry.message.clone(),
                Style::default().fg(Color::White),
            ),
        ]));
    }

    f.render_widget(Paragraph::new(lines), inner);
}

// ── Render stats + help bar ──────────────────────────────────────────────

fn draw_render_panel(f: &mut ratatui::Frame, app: &App, area: Rect) {
    let block = Block::default()
        .title(" Render ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));

    let text = if let Some(r) = &app.latest.render {
        Line::from(vec![
            Span::styled("  Draw calls: ", Style::default().fg(Color::DarkGray)),
            Span::styled(format!("{}", r.draw_calls), Style::default().fg(Color::White)),
            Span::raw("  |  "),
            Span::styled("Vertices: ", Style::default().fg(Color::DarkGray)),
            Span::styled(format!("{}", r.vertices), Style::default().fg(Color::White)),
            Span::raw("  |  "),
            Span::styled("Textures: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("{}", r.textures_loaded),
                Style::default().fg(Color::White),
            ),
        ])
    } else {
        Line::from(Span::styled(
            "  No render stats (diagnostics not sending render data)",
            Style::default().fg(Color::DarkGray),
        ))
    };

    let paragraph = Paragraph::new(text).block(block);
    f.render_widget(paragraph, area);
}

fn draw_search_bar(f: &mut ratatui::Frame, app: &App, area: Rect) {
    let line = Line::from(vec![
        Span::styled(" /", Style::default().fg(Color::Cyan)),
        Span::styled(
            format!("{}_", app.search_query),
            Style::default().fg(Color::White),
        ),
    ]);
    f.render_widget(Paragraph::new(line), area);
}

fn draw_help_bar(f: &mut ratatui::Frame, app: &App, area: Rect) {
    let mut spans = vec![
        Span::styled(" [1-4]", Style::default().fg(Color::Cyan)),
        Span::raw(" tab  "),
        Span::styled("[Tab]", Style::default().fg(Color::Cyan)),
        Span::raw(" next  "),
    ];

    match app.active_tab {
        Tab::Overview => {
            spans.push(Span::styled("[\u{2191}\u{2193}]", Style::default().fg(Color::Cyan)));
            spans.push(Span::raw(" navigate  "));
            spans.push(Span::styled("[Enter/\u{2192}]", Style::default().fg(Color::Cyan)));
            spans.push(Span::raw(" expand  "));
            spans.push(Span::styled("[\u{2190}]", Style::default().fg(Color::Cyan)));
            spans.push(Span::raw(" collapse  "));
            if app.active_filter.is_some() {
                spans.push(Span::styled("[/]", Style::default().fg(Color::Cyan)));
                spans.push(Span::raw(" clear filter  "));
            } else {
                spans.push(Span::styled("[/]", Style::default().fg(Color::Cyan)));
                spans.push(Span::raw(" search  "));
            }
            spans.push(Span::styled("[s]", Style::default().fg(Color::Cyan)));
            spans.push(Span::raw(" sort  "));
        }
        Tab::Systems => {
            // No special keys for systems tab currently.
        }
        Tab::Assets => {
            // No special keys for assets tab currently.
        }
        Tab::Logs => {
            spans.push(Span::styled("[l]", Style::default().fg(Color::Cyan)));
            spans.push(Span::raw(" filter  "));
            spans.push(Span::styled("[g]", Style::default().fg(Color::Cyan)));
            spans.push(Span::raw(" auto-scroll  "));
            spans.push(Span::styled("[\u{2191}\u{2193}]", Style::default().fg(Color::Cyan)));
            spans.push(Span::raw(" scroll  "));
        }
    }

    spans.push(Span::styled("[p]", Style::default().fg(Color::Cyan)));
    spans.push(Span::raw(" pause  "));
    spans.push(Span::styled("[q]", Style::default().fg(Color::Cyan)));
    spans.push(Span::raw(" quit"));

    let help = Line::from(spans);
    f.render_widget(Paragraph::new(help), area);
}

// ── Helpers ──────────────────────────────────────────────────────────────

fn stats(data: &[u64]) -> (f64, f64, f64) {
    if data.is_empty() {
        return (0.0, 0.0, 0.0);
    }
    let min = *data.iter().min().unwrap() as f64;
    let max = *data.iter().max().unwrap() as f64;
    let avg = data.iter().sum::<u64>() as f64 / data.len() as f64;
    (min, avg, max)
}

/// Parse a Rust Debug string like `TypeName { field1: val1, field2: val2 }` into
/// a vec of `(field_name, value)` pairs. Returns empty vec if the string isn't a
/// named-field struct (e.g. tuple structs, primitives, enums).
fn parse_debug_fields(debug_str: &str) -> Vec<(String, String)> {
    // Find the opening brace after the type name.
    let Some(brace_start) = debug_str.find('{') else {
        return Vec::new();
    };
    // Must end with '}'.
    let trimmed = debug_str.trim();
    if !trimmed.ends_with('}') {
        return Vec::new();
    }
    let inner = &trimmed[brace_start + 1..trimmed.len() - 1].trim();
    if inner.is_empty() {
        return Vec::new();
    }

    // Split by top-level commas (respecting nested braces/parens/brackets).
    let mut fields = Vec::new();
    let mut depth = 0i32;
    let mut start = 0;
    let bytes = inner.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'{' | b'(' | b'[' => depth += 1,
            b'}' | b')' | b']' => depth -= 1,
            b',' if depth == 0 => {
                let segment = inner[start..i].trim();
                if let Some(field) = parse_one_field(segment) {
                    fields.push(field);
                }
                start = i + 1;
            }
            _ => {}
        }
    }
    // Last segment.
    let segment = inner[start..].trim();
    if !segment.is_empty() {
        if let Some(field) = parse_one_field(segment) {
            fields.push(field);
        }
    }

    fields
}

/// Parse `"field_name: value"` into `(field_name, value)`.
fn parse_one_field(s: &str) -> Option<(String, String)> {
    // Find the first colon that's not inside nested structures.
    let mut depth = 0i32;
    for (i, b) in s.bytes().enumerate() {
        match b {
            b'{' | b'(' | b'[' => depth += 1,
            b'}' | b')' | b']' => depth -= 1,
            b':' if depth == 0 => {
                let name = s[..i].trim();
                let value = s[i + 1..].trim();
                if !name.is_empty() && !name.contains(' ') {
                    return Some((name.to_string(), value.to_string()));
                }
                return None;
            }
            _ => {}
        }
    }
    None
}

/// Create a compact preview of a debug value, truncated to `max_len` chars.
fn compact_preview(debug_str: &str, max_len: usize) -> String {
    // Strip the outer type name if present (show just `{ ... }`).
    let preview = if let Some(brace) = debug_str.find('{') {
        &debug_str[brace..]
    } else {
        debug_str
    };
    if preview.len() <= max_len {
        preview.to_string()
    } else {
        format!("{}\u{2026}", &preview[..max_len])
    }
}

fn format_uptime(secs: f32) -> String {
    let total = secs as u64;
    let h = total / 3600;
    let m = (total % 3600) / 60;
    let s = total % 60;
    if h > 0 {
        format!("{}h{}m{}s", h, m, s)
    } else if m > 0 {
        format!("{}m{}s", m, s)
    } else {
        format!("{:.1}s", secs)
    }
}
