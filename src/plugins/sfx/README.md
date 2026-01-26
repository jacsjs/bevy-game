# Audio Design (SFX Event Routing + Mixing)

This chapter provides a **complete, beginner-friendly audio architecture** for a Bevy game.
The goal is to avoid “bolt-on audio” that turns into a mess by:

- routing all sound effects through a single `SfxEvent` **message bus**
- keeping all sound handles in one place (an `AudioAssets` resource)
- centralizing mixing (master/music/sfx volumes) in an `AudioMix` resource
- keeping audio optional in headless/test builds

Bevy’s built-in audio system is ECS-driven: you play sounds by spawning entities with an `AudioPlayer` component (and `PlaybackSettings`).[^bevy_audio_module][^bevy_audio_player]
When playback starts, Bevy adds an `AudioSink` component that can be used to control playback (volume, pause, mute, etc.) while the sound is playing.[^bevy_audio_player][^bevy_audio_sink]

---

## 1) Architecture overview

### 1.1 Data flow

```text
Gameplay systems
  └─ write SfxEvent messages (high volume)

Audio plugin
  ├─ reads SfxEvent messages (MessageReader)
  ├─ looks up sound handles (AudioAssets)
  ├─ computes final volume (AudioMix)
  └─ spawns transient AudioPlayer entities (PlaybackSettings::DESPAWN)

Music system
  ├─ maintains a single looping AudioPlayer entity (PlaybackSettings::LOOP)
  └─ adjusts its AudioSink volume when AudioMix changes
```

This matches Bevy’s message model: messages are buffered and read in bulk using `MessageReader` / `MessageWriter`.[^bevy_message]
It also matches Bevy’s audio model: `AudioPlayer` triggers playback; `PlaybackSettings` configures initial behavior; `AudioSink` controls audio during playback.[^bevy_audio_player][^bevy_playback_settings][^bevy_audio_sink]

---

## 2) Why use Messages for SFX?

Sound effects are often high volume:

- bullets hitting
- enemies dying
- UI clicks

Bevy messages are explicitly designed for **buffered, pull-based** batch processing and can be more efficient than observer-style events for large numbers of occurrences.[^bevy_message]

**Rule:** gameplay systems never spawn audio entities directly; they only emit `SfxEvent` messages.

---

## 3) Define your SFX message API

### 3.1 Minimal SFX identifiers

Use a compact enum to represent what sound should play.

```rust
// src/plugins/audio/sfx.rs
use bevy::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SfxId {
    Shoot,
    Hit,
    Explosion,
    UiClick,
}

/// A request to play a sound effect.
///
/// Message-based to support high volume and batch processing.
#[derive(Message, Debug, Clone, Copy)]
pub struct SfxEvent {
    pub id: SfxId,
    /// Optional per-event gain multiplier (1.0 = default).
    pub gain: f32,
    /// Optional playback speed multiplier (1.0 = normal).
    pub speed: f32,
}

impl SfxEvent {
    pub fn new(id: SfxId) -> Self {
        Self { id, gain: 1.0, speed: 1.0 }
    }
}
```

Bevy messages are written with `MessageWriter` and read with `MessageReader`.[^bevy_message]

> Tip: If you later want spatial audio, extend `SfxEvent` with `pos: Option<Vec3>` and set `PlaybackSettings.spatial = true` for those events (Bevy supports basic stereo panning spatial audio via `PlaybackSettings`).[^bevy_playback_settings]

---

## 4) Audio assets registry (handles in one place)

### 4.1 The `AudioAssets` resource

Load your audio files once and store the handles in a resource.
This keeps string paths out of gameplay code and matches Bevy’s standard asset handle pattern.
`AudioSource` is the asset type used by `AudioPlayer` handles.[^bevy_audio_module][^bevy_audio_player]

```rust
// src/plugins/audio/assets.rs
use bevy::prelude::*;

use super::sfx::SfxId;

#[derive(Resource, Default)]
pub struct AudioAssets {
    pub shoot: Handle<AudioSource>,
    pub hit: Handle<AudioSource>,
    pub explosion: Handle<AudioSource>,
    pub ui_click: Handle<AudioSource>,
}

impl AudioAssets {
    pub fn get(&self, id: SfxId) -> Handle<AudioSource> {
        match id {
            SfxId::Shoot => self.shoot.clone(),
            SfxId::Hit => self.hit.clone(),
            SfxId::Explosion => self.explosion.clone(),
            SfxId::UiClick => self.ui_click.clone(),
        }
    }
}

pub fn load_audio_assets(asset_server: Res<AssetServer>, mut assets: ResMut<AudioAssets>) {
    assets.shoot = asset_server.load("sfx/weapons/shoot.ogg");
    assets.hit = asset_server.load("sfx/impacts/hit.ogg");
    assets.explosion = asset_server.load("sfx/impacts/explosion.ogg");
    assets.ui_click = asset_server.load("sfx/ui/click.ogg");
}
```

