//! Audio playback via [kira](https://docs.rs/kira).
//!
//! Provides [`AudioEngine`] (resource), [`SoundData`] (loadable audio),
//! [`SoundHandle`] (playing sound control), and [`AudioSource`] (ECS component).
//! Run [`audio_system`] each frame to auto-play component-attached sounds and
//! clean up finished handles.
//!
//! # Example
//!
//! ```ignore
//! use kera::prelude::*;
//!
//! let mut engine = AudioEngine::new();
//! let sfx = SoundData::from_file("assets/blip.ogg").unwrap();
//! let handle = engine.play(&sfx);
//! ```

use std::fmt;
use std::path::Path;

use kira::sound::static_sound::{StaticSoundData, StaticSoundHandle};
use kira::sound::PlaybackState;
use kira::{AudioManager, AudioManagerSettings, Decibels, DefaultBackend, Tween};

use crate::ecs::World;

/// Convert a linear amplitude (0.0 = silence, 1.0 = full) to decibels.
fn amplitude_to_db(amplitude: f64) -> Decibels {
    if amplitude <= 0.0 {
        Decibels::SILENCE
    } else {
        Decibels((20.0 * amplitude.log10()) as f32)
    }
}

// ── Errors ──────────────────────────────────────────────────────────────

/// Errors that can occur in the audio system.
#[derive(Debug)]
pub enum AudioError {
    /// Failed to initialize the audio backend.
    BackendInit(String),
    /// Failed to load a sound file.
    Load(String),
    /// Failed to play a sound.
    Play(String),
}

impl fmt::Display for AudioError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AudioError::BackendInit(e) => write!(f, "audio backend init failed: {e}"),
            AudioError::Load(e) => write!(f, "audio load failed: {e}"),
            AudioError::Play(e) => write!(f, "audio play failed: {e}"),
        }
    }
}

impl std::error::Error for AudioError {}

// ── SoundData ───────────────────────────────────────────────────────────

/// Decoded audio data, cheap to clone (shared via `Arc` internally).
///
/// Load from disk with [`from_file`](SoundData::from_file), then configure
/// with builder methods before passing to [`AudioEngine::play`].
#[derive(Clone)]
pub struct SoundData {
    inner: StaticSoundData,
}

impl SoundData {
    /// Load audio data from a file path (OGG, MP3, WAV, FLAC).
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, AudioError> {
        let data =
            StaticSoundData::from_file(path).map_err(|e| AudioError::Load(e.to_string()))?;
        Ok(Self { inner: data })
    }

    /// Set the volume (amplitude scale, 1.0 = full).
    pub fn volume(mut self, volume: f64) -> Self {
        self.inner = self.inner.volume(amplitude_to_db(volume));
        self
    }

    /// Loop the entire sound.
    pub fn looping(mut self) -> Self {
        self.inner = self.inner.loop_region(..);
        self
    }

    /// Set the playback rate (1.0 = normal speed).
    pub fn playback_rate(mut self, rate: f64) -> Self {
        self.inner = self.inner.playback_rate(rate);
        self
    }

    /// Set stereo panning (0.0 = left, 0.5 = center, 1.0 = right).
    pub fn panning(mut self, panning: f64) -> Self {
        self.inner = self.inner.panning(panning as f32);
        self
    }
}

impl fmt::Debug for SoundData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SoundData").finish_non_exhaustive()
    }
}

// ── SoundHandle ─────────────────────────────────────────────────────────

/// Handle to a playing sound. Use to pause, resume, stop, or adjust volume.
pub struct SoundHandle {
    inner: StaticSoundHandle,
}

impl SoundHandle {
    /// Pause playback instantly.
    pub fn pause(&mut self) {
        self.inner.pause(Tween::default());
    }

    /// Resume playback instantly.
    pub fn resume(&mut self) {
        self.inner.resume(Tween::default());
    }

    /// Stop playback instantly. The handle cannot be restarted after this.
    pub fn stop(&mut self) {
        self.inner.stop(Tween::default());
    }

    /// Set the volume of this playing sound (amplitude scale, 1.0 = full).
    pub fn set_volume(&mut self, volume: f64) {
        self.inner
            .set_volume(amplitude_to_db(volume), Tween::default());
    }

    /// Set the playback rate of this playing sound (1.0 = normal speed).
    pub fn set_playback_rate(&mut self, rate: f64) {
        self.inner.set_playback_rate(rate, Tween::default());
    }

    /// Returns `true` if the sound has finished or been stopped.
    pub fn is_stopped(&self) -> bool {
        matches!(self.inner.state(), PlaybackState::Stopped)
    }
}

impl fmt::Debug for SoundHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SoundHandle")
            .field("state", &self.inner.state())
            .finish()
    }
}

// ── AudioEngine ─────────────────────────────────────────────────────────

/// The audio engine resource. Wraps kira's `AudioManager` and provides a
/// simple API for playing sounds.
///
/// Insert as a resource and use directly or via [`audio_system`].
pub struct AudioEngine {
    manager: AudioManager<DefaultBackend>,
}

impl AudioEngine {
    /// Create a new audio engine with default settings.
    ///
    /// # Panics
    ///
    /// Panics if the audio backend cannot be initialized.
    pub fn new() -> Self {
        let manager = AudioManager::<DefaultBackend>::new(AudioManagerSettings::default())
            .expect("Failed to initialize audio backend");
        Self { manager }
    }

