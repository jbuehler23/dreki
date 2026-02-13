//! Diagnostics sender — ships real-time metrics to `kera-telemetry` over UDP.
//!
//! Enabled by the `diagnostics` feature flag. When active, a [`DiagSender`]
//! resource is inserted into the world and [`send_diagnostics`] is called once
//! per frame (throttled to 10 Hz) to serialize a JSON snapshot and send it over
//! UDP to `127.0.0.1:9100`.
//!
//! A second channel on port 9101 receives inspection requests from the TUI
//! (e.g. "send entity details for archetype index N").

use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::net::UdpSocket;
use std::sync::Mutex;
use std::time::Instant;

use serde::Serialize;

use crate::asset::AssetServer;
use crate::ecs::world::World;
use crate::time::Time;

// ── DiagSender ───────────────────────────────────────────────────────────

/// Resource that owns the outbound UDP socket and throttling state.
pub struct DiagSender {
    /// Socket for sending metrics datagrams (game → TUI, port 9100).
    socket: UdpSocket,
    /// Socket for receiving inspection requests (TUI → game, port 9101).
    request_socket: UdpSocket,
    /// Last time a datagram was sent (for 10 Hz throttle).
    last_send: Instant,
    /// Currently-expanded archetype indices (set by TUI request).
    expanded_archetypes: Vec<usize>,
}

impl DiagSender {
    /// Create a new sender. Binds an ephemeral port for sending and port 9101
    /// for receiving requests.
    pub fn new() -> Option<Self> {
        let socket = UdpSocket::bind("127.0.0.1:0").ok()?;
        socket.connect("127.0.0.1:9100").ok()?;
        socket.set_nonblocking(true).ok()?;

        let request_socket = UdpSocket::bind("127.0.0.1:9101").ok()?;
        request_socket.set_nonblocking(true).ok()?;

        Some(Self {
            socket,
            request_socket,
            last_send: Instant::now() - std::time::Duration::from_secs(1), // send immediately on first frame
            expanded_archetypes: Vec::new(),
        })
    }

    /// Check for incoming inspection requests (non-blocking).
    fn process_requests(&mut self) {
        let mut buf = [0u8; 4096];
        while let Ok(n) = self.request_socket.recv(&mut buf) {
            if let Ok(req) = serde_json::from_slice::<InspectRequest>(&buf[..n]) {
                self.expanded_archetypes = req.expanded_archetypes;
            }
        }
    }
}

/// An inspection request from the TUI.
#[derive(serde::Deserialize)]
struct InspectRequest {
    expanded_archetypes: Vec<usize>,
}

// ── Snapshot types (wire format) ────────────────────────────────────────

#[derive(Serialize)]
struct DiagSnapshot {
    fps: f32,
    delta_ms: f32,
    frame_count: u64,
    elapsed_secs: f32,
    entity_count: usize,
    archetype_count: usize,
    archetypes: Vec<ArchetypeInfo>,
    render: Option<RenderStatsSnapshot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system_timings: Option<Vec<SystemTimingSnapshot>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    frame_budget: Option<FrameBudgetSnapshot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    entity_pool: Option<EntityPoolSnapshot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    assets: Option<AssetSnapshot>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    logs: Vec<LogEntrySnapshot>,
}

#[derive(Serialize)]
struct ArchetypeInfo {
    entity_count: usize,
    component_names: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    entities: Option<Vec<EntityInfo>>,
}

#[derive(Serialize)]
struct EntityInfo {
    id: u32,
    generation: u32,
    components: Vec<ComponentInfo>,
}

#[derive(Serialize)]
struct ComponentInfo {
    name: String,
    debug_value: String,
}

#[derive(Serialize)]
struct RenderStatsSnapshot {
    draw_calls: u32,
    vertices: u32,
    textures_loaded: u32,
}

#[derive(Serialize)]
struct SystemTimingSnapshot {
    name: String,
    duration_us: f64,
}

#[derive(Serialize)]
struct FrameBudgetSnapshot {
    systems_us: f64,
    render_us: f64,
}