Bevy’s audio module shows the canonical pattern of spawning an `AudioPlayer` with a handle loaded from the `AssetServer`.[^bevy_audio_module]

---

## 5) Mixing conventions (master/music/sfx)

### 5.1 Use a single `AudioMix` resource

This resource is the “mixer UI surface area” for settings screens.

- `master`: global overall volume
- `music`: background/ambient volume
- `sfx`: sound effects volume

Bevy provides a `GlobalVolume` resource that controls global volume for audio; however, changing `GlobalVolume` does not affect already playing audio.[^bevy_global_volume]
Therefore, we use `AudioMix` as the source of truth and apply it:

- to `GlobalVolume` for newly-started audio
- to `AudioSink` for already-playing music

```rust
// src/plugins/audio/mix.rs
use bevy::prelude::*;
use bevy::audio::Volume;

#[derive(Resource, Debug, Clone, Copy)]
pub struct AudioMix {
    pub master: Volume,
    pub music: Volume,
    pub sfx: Volume,
}

impl Default for AudioMix {
    fn default() -> Self {
        Self {
            master: Volume::Linear(1.0),
            music: Volume::Linear(0.8),
            sfx: Volume::Linear(0.9),
        }
    }
}

impl AudioMix {
    pub fn sfx_final(&self, event_gain: f32) -> Volume {
        // Keep it simple: multiply linear volumes.
        // Volume supports linear and decibel scales.
        let m = self.master.to_linear();
        let s = self.sfx.to_linear();
        Volume::Linear((m * s * event_gain).clamp(0.0, 2.0))
    }

    pub fn music_final(&self) -> Volume {
        Volume::Linear((self.master.to_linear() * self.music.to_linear()).clamp(0.0, 2.0))
    }
}
```

`Volume` supports both linear and decibel representations; `Volume::Linear(1.0)` is the normal volume, and `Volume::SILENT` represents muted/off.[^bevy_volume]

### 5.2 Updating already-playing music

`PlaybackSettings` are initial settings only; changes are not applied to already-playing audio.[^bevy_playback_settings]
To change volume during playback, query `AudioSink` and call `set_volume`.[^bevy_audio_sink]

---

## 6) The audio plugin (routing + playback)

### 6.1 Plugin responsibilities

- define `SfxEvent`
- load `AudioAssets`
- store `AudioMix`
- read `SfxEvent` messages and spawn one-shot audio entities
- create and maintain a looping music entity

Bevy’s audio docs show that an app can add `AudioPlugin` explicitly (e.g., in minimal/headless setups).[^bevy_audio_module]

### 6.2 Audio enable/disable switch (for tests)

Make audio optional using a resource gate:

```rust
// src/plugins/audio/enabled.rs
use bevy::prelude::*;

#[derive(Resource, Debug, Clone, Copy)]
pub struct AudioEnabled(pub bool);

impl Default for AudioEnabled {
    fn default() -> Self {
        Self(true)
    }
}
```

### 6.3 Full plugin wiring