    /// Try to create a new audio engine, returning an error on failure.
    pub fn try_new() -> Result<Self, AudioError> {
        let manager = AudioManager::<DefaultBackend>::new(AudioManagerSettings::default())
            .map_err(|e| AudioError::BackendInit(e.to_string()))?;
        Ok(Self { manager })
    }

    /// Play a sound, returning a handle for controlling it.
    pub fn play(&mut self, sound: &SoundData) -> SoundHandle {
        let handle = self
            .manager
            .play(sound.inner.clone())
            .expect("Failed to play sound");
        SoundHandle { inner: handle }
    }

    /// Try to play a sound, returning a handle or an error.
    pub fn try_play(&mut self, sound: &SoundData) -> Result<SoundHandle, AudioError> {
        let handle = self
            .manager
            .play(sound.inner.clone())
            .map_err(|e| AudioError::Play(e.to_string()))?;
        Ok(SoundHandle { inner: handle })
    }

    /// Set the main (global) volume for all sounds (amplitude scale, 1.0 = full).
    pub fn set_main_volume(&mut self, volume: f64) {
        self.manager
            .main_track()
            .set_volume(amplitude_to_db(volume), Tween::default());
    }
}

impl Default for AudioEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for AudioEngine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AudioEngine").finish_non_exhaustive()
    }
}

// ── AudioSource component ───────────────────────────────────────────────

/// An entity-attached audio source component.
///
/// When combined with [`audio_system`], sounds with `auto_play = true` will
/// start playing automatically. The internal handle is managed by the system.
///
/// # Example
///
/// ```ignore
/// world.spawn((
///     Transform::default(),
///     AudioSource::new(sfx_data).auto_play().looping().with_volume(0.5),
/// ));
/// ```
pub struct AudioSource {
    /// The sound data to play.
    pub sound: SoundData,
    /// Whether to auto-play when first discovered by `audio_system`.
    pub auto_play: bool,
    /// Whether the sound should loop.
    pub looping: bool,
    /// Volume for this source (amplitude scale, 1.0 = full).
    pub volume: f32,
    /// Internal handle to the playing sound (managed by `audio_system`).
    pub(crate) handle: Option<SoundHandle>,
}

impl AudioSource {
    /// Create a new audio source from sound data.
    pub fn new(sound: SoundData) -> Self {
        Self {
            sound,
            auto_play: false,
            looping: false,
            volume: 1.0,
            handle: None,
        }
    }

    /// Enable auto-play (the sound starts when `audio_system` first sees it).
    pub fn auto_play(mut self) -> Self {
        self.auto_play = true;
        self
    }

    /// Enable looping.
    pub fn looping(mut self) -> Self {
        self.looping = true;
        self
    }

    /// Set the volume (builder pattern).
    pub fn with_volume(mut self, volume: f32) -> Self {
        self.volume = volume;
        self
    }
}

impl fmt::Debug for AudioSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AudioSource")
            .field("auto_play", &self.auto_play)
            .field("looping", &self.looping)
            .field("volume", &self.volume)
            .field("playing", &self.handle.is_some())
            .finish()
    }
}

// ── Plugin ──────────────────────────────────────────────────────────────

/// Plugin: inserts an [`AudioEngine`] resource and registers [`audio_system`].
pub struct AudioPlugin;

impl crate::app::Plugin for AudioPlugin {
    fn build(&self, app: &mut crate::app::App) {
        app.world.insert_resource(AudioEngine::new());
        app.systems.add_system(audio_system);
    }
}

// ── System ──────────────────────────────────────────────────────────────

/// Audio system — auto-plays `AudioSource` components and cleans up finished sounds.
///
/// Uses the extract/reinsert pattern for `AudioEngine` (same as `physics_step_2d`).
pub fn audio_system(world: &mut World) {
    let Some(mut engine) = world.resource_remove::<AudioEngine>() else {
        return;
    };

    // Collect entities that need to start playing.
    let mut to_play: Vec<(crate::ecs::Entity, SoundData, bool, f32)> = Vec::new();
    world.query::<(&AudioSource,)>(|entity, (src,)| {
        if src.auto_play && src.handle.is_none() {
            to_play.push((entity, src.sound.clone(), src.looping, src.volume));
        }
    });

    // Start playback for new auto-play sources.
    for (entity, sound, looping, volume) in to_play {
        let mut data = sound;
        if looping {
            data = data.looping();
        }
        data = data.volume(volume as f64);
        let handle = engine.play(&data);
        if let Some(src) = world.get_mut::<AudioSource>(entity) {
            src.handle = Some(handle);
        }
    }

    // Clean up stopped handles.
    let mut stopped: Vec<crate::ecs::Entity> = Vec::new();
    world.query::<(&AudioSource,)>(|entity, (src,)| {
        if let Some(ref handle) = src.handle {
            if handle.is_stopped() {
                stopped.push(entity);
            }
        }
    });
    for entity in stopped {
        if let Some(src) = world.get_mut::<AudioSource>(entity) {
            src.handle = None;
        }
    }

    world.insert_resource(engine);
}
