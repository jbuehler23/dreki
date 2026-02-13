//! # Asset Hot-Reload
//!
//! The asset system watches files on disk for changes and reloads them at
//! runtime without restarting the application. When a texture PNG is edited in
//! an image editor, or a shader `.wgsl` file is saved in a text editor, the
//! engine detects the change and swaps in the new data — all while the game
//! continues running.
//!
//! ## How It Works
//!
//! ```text
//! ┌────────────────────────────────────────────────────────┐
//! │  AssetServer (resource in World)                       │
//! │                                                        │
//! │  watcher ──► background thread (notify crate)          │
//! │              watches registered file paths              │
//! │              sends events over mpsc channel             │
//! │                                                        │
//! │  rx ◄──────── receives filesystem events               │
//! │                                                        │
//! │  watched_paths ── maps path → AssetKind                │
//! │  pending_reloads ── debounce buffer (path → timestamp) │
//! └────────────────────────────────────────────────────────┘
//!
//! Per-frame: process_asset_reloads(world)
//!   1. Poll: drain rx into pending_reloads
//!   2. Debounce: only act on entries older than 100ms
//!   3. Dispatch: reload by asset kind (texture, shader)
//! ```
//!
//! ## Debounce
//!
//! Text editors and image editors often perform *atomic saves*: write to a
//! temporary file, then rename over the original. This generates multiple
//! filesystem events (create, modify, rename) in rapid succession. Without
//! debouncing, each event would trigger a separate reload — wasteful and
//! sometimes incorrect (the file may be half-written).
//!
//! The debounce buffer collects events per path and waits 100ms of quiet time
//! before triggering a reload. Repeated events for the same path just reset
//! the timer. This ensures one burst of saves → exactly one reload.
//!
//! ## Handle Stability
//!
//! Handles (`TextureHandle`, `TextureHandle3d`) are indices into a `Vec`.
//! On reload, the *data* at that index is replaced — the handle value stays
//! the same. Any component holding a handle automatically sees the new data
//! next frame. No reference counting or invalidation needed.
//!
//! ## Graceful Degradation
//!
//! If the filesystem watcher fails to initialize (e.g., inotify limit
//! reached), the `AssetServer` still works — assets load normally, they just
//! won't hot-reload. Errors are logged, not panicked.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::Mutex;
use std::time::Instant;

use notify::{RecommendedWatcher, RecursiveMode, Watcher};

use crate::ecs::World;

/// The debounce window. Events within this duration of each other are collapsed
/// into a single reload.
const DEBOUNCE_DURATION: std::time::Duration = std::time::Duration::from_millis(100);

/// A record of one asset reload attempt (diagnostics only).
#[cfg(feature = "diagnostics")]
#[derive(Clone)]
pub(crate) struct ReloadEvent {
    pub timestamp_secs: f32,
    pub path: String,
    pub kind: String,
    pub success: bool,
    pub error: Option<String>,
}

/// What kind of asset a watched path corresponds to.
#[derive(Debug, Clone)]
pub(crate) enum AssetKind {
    /// A 2D texture at a specific handle index.
    #[cfg(feature = "render2d")]
    Texture2d(crate::render2d::TextureHandle),
    /// A 3D texture at a specific handle index.
    #[cfg(feature = "render3d")]
    Texture3d(crate::render3d::TextureHandle3d),
    /// The 2D sprite shader.
    #[cfg(feature = "render2d")]
    Shader2d,
    /// The 3D PBR shader.
    #[cfg(feature = "render3d")]
    Shader3d,
}

/// The asset server manages filesystem watching and hot-reload dispatch.
///
/// Stored as a resource in the [`World`]. Created by [`DefaultPlugins`](crate::app::DefaultPlugins).
pub struct AssetServer {
    /// The filesystem watcher. `None` if initialization failed.
    watcher: Option<RecommendedWatcher>,
    /// Receives filesystem events from the watcher's background thread.
    /// Wrapped in `Mutex` to satisfy `Sync` (required by World resources).
    /// Only accessed from the main thread via `poll()`, so contention is zero.
    rx: Mutex<mpsc::Receiver<Result<notify::Event, notify::Error>>>,
    /// Maps absolute file paths to their asset kind, so we know what to reload.
    watched_paths: HashMap<PathBuf, AssetKind>,
    /// Debounce buffer: path → (asset kind, timestamp of last event).
    pending_reloads: HashMap<PathBuf, (AssetKind, Instant)>,
    /// Set to true if the receiver has disconnected (log once, then stop polling).
    rx_disconnected: bool,
    /// Log of reload events (diagnostics only).
    #[cfg(feature = "diagnostics")]
    reload_log: Vec<ReloadEvent>,
}