```rust
// src/plugins/audio/mod.rs
use bevy::prelude::*;
use bevy::audio::{AudioPlugin, GlobalVolume, PlaybackSettings, PlaybackMode, Volume};

mod assets;
mod enabled;
mod mix;
mod sfx;

use assets::*;
use enabled::*;
use mix::*;
use sfx::*;

#[derive(Component)]
struct MusicTag;

pub fn plugin(app: &mut App) {
    // In the full game, DefaultPlugins already includes audio.
    // In headless/minimal configurations you may add AudioPlugin explicitly.
    app.add_plugins(AudioPlugin::default())
        .init_resource::<AudioEnabled>()
        .init_resource::<AudioAssets>()
        .init_resource::<AudioMix>()
        .insert_resource(GlobalVolume::default())
        .add_message::<SfxEvent>()
        .add_systems(Startup, load_audio_assets)
        .add_systems(Startup, start_music)
        .add_systems(Update, (apply_mix_to_global_volume, apply_mix_to_music_sink))
        .add_systems(PostUpdate, play_sfx_messages);
}

fn start_music(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    enabled: Res<AudioEnabled>,
    mix: Res<AudioMix>,
) {
    if !enabled.0 {
        return;
    }

    let music: Handle<AudioSource> = asset_server.load("music/main_theme.ogg");

    commands.spawn((
        MusicTag,
        AudioPlayer::new(music),
        PlaybackSettings {
            mode: PlaybackMode::Loop,
            volume: mix.music_final(),
            ..default()
        },
    ));
}

fn play_sfx_messages(
    mut commands: Commands,
    enabled: Res<AudioEnabled>,
    mix: Res<AudioMix>,
    audio_assets: Res<AudioAssets>,
    mut sfx: MessageReader<SfxEvent>,
) {
    if !enabled.0 {
        // Still consume to avoid backlog.
        sfx.clear();
        return;
    }

    for e in sfx.read() {
        let handle = audio_assets.get(e.id);

        commands.spawn((
            AudioPlayer::new(handle),
            PlaybackSettings {
                mode: PlaybackMode::Despawn,
                volume: mix.sfx_final(e.gain),
                speed: e.speed,
                ..default()
            },
        ));
    }
}

fn apply_mix_to_global_volume(mut gv: ResMut<GlobalVolume>, mix: Res<AudioMix>) {
    if mix.is_changed() {
        // GlobalVolume affects all newly-started audio.
        // Note: does not affect already playing audio.
        gv.volume = mix.master;
    }
}

fn apply_mix_to_music_sink(
    mix: Res<AudioMix>,
    mut q: Query<&mut AudioSink, With<MusicTag>>,
) {
    if !mix.is_changed() {
        return;
    }
    if let Ok(mut sink) = q.get_single_mut() {
        sink.set_volume(mix.music_final());
    }
}
```

**Why this works**

- `AudioPlayer` starts playback and `PlaybackSettings` configures initial behavior.[^bevy_audio_player][^bevy_playback_settings]
- `PlaybackSettings::DESPAWN` (or `PlaybackMode::Despawn`) is ideal for one-shot SFX entities.[^bevy_playback_settings]
- `AudioSink` lets you control volume while the sound is playing (used for music bus updates).[^bevy_audio_sink]
- `GlobalVolume` controls overall volume but does not affect already playing audio, so we adjust the music sink separately.[^bevy_global_volume][^bevy_audio_sink]

---

## 7) Emitting SFX from gameplay

Gameplay systems simply write messages:

```rust
use bevy::prelude::*;
use crate::plugins::audio::sfx::{SfxEvent, SfxId};

fn on_player_shoot(mut sfx: MessageWriter<SfxEvent>) {
    sfx.write(SfxEvent::new(SfxId::Shoot));
}

fn on_bullet_hit(mut sfx: MessageWriter<SfxEvent>) {
    sfx.write(SfxEvent { id: SfxId::Hit, gain: 0.8, speed: 1.0 });
}
```

This uses Bevy’s `MessageWriter` / `MessageReader` communication model.[^bevy_message]

---

## 8) Headless tests: gating audio safely

### 8.1 Don’t add audio at all in headless configs

In your headless test harness, avoid adding `AudioPlugin`.
Bevy’s audio docs show that audio support is provided by `AudioPlugin`, and can be added explicitly when needed.[^bevy_audio_module]

### 8.2 If you share plugin wiring, gate audio with `AudioEnabled(false)`

This avoids touching audio devices and keeps message queues healthy by still consuming `SfxEvent` messages.

---

## 9) Tests

### 9.1 Test: SFX messages spawn audio entities when enabled

```

### 7.1 Example: explicit `SfxEvent` messages

Here are **concrete `SfxEvent` payloads** you can emit from gameplay code.
These examples show the default constructor and manual overrides for gain and speed.

```rust
use bevy::prelude::*;
use crate::plugins::audio::sfx::{SfxEvent, SfxId};

fn emit_some_sfx(mut sfx: MessageWriter<SfxEvent>) {
    // 1) Minimal / default: gain=1.0, speed=1.0
    sfx.write(SfxEvent::new(SfxId::Shoot));

    // 2) Quieter hit (80% gain)
    sfx.write(SfxEvent { id: SfxId::Hit, gain: 0.8, speed: 1.0 });

    // 3) "Big" explosion: louder + slightly slower playback
    sfx.write(SfxEvent { id: SfxId::Explosion, gain: 1.3, speed: 0.92 });

    // 4) UI click: often slightly quieter
    sfx.write(SfxEvent { id: SfxId::UiClick, gain: 0.6, speed: 1.0 });
}
```

Why this is a good pattern:

- Gameplay emits a **small, stable data message** (no audio handles or paths).
- The audio plugin decides how to map `SfxId → Handle<AudioSource>` and how to apply mixing.
- Using `gain` and `speed` gives expressive variation without multiplying asset files.

rust
use bevy::prelude::*;
use bevy::audio::AudioPlayer;

use crate::plugins::audio::{self, enabled::AudioEnabled};
use crate::plugins::audio::sfx::{SfxEvent, SfxId};

# [test]
fn sfx_spawns_audio_player_entities_when_enabled() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);

    audio::plugin(&mut app);
    app.insert_resource(AudioEnabled(true));

    // Write an SFX message
    app.world_mut().write_message(SfxEvent::new(SfxId::UiClick));

    // Run one tick
    app.update();

    // Ensure at least one AudioPlayer exists
    let count = app.world().query::<&AudioPlayer>().iter(app.world()).count();
    assert!(count >= 1);
}

```