#[derive(Serialize)]
struct EntityPoolSnapshot {
    total_slots: u32,
    free_count: usize,
    alive_count: usize,
    spawned_this_tick: u32,
    despawned_this_tick: u32,
    fragmentation_pct: f32,
}

#[derive(Serialize)]
struct AssetSnapshot {
    watched_count: usize,
    watcher_active: bool,
    pending_count: usize,
    watched_files: Vec<(String, String)>,
    reload_events: Vec<ReloadEventWire>,
}

#[derive(Serialize)]
struct ReloadEventWire {
    timestamp_secs: f32,
    path: String,
    kind: String,
    success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Serialize)]
struct LogEntrySnapshot {
    level: String,
    target: String,
    message: String,
    timestamp_secs: f32,
}

// ── Internal snapshot types (from World/AssetServer) ────────────────────

pub(crate) struct ArchetypeSnapshot {
    pub entity_count: usize,
    pub component_names: Vec<String>,
    pub entities: Option<Vec<EntitySnapshot>>,
}

pub(crate) struct EntitySnapshot {
    pub id: u32,
    pub generation: u32,
    pub components: Vec<ComponentSnapshot>,
}

pub(crate) struct ComponentSnapshot {
    pub name: String,
    pub debug_value: String,
}

/// Entity pool statistics gathered by World::diagnostics_entity_stats().
pub(crate) struct EntityPoolStats {
    pub total_slots: u32,
    pub free_count: usize,
    pub alive_count: usize,
    pub spawned_this_tick: u32,
    pub despawned_this_tick: u32,
}

/// Asset diagnostics snapshot gathered by AssetServer::diagnostics_snapshot().
pub(crate) struct AssetDiagSnapshot {
    pub watched_count: usize,
    pub watcher_active: bool,
    pub pending_count: usize,
    pub watched_files: Vec<(String, String)>,
    pub reload_events: Vec<ReloadEventSnapshot>,
}

pub(crate) struct ReloadEventSnapshot {
    pub timestamp_secs: f32,
    pub path: String,
    pub kind: String,
    pub success: bool,
    pub error: Option<String>,
}

// ── Resources ───────────────────────────────────────────────────────────

/// Per-frame render statistics, populated by the render pipeline.
pub struct RenderStats {
    pub draw_calls: u32,
    pub vertices: u32,
    pub textures_loaded: u32,
}

impl RenderStats {
    pub fn new() -> Self {
        Self {
            draw_calls: 0,
            vertices: 0,
            textures_loaded: 0,
        }
    }
}

/// Per-frame budget: how long systems and render took.
pub struct FrameBudget {
    pub systems_us: f64,
    pub render_us: f64,
}

/// Per-system timings from the most recent frame.
pub(crate) struct SystemTimings(pub Vec<crate::ecs::system::SystemTiming>);

// ── ComponentRegistry ────────────────────────────────────────────────────

/// Maps `TypeId` to a debug-formatter function so component values can be
/// printed in the diagnostics TUI.
pub struct ComponentRegistry {
    formatters: HashMap<TypeId, fn(&dyn Any) -> String>,
}

impl ComponentRegistry {
    pub fn new() -> Self {
        Self {
            formatters: HashMap::new(),
        }
    }

    /// Register a component type for debug formatting.
    pub fn register<T: std::fmt::Debug + 'static>(&mut self) {
        self.formatters.insert(TypeId::of::<T>(), |any| {
            if let Some(val) = any.downcast_ref::<T>() {
                format!("{:?}", val)
            } else {
                "<downcast failed>".to_string()
            }
        });
    }

    /// Format a component value, or return `"<opaque>"` if unregistered.
    pub(crate) fn format(&self, type_id: &TypeId, value: &dyn Any) -> String {
        if let Some(fmt) = self.formatters.get(type_id) {
            fmt(value)
        } else {
            "<opaque>".to_string()
        }
    }
}

// ── Log Capture ──────────────────────────────────────────────────────────

/// A captured log message.
struct CapturedLog {
    level: log::Level,
    target: String,
    message: String,
    timestamp_secs: f32,
}