impl AssetServer {
    /// Create a new asset server. Starts the filesystem watcher.
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();

        let watcher = notify::recommended_watcher(move |res| {
            // Send the event to the main thread. Ignore send errors (receiver dropped).
            let _ = tx.send(res);
        });

        let watcher = match watcher {
            Ok(w) => Some(w),
            Err(e) => {
                log::warn!("Failed to create file watcher: {e}. Hot-reload disabled.");
                None
            }
        };

        Self {
            watcher,
            rx: Mutex::new(rx),
            watched_paths: HashMap::new(),
            pending_reloads: HashMap::new(),
            rx_disconnected: false,
            #[cfg(feature = "diagnostics")]
            reload_log: Vec::new(),
        }
    }

    /// Register a file path for watching. The `kind` determines what reload
    /// action to take when the file changes.
    pub(crate) fn watch(&mut self, path: impl Into<PathBuf>, kind: AssetKind) {
        let path = path.into();

        // Canonicalize so we match events correctly.
        let canonical = match path.canonicalize() {
            Ok(p) => p,
            Err(e) => {
                log::warn!("Cannot watch '{}': {e}", path.display());
                return;
            }
        };

        if let Some(watcher) = &mut self.watcher {
            if let Err(e) = watcher.watch(&canonical, RecursiveMode::NonRecursive) {
                log::warn!("Failed to watch '{}': {e}", canonical.display());
                return;
            }
        }

        self.watched_paths.insert(canonical, kind);
    }

    /// Drain filesystem events from the receiver into the debounce buffer.
    fn poll(&mut self) {
        if self.rx_disconnected {
            return;
        }

        let rx = self.rx.get_mut().expect("AssetServer rx mutex poisoned");

        loop {
            match rx.try_recv() {
                Ok(Ok(event)) => {
                    // We care about modify and create events (atomic saves appear as create).
                    use notify::EventKind;
                    match event.kind {
                        EventKind::Modify(_) | EventKind::Create(_) => {
                            for path in &event.paths {
                                // Canonicalize the event path to match our watched_paths keys.
                                let canonical = path.canonicalize().unwrap_or_else(|_| path.clone());
                                if let Some(kind) = self.watched_paths.get(&canonical) {
                                    self.pending_reloads
                                        .insert(canonical, (kind.clone(), Instant::now()));
                                }
                            }
                        }
                        _ => {}
                    }
                }
                Ok(Err(e)) => {
                    log::warn!("File watcher error: {e}");
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    log::warn!("File watcher disconnected. Hot-reload disabled.");
                    self.rx_disconnected = true;
                    break;
                }
            }
        }
    }

    /// Return entries that have been quiet for at least the debounce duration.
    fn drain_ready(&mut self) -> Vec<(PathBuf, AssetKind)> {
        let now = Instant::now();
        let mut ready = Vec::new();

        self.pending_reloads.retain(|path, (kind, timestamp)| {
            if now.duration_since(*timestamp) >= DEBOUNCE_DURATION {
                ready.push((path.clone(), kind.clone()));
                false // remove from pending
            } else {
                true // keep waiting
            }
        });

        ready
    }

    /// Collect a diagnostics snapshot of asset state and drain the reload log.
    #[cfg(feature = "diagnostics")]
    pub(crate) fn diagnostics_snapshot(&mut self) -> crate::diag::AssetDiagSnapshot {
        let watcher_active = self.watcher.is_some() && !self.rx_disconnected;
        let pending_count = self.pending_reloads.len();
        let watched_count = self.watched_paths.len();

        // Group watched files by kind label.
        let mut watched_files: Vec<(String, String)> = Vec::new();
        for (path, kind) in &self.watched_paths {
            let kind_label = match kind {
                #[cfg(feature = "render2d")]
                AssetKind::Texture2d(_) => "Texture2d",
                #[cfg(feature = "render3d")]
                AssetKind::Texture3d(_) => "Texture3d",
                #[cfg(feature = "render2d")]
                AssetKind::Shader2d => "Shader2d",
                #[cfg(feature = "render3d")]
                AssetKind::Shader3d => "Shader3d",
            };
            let filename = path
                .file_name()
                .map(|f| f.to_string_lossy().to_string())
                .unwrap_or_else(|| path.display().to_string());
            watched_files.push((kind_label.to_string(), filename));
        }

        let reload_events: Vec<crate::diag::ReloadEventSnapshot> = self
            .reload_log
            .drain(..)
            .map(|e| crate::diag::ReloadEventSnapshot {
                timestamp_secs: e.timestamp_secs,
                path: e.path,
                kind: e.kind,
                success: e.success,
                error: e.error,
            })
            .collect();

        crate::diag::AssetDiagSnapshot {
            watched_count,
            watcher_active,
            pending_count,
            watched_files,
            reload_events,
        }
    }
}