`AudioPlayer` is the component that triggers an audio source to begin playing when inserted.[^bevy_audio_player]

### 9.2 Test: headless gating consumes messages but spawns nothing

```rust
use bevy::prelude::*;
use bevy::audio::AudioPlayer;

use crate::plugins::audio::{self, enabled::AudioEnabled};
use crate::plugins::audio::sfx::{SfxEvent, SfxId};

#[test]
fn sfx_messages_are_consumed_but_no_audio_when_disabled() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);

    audio::plugin(&mut app);
    app.insert_resource(AudioEnabled(false));

    app.world_mut().write_message(SfxEvent::new(SfxId::Shoot));
    app.update();

    // No audio players should be spawned
    let count = app.world().query::<&AudioPlayer>().iter(app.world()).count();
    assert_eq!(count, 0);
}
```

We still consume messages using `MessageReader::clear()` in the audio system to prevent backlog when disabled.[^bevy_message_reader]

---

## 10) Common pitfalls (and fixes)

### Pitfall: volume changes don’t affect already playing audio

Fix: use `AudioSink` for live updates; `PlaybackSettings` and `GlobalVolume` only affect playback at creation time (and `GlobalVolume` does not affect already playing audio).[^bevy_playback_settings][^bevy_global_volume][^bevy_audio_sink]

### Pitfall: creating/reinserting audio output repeatedly

Fix: add `AudioPlugin` once. Bevy’s internal `AudioOutput` initialization intentionally leaks the `OutputStream` to prevent audio stopping, and repeatedly inserting it would leak more memory.[^bevy_audio_output]

### Pitfall: backlog of SFX messages in headless tests

Fix: if audio is disabled, still **consume** `SfxEvent` messages (`reader.clear()`) to keep tests deterministic and avoid message backlog.[^bevy_message_reader]

---

## References

[^bevy_audio_module]: Bevy audio module docs (AudioPlugin, AudioPlayer, AudioSink, GlobalVolume, PlaybackSettings): <https://docs.rs/bevy/latest/bevy/audio/index.html>
[^bevy_audio_player]: Bevy `AudioPlayer` docs (insert to begin playback; AudioSink added when playback begins; uses PlaybackSettings): <https://docs.rs/bevy/latest/bevy/audio/struct.AudioPlayer.html>
[^bevy_audio_sink]: Bevy `AudioSink` docs (control playback; set_volume, mute, pause, etc.): <https://docs.rs/bevy/latest/bevy/audio/struct.AudioSink.html>
[^bevy_playback_settings]: Bevy `PlaybackSettings` docs (initial settings; volume/speed/mode; changes don’t affect already-playing audio): <https://docs.rs/bevy/latest/bevy/audio/struct.PlaybackSettings.html>
[^bevy_global_volume]: Bevy `GlobalVolume` docs (global volume resource; does not affect already playing audio): <https://docs.rs/bevy/latest/bevy/audio/struct.GlobalVolume.html>
[^bevy_volume]: Bevy `Volume` docs (Linear/Decibels; SILENT constant; conversion): <https://docs.rs/bevy/latest/bevy/audio/enum.Volume.html>
[^bevy_message]: Bevy `Message` trait docs (buffered pull-based messages; MessageWriter/MessageReader): <https://docs.rs/bevy/latest/bevy/prelude/trait.Message.html>
[^bevy_message_reader]: Bevy `MessageReader` docs (`read`, `len`, `is_empty`, `clear`): <https://docs.rs/bevy/latest/bevy/prelude/struct.MessageReader.html>
[^bevy_audio_output]: Bevy audio output internals (AudioOutput leaks OutputStream to prevent stopping; repeated insertion leaks more memory): <https://github.com/bevyengine/bevy/blob/main/crates/bevy_audio/src/audio_output.rs>