/// Ring buffer for captured logs (capped at 500).
struct LogRing {
    entries: Vec<CapturedLog>,
}

impl LogRing {
    fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    fn push(&mut self, entry: CapturedLog) {
        if self.entries.len() >= 500 {
            self.entries.remove(0);
        }
        self.entries.push(entry);
    }

    fn drain(&mut self, max: usize) -> Vec<CapturedLog> {
        let n = self.entries.len().min(max);
        self.entries.drain(..n).collect()
    }
}

static LOG_RING: Mutex<Option<LogRing>> = Mutex::new(None);
static LOG_START: Mutex<Option<Instant>> = Mutex::new(None);

/// A logger that captures messages to the ring buffer AND delegates to
/// env_logger for stderr output.
struct DiagLogger {
    inner: env_logger::Logger,
}

impl log::Log for DiagLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        self.inner.enabled(metadata)
            || metadata.level() <= log::Level::Info
    }

    fn log(&self, record: &log::Record) {
        // Delegate to env_logger for stderr output.
        if self.inner.enabled(record.metadata()) {
            self.inner.log(record);
        }

        // Capture to ring buffer.
        let timestamp_secs = LOG_START
            .lock()
            .ok()
            .and_then(|g| g.map(|s| s.elapsed().as_secs_f32()))
            .unwrap_or(0.0);
        let entry = CapturedLog {
            level: record.level(),
            target: record.target().to_string(),
            message: format!("{}", record.args()),
            timestamp_secs,
        };
        if let Ok(mut guard) = LOG_RING.lock() {
            if let Some(ring) = guard.as_mut() {
                ring.push(entry);
            }
        }
    }

    fn flush(&self) {
        self.inner.flush();
    }
}

static DIAG_LOGGER: std::sync::OnceLock<DiagLogger> = std::sync::OnceLock::new();

/// Initialize the diagnostics logger. Captures log messages into a ring buffer
/// and delegates to env_logger for stderr output.
///
/// Call this early (before any log messages) to capture everything.
pub fn init_logger() {
    // Initialize statics.
    {
        let mut ring = LOG_RING.lock().unwrap();
        *ring = Some(LogRing::new());
    }
    {
        let mut start = LOG_START.lock().unwrap();
        *start = Some(Instant::now());
    }

    let inner = env_logger::Builder::new()
        .parse_default_env()
        .build();
    let max_level = inner.filter();

    let logger = DIAG_LOGGER.get_or_init(|| DiagLogger { inner });

    if log::set_logger(logger).is_err() {
        eprintln!("[kera] Warning: a logger is already set. Log capture disabled.");
        return;
    }
    log::set_max_level(max_level.max(log::LevelFilter::Info));
}

/// Drain up to `max` captured log entries from the ring buffer.
pub(crate) fn drain_captured_logs(max: usize) -> Vec<(String, String, String, f32)> {
    let mut guard = match LOG_RING.lock() {
        Ok(g) => g,
        Err(_) => return Vec::new(),
    };
    let Some(ring) = guard.as_mut() else {
        return Vec::new();
    };
    ring.drain(max)
        .into_iter()
        .map(|e| (e.level.to_string(), e.target, e.message, e.timestamp_secs))
        .collect()
}

// ── send_diagnostics ─────────────────────────────────────────────────────