impl Default for AssetServer {
    fn default() -> Self {
        Self::new()
    }
}

/// Poll for filesystem changes and dispatch reloads. Called once per frame
/// from the main loop (before systems run).
pub(crate) fn process_asset_reloads(world: &mut World) {
    // Extract the asset server to avoid borrow conflicts.
    let Some(mut server) = world.resource_remove::<AssetServer>() else {
        return;
    };

    server.poll();
    let ready = server.drain_ready();

    // Put it back before dispatching reloads (dispatchers need world access).
    world.insert_resource(server);

    for (path, kind) in ready {
        match kind {
            #[cfg(feature = "render2d")]
            AssetKind::Texture2d(handle) => {
                reload_texture_2d(world, &path, handle);
            }
            #[cfg(feature = "render3d")]
            AssetKind::Texture3d(handle) => {
                reload_texture_3d(world, &path, handle);
            }
            #[cfg(feature = "render2d")]
            AssetKind::Shader2d => {
                reload_shader_2d(world, &path);
            }
            #[cfg(feature = "render3d")]
            AssetKind::Shader3d => {
                reload_shader_3d(world, &path);
            }
        }
    }
}

// ── Reload Dispatchers ──────────────────────────────────────────────────────

/// Reload a 2D texture from disk, replacing the GPU data at the existing handle.
#[cfg(feature = "render2d")]
fn reload_texture_2d(
    world: &mut World,
    path: &std::path::Path,
    handle: crate::render2d::TextureHandle,
) {
    use crate::render2d::texture::TextureStore;
    use crate::render::GpuContext;
    use crate::render2d::pipeline::SpriteRenderer;

    let img = match image::open(path) {
        Ok(img) => img.to_rgba8(),
        Err(e) => {
            log::warn!("Hot-reload failed for '{}': {e}", path.display());
            #[cfg(feature = "diagnostics")]
            push_reload_event(world, path, "Texture2d", false, Some(e.to_string()));
            return;
        }
    };
    let (width, height) = img.dimensions();
    let data = img.into_raw();

    // Extract resources needed for GPU upload.
    let Some(gpu) = world.resource_remove::<GpuContext>() else { return };
    let Some(renderer) = world.resource_remove::<SpriteRenderer>() else {
        world.insert_resource(gpu);
        return;
    };
    let Some(mut store) = world.resource_remove::<TextureStore>() else {
        world.insert_resource(gpu);
        world.insert_resource(renderer);
        return;
    };

    store.reload_entry(&gpu, &renderer, handle, width, height, &data);
    log::info!("Hot-reloaded 2D texture: {}", path.display());

    world.insert_resource(store);
    world.insert_resource(renderer);
    world.insert_resource(gpu);

    #[cfg(feature = "diagnostics")]
    push_reload_event(world, path, "Texture2d", true, None);
}

/// Reload a 3D texture from disk, replacing the GPU data at the existing handle.
#[cfg(feature = "render3d")]
fn reload_texture_3d(
    world: &mut World,
    path: &std::path::Path,
    handle: crate::render3d::TextureHandle3d,
) {
    use crate::render3d::texture::TextureStore3d;
    use crate::render::GpuContext;

    let img = match image::open(path) {
        Ok(img) => img.to_rgba8(),
        Err(e) => {
            log::warn!("Hot-reload failed for '{}': {e}", path.display());
            #[cfg(feature = "diagnostics")]
            push_reload_event(world, path, "Texture3d", false, Some(e.to_string()));
            return;
        }
    };
    let (width, height) = img.dimensions();
    let data = img.into_raw();

    let Some(gpu) = world.resource_remove::<GpuContext>() else { return };
    let Some(mut store) = world.resource_remove::<TextureStore3d>() else {
        world.insert_resource(gpu);
        return;
    };

    store.reload_entry(&gpu, handle, width, height, &data);
    log::info!("Hot-reloaded 3D texture: {}", path.display());

    world.insert_resource(store);
    world.insert_resource(gpu);

    #[cfg(feature = "diagnostics")]
    push_reload_event(world, path, "Texture3d", true, None);
}

/// Reload the 2D sprite shader from disk and recreate the pipeline.
#[cfg(feature = "render2d")]
fn reload_shader_2d(world: &mut World, path: &std::path::Path) {
    use crate::render2d::pipeline::SpriteRenderer;
    use crate::render::GpuContext;

    let source = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            log::warn!("Hot-reload failed for shader '{}': {e}", path.display());
            #[cfg(feature = "diagnostics")]
            push_reload_event(world, path, "Shader2d", false, Some(e.to_string()));
            return;
        }
    };

    let Some(gpu) = world.resource_remove::<GpuContext>() else { return };
    let Some(mut renderer) = world.resource_remove::<SpriteRenderer>() else {
        world.insert_resource(gpu);
        return;
    };

    // Push an error scope so we can catch validation errors without panicking.
    gpu.device.push_error_scope(wgpu::ErrorFilter::Validation);

    let shader = gpu.device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("sprite shader (hot-reload)"),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });

    let candidate = renderer.build_pipeline(&gpu, &shader);

    // Check if the pipeline compiled successfully before swapping it in.
    let error = pollster::block_on(gpu.device.pop_error_scope());
    if let Some(err) = error {
        log::warn!("Shader error in '{}': {err}. Keeping old pipeline.", path.display());
        #[cfg(feature = "diagnostics")]
        push_reload_event(world, path, "Shader2d", false, Some(err.to_string()));
    } else {
        renderer.pipeline = candidate;
        log::info!("Hot-reloaded 2D shader: {}", path.display());
        #[cfg(feature = "diagnostics")]
        push_reload_event(world, path, "Shader2d", true, None);
    }

    world.insert_resource(renderer);
    world.insert_resource(gpu);
}

/// Reload the 3D PBR shader from disk and recreate the pipeline.
#[cfg(feature = "render3d")]
fn reload_shader_3d(world: &mut World, path: &std::path::Path) {
    use crate::render3d::pipeline::MeshRenderer;
    use crate::render::GpuContext;

    let source = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            log::warn!("Hot-reload failed for shader '{}': {e}", path.display());
            #[cfg(feature = "diagnostics")]
            push_reload_event(world, path, "Shader3d", false, Some(e.to_string()));
            return;
        }
    };

    let Some(gpu) = world.resource_remove::<GpuContext>() else { return };
    let Some(mut renderer) = world.resource_remove::<MeshRenderer>() else {
        world.insert_resource(gpu);
        return;
    };

    gpu.device.push_error_scope(wgpu::ErrorFilter::Validation);

    let shader = gpu.device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("pbr shader (hot-reload)"),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });

    let candidate = renderer.build_pipeline(&gpu, &shader);

    // Check if the pipeline compiled successfully before swapping it in.
    let error = pollster::block_on(gpu.device.pop_error_scope());
    if let Some(err) = error {
        log::warn!("Shader error in '{}': {err}. Keeping old pipeline.", path.display());
        #[cfg(feature = "diagnostics")]
        push_reload_event(world, path, "Shader3d", false, Some(err.to_string()));
    } else {
        renderer.pipeline = candidate;
        log::info!("Hot-reloaded 3D shader: {}", path.display());
        #[cfg(feature = "diagnostics")]
        push_reload_event(world, path, "Shader3d", true, None);
    }

    world.insert_resource(renderer);
    world.insert_resource(gpu);
}

/// Push a reload event into the AssetServer's reload log (diagnostics only).
#[cfg(feature = "diagnostics")]
fn push_reload_event(
    world: &mut World,
    path: &std::path::Path,
    kind: &str,
    success: bool,
    error: Option<String>,
) {
    let timestamp_secs = world
        .get_resource::<crate::time::Time>()
        .map(|t| t.elapsed_secs())
        .unwrap_or(0.0);
    let filename = path
        .file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_else(|| path.display().to_string());
    if let Some(server) = world.get_resource_mut::<AssetServer>() {
        server.reload_log.push(ReloadEvent {
            timestamp_secs,
            path: filename,
            kind: kind.to_string(),
            success,
            error,
        });
    }
}