/// Called once per frame. Throttled to 10 Hz internally.
pub fn send_diagnostics(world: &mut World) {
    // Extract sender to avoid borrow conflict.
    let Some(mut sender) = world.resource_remove::<DiagSender>() else {
        return;
    };

    // Process any incoming inspection requests.
    sender.process_requests();

    // Throttle to 10 Hz.
    let now = Instant::now();
    if now.duration_since(sender.last_send).as_millis() < 100 {
        world.insert_resource(sender);
        return;
    }
    sender.last_send = now;

    // Gather time stats.
    let (fps, delta_ms, frame_count, elapsed_secs) =
        if let Some(time) = world.get_resource::<Time>() {
            (
                time.fps(),
                time.delta().as_secs_f32() * 1000.0,
                time.frame_count(),
                time.elapsed_secs(),
            )
        } else {
            (0.0, 0.0, 0, 0.0)
        };

    // Gather ECS stats.
    let expanded = &sender.expanded_archetypes;
    let registry = world.resource_remove::<ComponentRegistry>();
    let (entity_count, archetype_count, arch_snapshots) =
        world.diagnostics_snapshot(expanded, registry.as_ref());
    if let Some(reg) = registry {
        world.insert_resource(reg);
    }
    let archetypes: Vec<ArchetypeInfo> = arch_snapshots
        .into_iter()
        .map(|a| ArchetypeInfo {
            entity_count: a.entity_count,
            component_names: a.component_names,
            entities: a.entities.map(|ents| {
                ents.into_iter()
                    .map(|e| EntityInfo {
                        id: e.id,
                        generation: e.generation,
                        components: e
                            .components
                            .into_iter()
                            .map(|c| ComponentInfo {
                                name: c.name,
                                debug_value: c.debug_value,
                            })
                            .collect(),
                    })
                    .collect()
            }),
        })
        .collect();

    // Gather render stats.
    let render = world.get_resource::<RenderStats>().map(|r| RenderStatsSnapshot {
        draw_calls: r.draw_calls,
        vertices: r.vertices,
        textures_loaded: r.textures_loaded,
    });

    // Gather system timings.
    let system_timings = world.resource_remove::<SystemTimings>().map(|st| {
        st.0.into_iter()
            .map(|t| SystemTimingSnapshot {
                name: t.name,
                duration_us: t.duration_us,
            })
            .collect()
    });

    // Gather frame budget.
    let frame_budget = world.resource_remove::<FrameBudget>().map(|fb| {
        FrameBudgetSnapshot {
            systems_us: fb.systems_us,
            render_us: fb.render_us,
        }
    });

    // Gather entity pool stats.
    let pool_stats = world.diagnostics_entity_stats();
    let frag_pct = if pool_stats.total_slots > 0 {
        (pool_stats.free_count as f32 / pool_stats.total_slots as f32) * 100.0
    } else {
        0.0
    };
    let entity_pool = Some(EntityPoolSnapshot {
        total_slots: pool_stats.total_slots,
        free_count: pool_stats.free_count,
        alive_count: pool_stats.alive_count,
        spawned_this_tick: pool_stats.spawned_this_tick,
        despawned_this_tick: pool_stats.despawned_this_tick,
        fragmentation_pct: frag_pct,
    });

    // Gather asset stats.
    let assets = {
        let snap = world
            .resource_remove::<AssetServer>()
            .map(|mut server| {
                let snap = server.diagnostics_snapshot();
                world.insert_resource(server);
                snap
            });
        snap.map(|s| AssetSnapshot {
            watched_count: s.watched_count,
            watcher_active: s.watcher_active,
            pending_count: s.pending_count,
            watched_files: s.watched_files,
            reload_events: s
                .reload_events
                .into_iter()
                .map(|e| ReloadEventWire {
                    timestamp_secs: e.timestamp_secs,
                    path: e.path,
                    kind: e.kind,
                    success: e.success,
                    error: e.error,
                })
                .collect(),
        })
    };

    // Drain captured logs (up to 50 per tick).
    let log_entries = drain_captured_logs(50);
    let logs: Vec<LogEntrySnapshot> = log_entries
        .into_iter()
        .map(|(level, target, message, ts)| LogEntrySnapshot {
            level,
            target,
            message,
            timestamp_secs: ts,
        })
        .collect();

    let snapshot = DiagSnapshot {
        fps,
        delta_ms,
        frame_count,
        elapsed_secs,
        entity_count,
        archetype_count,
        archetypes,
        render,
        system_timings,
        frame_budget,
        entity_pool,
        assets,
        logs,
    };

    // Serialize and send (errors silently ignored — fire-and-forget).
    if let Ok(json) = serde_json::to_vec(&snapshot) {
        let _ = sender.socket.send(&json);
    }

    world.insert_resource(sender);
}
