# Blah³ (BlahBlahBlah) — Local Voice Toolkit for macOS

## A Tauri v2 desktop app combining Speech-to-Text and Text-to-Speech with local AI models on Apple Silicon

---

## Vision

A single, lightweight, open-source macOS app that replaces SuperWhisper, FluidVoice, and paid TTS tools — all running **100% offline** on your M1 MacBook Pro (32GB RAM). Global hotkeys, screen reading, dictation, and model management in one unified interface.

**Name**: Blah³ — because talking is just organized blah blah blah.
**Package ID**: `com.blahcubed.app`
**Crate name**: `blah3`
**GitHub**: `github.com/Anomali007/blah3`

---

## Current Implementation Status

| Feature | Status | Notes |
|---------|--------|-------|
| **Tauri v2 + React** | ✅ Complete | Full app scaffolding with Tailwind CSS |
| **STT (whisper-rs)** | ✅ Working | Hold-to-record transcription with auto-paste |
| **TTS (kokoro-tiny)** | ✅ Working | Real TTS synthesis with 11 voices, speed control |
| **Global Hotkeys** | ✅ Working | ⌘+⇧+D (dictation), ⌘+⇧+S (read aloud) |
| **Audio Capture** | ✅ Working | 16kHz mono via cpal |
| **Audio Playback** | ✅ Working | Via rodio |
| **Model Manager** | ✅ Working | Download/delete with progress UI |
| **Settings Persistence** | ✅ Working | JSON in Application Support |
| **System Tray** | ✅ Working | Show/Quit menu |
| **Selected Text** | ✅ Working | AppleScript clipboard method |
| **Auto-paste** | ✅ Working | Clipboard + simulated ⌘+V |
| **CoreML Acceleration** | ✅ Implemented | CoreML encoder models downloadable via Model Manager |
| **Floating Overlay** | ✅ Polished | Dictation HUD with real waveform, timer, streaming partial transcripts |
| **Silence Detection** | ✅ Working | Auto-stop after configurable silence duration |
| **Launch at Login** | ✅ Working | Via tauri-plugin-autostart (LaunchAgent) |
| **First-run Onboarding** | ✅ Working | 5-step wizard: welcome, permissions, models, hotkeys, complete |

---

## Architecture Overview

```
┌─────────────────────────────────────────────────────────┐
│                    Tauri v2 Frontend                     │
│              (React + Tailwind CSS UI)                   │
│  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌────────────┐  │
│  │ Dictation│ │  Screen  │ │  Model   │ │  Settings  │  │
│  │   Panel  │ │  Reader  │ │  Manager │ │   Panel    │  │
│  └────┬─────┘ └────┬─────┘ └────┬─────┘ └─────┬──────┘  │
│       │             │            │              │         │
├───────┴─────────────┴────────────┴──────────────┴────────┤
│                 Tauri IPC Bridge (Commands)               │
├──────────────────────────────────────────────────────────┤
│                     Rust Backend                          │
│                                                          │
│  ┌──────────────┐  ┌──────────────┐  ┌────────────────┐  │
│  │   STT Engine │  │  TTS Engine  │  │  macOS Native  │  │
│  │              │  │              │  │    Services    │  │
│  │  whisper-rs  │  │ kokoro-tiny  │  │                │  │
│  │  (whisper.   │  │  (Kokoro     │  │ • Global Keys  │  │
│  │   cpp +      │  │   82M via    │  │ • Accessibility│  │
│  │   CoreML)    │  │   ONNX)      │  │ • Mic Capture  │  │
│  │              │  │              │  │ • Audio Output │  │
│  └──────┬───────┘  └──────┬───────┘  └───────┬────────┘  │
│         │                 │                  │           │
│  ┌──────┴─────────────────┴──────────────────┴────────┐  │
│  │              Audio Pipeline (cpal + rodio)          │  │
│  │         16-bit PCM capture ↔ WAV playback           │  │
│  └────────────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────────────┘
```

---

## Core Rust Crates

| Crate | Purpose | Status | Notes |
|-------|---------|--------|-------|
| **`whisper-rs`** | STT inference | ✅ Integrated | Rust bindings for whisper.cpp; CoreML + Metal acceleration on Apple Silicon |
| **`kokoro-tiny`** | TTS inference | ✅ Integrated | Minimal Kokoro-82M via ONNX Runtime; uses espeak-rs for phonemization |
| **`cpal`** | Audio I/O | ✅ Integrated | Cross-platform low-level audio capture at 16kHz mono |
| **`rodio`** | Audio playback | ✅ Integrated | High-level audio sink for TTS output |
| **`tauri`** v2 | App framework | ✅ Integrated | Rust + web frontend; tray icon, window management |
| **`tauri-plugin-global-shortcut`** | Global hotkeys | ✅ Integrated | Hold-to-record for STT, single press for TTS |
| **`tauri-plugin-shell`** | Shell access | ✅ Integrated | Used for AppleScript execution |
| **`tauri-plugin-autostart`** | Launch at login | ✅ Integrated | macOS LaunchAgent for auto-start |
| **`hound`** | WAV encoding | ✅ Integrated | Write audio buffers to WAV format |
| **`reqwest`** | Model downloads | ✅ Integrated | Download models from HuggingFace with progress tracking |
| **`sysinfo`** | Hardware detection | ✅ Integrated | Detect RAM, CPU cores, chip type for model recommendations |

---

## Feature Breakdown

### 1. Speech-to-Text (Dictation Mode)

**Goal**: Press a global hotkey → speak → text appears wherever your cursor is (like FluidVoice).

**Engine**: `whisper-rs` wrapping `whisper.cpp`

**Models** (user-selectable, downloaded on demand):

| Model | Size | Speed on M1 | Accuracy | Use Case |
|-------|------|-------------|----------|----------|
| `tiny.en` | 39 MB | ~30x realtime | Good | Quick drafts |
| `base.en` | 142 MB | ~15x realtime | Great | Daily use (recommended default) |
| `small.en` | 488 MB | ~6x realtime | Excellent | Important content |
| `medium.en` | 1.5 GB | ~2x realtime | Outstanding | Max accuracy |

**Apple Silicon acceleration**:
- Metal GPU: ~3-4x faster than CPU
- CoreML + Metal: ~8-12x faster (pre-built CoreML models from HuggingFace)
- Enable via `whisper-rs` feature flags: `coreml` and `metal`

**Pipeline**:
```
Global Hotkey Pressed
    → Start mic capture (cpal, 16kHz mono PCM)
    → Show dictation overlay (real-time waveform + timer)
    → Emit audio level events (50ms interval) for visualization

Hotkey Released OR Silence Detected (auto-stop)
    → Stop capture
    → Feed PCM buffer to whisper-rs (transcribe_streaming)
    → Segment callback fires per decoded segment:
        → Emit partial transcript to overlay (text appears progressively)
    → Get final transcription text
    → Paste into active app via:
        Option A: macOS pasteboard + Cmd+V simulation
        Option B: CGEventPost for character-by-character typing
    → Show result in overlay, auto-hide after 2 seconds
```

**Silence Detection** (auto-stop):

The app automatically stops recording when silence is detected after speech:
- Uses RMS (Root Mean Square) to measure audio levels in real-time
- Only triggers after speech has been detected (ignores initial silence)
- Configurable threshold (0.001–0.1 RMS) and duration (0.5–5.0 seconds)
- Default: 1.5 seconds of silence at 0.01 RMS threshold

Settings in UI (Settings → Silence Detection):
- Enable/disable auto-stop
- Silence duration slider (0.5–5.0 seconds)
- Sensitivity slider (High/Medium/Low)

```rust
// Silence detection algorithm
pub fn process(&mut self, samples: &[f32]) -> bool {
    let rms = calculate_rms(samples);
    let is_silent = rms < self.threshold;

    if is_silent {
        self.silent_samples += samples.len();
        // Only trigger if speech was detected before
        if self.speech_detected && self.silent_samples >= self.samples_needed {
            return true; // Trigger auto-stop
        }
    } else {
        self.silent_samples = 0;
        self.speech_detected = true;
    }
    false
}
```

**Key Rust code sketch** (STT command):
```rust
use whisper_rs::{WhisperContext, WhisperContextParameters, FullParams, SamplingStrategy};

#[tauri::command]
async fn transcribe(audio_data: Vec<f32>, model_path: String) -> Result<String, String> {
    let ctx = WhisperContext::new_with_params(
        &model_path,
        WhisperContextParameters::default()
    ).map_err(|e| e.to_string())?;
    
    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    params.set_language(Some("en"));
    params.set_print_progress(false);
    params.set_no_timestamps(true);
    
    let mut state = ctx.create_state().map_err(|e| e.to_string())?;
    state.full(params, &audio_data).map_err(|e| e.to_string())?;
    
    let num_segments = state.full_n_segments().map_err(|e| e.to_string())?;
    let mut text = String::new();
    for i in 0..num_segments {
        text.push_str(
            state.full_get_segment_text(i).map_err(|e| e.to_string())?.as_str()
        );
    }
    Ok(text.trim().to_string())
}
```

---

### 2. Text-to-Speech (Screen Reader Mode)

**Goal**: Select text anywhere → press hotkey → hear it read aloud with a natural AI voice.

**Engine**: `kokoro-tiny` (Rust Kokoro-82M via ONNX Runtime)

**Why Kokoro-82M**:
- Only 82M parameters — fits easily in RAM
- #1-2 on HuggingFace TTS Arena for quality
- Sub-0.3 second generation for typical sentences
- 54 voices across 8 languages
- ONNX format = no Python dependency at runtime
- Uses espeak-rs for phonemization (no tokenizer.json needed)

**Required model files** (downloaded via Model Manager):
- `kokoro-v1.0.onnx` (~330 MB) — the neural network
- `voices-v1.0.bin` (~5 MB) — voice style vectors

**System dependency**:
- `espeak-ng` must be installed: `brew install espeak-ng`

**Pipeline**:
```
Global Hotkey Pressed
    → Get selected text via AppleScript clipboard method
    → If no selection, use clipboard contents
    → Initialize TTS engine (lazy-loaded, cached for app lifetime)
    → Synthesize speech: tts.synthesize(text, voice_id) → Vec<f32>
    → Apply speed via sample rate adjustment (24kHz * speed)
    → Play through rodio AudioPlayer
    → Show floating player with stop control
```

**Key Rust code** (TTS engine wrapper):
```rust
use kokoro_tiny::TtsEngine;

pub struct KokoroEngine {
    tts: TtsEngine,
    model_dir: PathBuf,
}

impl KokoroEngine {
    pub async fn new(model_dir: PathBuf) -> Result<Self> {
        let model_path = model_dir.join("kokoro-v1.0.onnx");
        let voices_path = model_dir.join("voices-v1.0.bin");

        // Validate required files exist
        if !model_path.exists() || !voices_path.exists() {
            return Err(anyhow!("Missing TTS files. Please download from Models tab."));
        }

        let tts = TtsEngine::with_paths(
            model_path.to_string_lossy().as_ref(),
            voices_path.to_string_lossy().as_ref(),
        ).await?;

        Ok(Self { tts, model_dir })
    }

    pub fn synthesize(&mut self, text: &str, voice_id: &str, speed: f32) -> Result<AudioBuffer> {
        let samples = self.tts.synthesize(text, Some(voice_id))?;
        // Speed control via sample rate adjustment
        let adjusted_sample_rate = (24000.0 * speed) as u32;
        Ok(AudioBuffer::new(samples, adjusted_sample_rate))
    }
}
```

**TTS command** (in `commands/tts.rs`):
```rust
// Global TTS engine cache - lazy initialized on first use
static TTS_ENGINE: OnceLock<Arc<TokioMutex<Option<KokoroEngine>>>> = OnceLock::new();

#[tauri::command]
pub async fn speak_text(
    text: String,
    voice_id: String,
    speed: f32,
    _model_path: String,
) -> Result<(), String> {
    // Initialize TTS engine if not already done
    get_or_init_tts_engine().await?;

    // Synthesize speech
    let audio_buffer = {
        let state = get_tts_engine_state();
        let mut guard = state.lock().await;
        let engine = guard.as_mut().ok_or("TTS engine not initialized")?;
        engine.synthesize(&text, &voice_id, speed)?
    };

    // Play audio
    let player = AudioPlayer::new()?;
    player.play(audio_buffer.samples(), audio_buffer.sample_rate)?;
    Ok(())
}
```

---

### 3. Getting Selected Text (macOS Accessibility)

This is the glue that makes screen reading work system-wide.

**Current Implementation**: Uses AppleScript to simulate ⌘+C and read from clipboard. This is more compatible across apps but temporarily modifies the clipboard (restored after 500ms).

```rust
/// Get the currently selected text from the frontmost application.
/// Uses AppleScript as a reliable cross-app method.
pub fn get_selected_text() -> Option<String> {
    let script = r#"
        tell application "System Events"
            keystroke "c" using {command down}
        end tell
        delay 0.1
        the clipboard
    "#;

    // Save current clipboard, run script, restore clipboard
    let old_clipboard = get_clipboard();
    let output = Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output()
        .ok()?;

    if output.status.success() {
        let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
        // Restore clipboard in background thread
        // ...
        return Some(text);
    }
    None
}
```

**Future Enhancement**: Direct AXUIElement API access via `macos-accessibility-client` crate for better performance and no clipboard interference.

**Requires**: Accessibility permission (user must grant in System Settings → Privacy & Security → Accessibility)

---

### 4. Model Manager

**Goal**: Download, switch, and delete STT/TTS models from a clean UI.

**Storage**: `~/Library/Application Support/com.blahcubed.app/models/`

```
models/
├── stt/
│   ├── ggml-tiny.en.bin              (39 MB)
│   ├── ggml-base.en.bin              (142 MB)
│   ├── ggml-small.en.bin             (488 MB)
│   ├── ggml-medium.en.bin            (1.5 GB)
│   ├── ggml-tiny.en-encoder.mlmodelc/    (26 MB, CoreML Neural Engine)
│   ├── ggml-base.en-encoder.mlmodelc/    (38 MB, CoreML Neural Engine)
│   └── ggml-small.en-encoder.mlmodelc/   (130 MB, CoreML Neural Engine)
└── tts/
    ├── kokoro-v1.0.onnx          (330 MB)
    └── voices-v1.0.bin           (5 MB)
```

> **CoreML Note**: CoreML encoder models enable Neural Engine acceleration on Apple Silicon. Download the encoder matching your Whisper model for best performance. Models are distributed as `.zip` files and automatically extracted.

> **Note**: kokoro-tiny uses espeak-rs for phonemization, so no tokenizer.json is needed.

**Download sources**:
- STT models: `https://huggingface.co/ggerganov/whisper.cpp/resolve/main/`
- TTS models: `https://huggingface.co/onnx-community/Kokoro-82M-v1.0-ONNX/resolve/main/`
- CoreML models: `https://huggingface.co/ggerganov/whisper.cpp/resolve/main/` (`.mlmodelc.zip`)

**UI**: Card-based grid showing each model with size, status (downloaded/available), and a download/delete button. Progress bar during downloads.

---

### 5. Settings & Preferences

| Setting | Default | Notes |
|---------|---------|-------|
| STT Hotkey | `Cmd+Shift+D` | Configurable, hold-to-record |
| TTS Hotkey | `Cmd+Shift+S` | Read selected text |
| STT Model | `base.en` | Dropdown of downloaded models |
| TTS Voice | `af_heart` | Preview + select from 54 voices |
| TTS Speed | `1.0x` | Slider 0.25x – 5.0x (clamped) |
| Auto-paste | `true` | Paste transcription automatically |
| Launch at login | `false` | macOS login item |
| Menu bar mode | `true` | Run as menu bar app (no dock icon) |
| Silence Detection | `true` | Auto-stop recording on silence |
| Silence Threshold | `0.01` | RMS threshold (0.001–0.1) |
| Silence Duration | `1.5s` | Seconds before auto-stop (0.5–5.0) |

---

## Project Structure

```
blah3/
├── src-tauri/
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   ├── capabilities/
│   │   └── default.json          # Permissions config
│   ├── src/
│   │   ├── main.rs               # App entry point
│   │   ├── lib.rs                # Plugin registration
│   │   ├── commands/
│   │   │   ├── mod.rs
│   │   │   ├── stt.rs            # Speech-to-text commands
│   │   │   ├── tts.rs            # Text-to-speech commands
│   │   │   ├── models.rs         # Model management commands
│   │   │   ├── settings.rs       # Settings CRUD
│   │   │   └── permissions.rs    # macOS permission checking (mic + accessibility)
│   │   ├── audio/
│   │   │   ├── mod.rs
│   │   │   ├── capture.rs        # Mic recording (cpal) with silence detection
│   │   │   ├── playback.rs       # Audio output (rodio)
│   │   │   ├── processing.rs     # PCM conversion, resampling
│   │   │   └── silence.rs        # RMS-based silence detection
│   │   ├── engines/
│   │   │   ├── mod.rs
│   │   │   ├── whisper.rs        # whisper-rs wrapper
│   │   │   └── kokoro.rs         # kokoroxide wrapper
│   │   ├── accessibility/
│   │   │   ├── mod.rs
│   │   │   ├── selected_text.rs  # AXUIElement selected text
│   │   │   └── paste.rs          # Simulate paste / typing
│   │   ├── models/
│   │   │   ├── mod.rs
│   │   │   ├── download.rs       # HuggingFace model downloader
│   │   │   ├── registry.rs       # Available models catalog
│   │   │   └── hardware.rs       # Detect chip/RAM/tier, recommend models
│   └── Info.plist                # Mic + Accessibility descriptions
│
├── src/                          # Frontend (React + Tailwind)
│   ├── App.tsx
│   ├── main.tsx
│   ├── components/
│   │   ├── DictationPanel.tsx    # STT recording UI
│   │   ├── ScreenReader.tsx      # TTS playback controls
│   │   ├── ModelManager.tsx      # Download/manage models
│   │   ├── SettingsPanel.tsx     # Configuration + silence detection + autostart
│   │   ├── DictationOverlay.tsx  # Dictation HUD with waveform, streaming transcripts
│   │   ├── FloatingOverlay.tsx   # Compact pill overlay with timer, audio levels, stop
│   │   ├── StatusIndicator.tsx   # Global status + toast notifications
│   │   ├── Onboarding.tsx        # First-run wizard (permissions, models, hotkeys)
│   │   ├── VoicePreview.tsx      # Preview TTS voices
│   │   └── WaveformViz.tsx       # Audio waveform visualization
│   ├── hooks/
│   │   ├── useSTT.ts             # STT state management
│   │   ├── useTTS.ts             # TTS state management
│   │   ├── useModels.ts          # Model download state
│   │   └── usePermissions.ts     # macOS permission status polling
│   └── lib/
│       └── tauri.ts              # Typed Tauri command bindings
│
├── package.json
├── tsconfig.json
├── tailwind.config.js
└── vite.config.ts
```

---

## Cargo.toml (Current Dependencies)

```toml
[package]
name = "blah3"
version = "0.1.0"
edition = "2021"

[features]
default = ["apple-silicon"]
apple-silicon = ["whisper-rs/coreml", "whisper-rs/metal"]
intel = []  # CPU-only path, no CoreML/Metal

[lib]
name = "blah3_lib"
crate-type = ["staticlib", "cdylib", "rlib"]

[dependencies]
tauri = { version = "2", features = ["tray-icon"] }
tauri-plugin-global-shortcut = "2"
tauri-plugin-shell = "2"
tauri-plugin-autostart = "2"  # Launch at login via LaunchAgent
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["full"] }

# STT
whisper-rs = "0.13"  # CoreML/Metal enabled via features above

# TTS
kokoro-tiny = "0.1"  # Kokoro-82M via ONNX, uses espeak-rs for phonemization

# Audio
cpal = "0.15"
rodio = { version = "0.19", default-features = false, features = ["wav"] }
hound = "3.5"

# macOS native
core-foundation = "0.10"

# Utilities
reqwest = { version = "0.12", features = ["stream"] }
zip = "0.6"  # Extract CoreML model bundles
dirs = "5"
anyhow = "1"
thiserror = "1"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
sysinfo = "0.31"
futures-util = "0.3"

[target.'cfg(target_os = "macos")'.dependencies]
# macos-accessibility-client = "0.0.1"  # Uncomment when ready
```

> **Note**: kokoro-tiny requires `ort` (ONNX Runtime) which is pinned to `2.0.0-rc.10` for compatibility with rustc < 1.88. Run `cargo update ort@2.0.0-rc.11 --precise 2.0.0-rc.10` if you encounter version conflicts.

---

## Performance Optimizations

### Apple Silicon Specific

1. **CoreML for STT**: Pre-built `.mlmodelc` models run on the Neural Engine, freeing CPU/GPU for the app. whisper.cpp with CoreML achieves 8-12x realtime on M1.

2. **ONNX Runtime for TTS**: Kokoro-82M via ONNX Runtime uses Apple's Accelerate framework for matrix ops. The 82M model fits entirely in unified memory with room to spare.

3. **Streaming TTS**: Don't wait for full generation — chunk text into sentences and start playback as soon as the first chunk is ready. Overlap generation of chunk N+1 with playback of chunk N.

4. **Background model loading**: Load whisper and kokoro models into memory at app startup (or on first use), keep them warm. The M1's unified memory architecture means no GPU↔CPU transfer overhead.

5. **Audio pipeline**: Use `cpal` directly for mic capture at 16kHz mono (whisper's native format) — skip unnecessary resampling.

### Memory Budget (32GB M1)

| Component | Memory | Notes |
|-----------|--------|-------|
| Whisper base.en | ~200 MB | Model + inference buffers |
| Whisper base.en CoreML | ~300 MB | Slightly more for ANE context |
| Kokoro-82M ONNX | ~400 MB | Model + ONNX Runtime session |
| Audio buffers | ~50 MB | Generous for long recordings |
| App + UI | ~100 MB | Tauri webview + Rust runtime |
| **Total** | **~1 GB** | Leaves 31 GB for everything else |

---

## Hardware-Adaptive Model Profiles

The app detects hardware at startup and recommends the optimal model configuration. Users can always override, but smart defaults prevent frustration on lower-end machines.

### Detection Logic (Rust, runs at first launch)

```rust
#[derive(Debug, Clone, Serialize)]
pub struct HardwareProfile {
    pub chip: ChipType,        // AppleSilicon | Intel
    pub ram_gb: u64,           // Total system RAM
    pub cpu_cores: usize,      // Performance core count
    pub has_neural_engine: bool,
    pub has_metal: bool,
    pub recommended_tier: Tier, // Computed from above
}

#[derive(Debug, Clone, Serialize)]
pub enum Tier {
    Lite,     // Intel or 8GB Apple Silicon
    Standard, // 16GB Apple Silicon
    Power,    // 32GB+ Apple Silicon
}
```

### Recommended Presets by Tier

#### 🟢 Power Tier — Apple Silicon, 32GB+ RAM (your M1 Max)

| Engine | Model | Size | Performance | Notes |
|--------|-------|------|-------------|-------|
| **STT** | `base.en` + CoreML | 142 MB + CoreML | ~12x realtime | Neural Engine offload, instant feel |
| **STT alt** | `small.en` + CoreML | 488 MB + CoreML | ~6x realtime | When accuracy matters more |
| **TTS** | Kokoro-82M (ONNX) | 330 MB | <0.3s/sentence | Full quality, all 54 voices |
| **Both loaded** | — | ~1 GB total | — | Can keep both models warm in RAM |

#### 🟡 Standard Tier — Apple Silicon, 16GB RAM

| Engine | Model | Size | Performance | Notes |
|--------|-------|------|-------------|-------|
| **STT** | `base.en` + Metal | 142 MB | ~4-6x realtime | Metal GPU accel, skip CoreML to save RAM |
| **STT alt** | `tiny.en` | 39 MB | ~30x realtime | Ultra-fast, good for clear speech |
| **TTS** | Kokoro-82M (ONNX) | 330 MB | <0.5s/sentence | Full quality |
| **Strategy** | — | ~500 MB | — | Load one engine at a time; lazy-load the other |

#### 🔴 Lite Tier — Intel Mac or 8GB Apple Silicon

| Engine | Model | Size | Performance | Notes |
|--------|-------|------|-------------|-------|
| **STT** | `tiny.en` (quantized q5_0) | ~40 MB | ~2-4x realtime | AVX2 on Intel, works on 2017 MBP |
| **STT alt** | `base.en` (quantized q5_0) | ~60 MB | ~1-2x realtime | Usable but noticeable lag |
| **TTS** | Piper TTS | ~60 MB | Near-instant | Less natural but drastically lighter than Kokoro |
| **TTS alt** | Kokoro-82M (ONNX) | 330 MB | 1-3s/sentence | Usable on Intel; CPU-only ONNX, slower |
| **Strategy** | — | ~100-400 MB | — | Quantized models + Piper default; Kokoro optional |

### Quantized Models

whisper.cpp supports quantized GGML models that dramatically reduce size and memory:

| Original Model | Quantized (q5_0) | RAM Savings | Quality Loss |
|---------------|-------------------|-------------|-------------|
| `tiny.en` (75 MB) | ~40 MB | ~47% smaller | Negligible |
| `base.en` (142 MB) | ~60 MB | ~58% smaller | Minimal |
| `small.en` (488 MB) | ~190 MB | ~61% smaller | Small |
| `medium.en` (1.5 GB) | ~540 MB | ~64% smaller | Noticeable on accents |

### Piper TTS (Lite Tier Fallback)

For Intel Macs or low-RAM setups where Kokoro-82M is too heavy:

- **Piper**: 60MB models, near-instant generation, ~20 natural-sounding voices
- **Integration**: Bundle the `piper` binary or call via CLI subprocess
- **Quality**: Less natural than Kokoro, but perfectly usable for screen reading
- **Source**: `rhasspy/piper` (archived but stable, forked as `OHF-Voice/piper1-gpl`)

The app presents this as a choice in Model Manager: "Prioritize quality (Kokoro) or speed (Piper)?"

### First-Run Experience

```
1. Detect hardware → assign Tier
2. Show "Welcome" screen with recommendation:
   "Your M1 MacBook Pro (32GB) can run the full experience.
    We recommend: base.en (STT) + Kokoro (TTS)
    Total download: ~475 MB"
3. User confirms or customizes
4. Download models with progress bar
5. Grant permissions (mic + accessibility)
6. Ready to use!
```

### Future Model Support

The engine layer is abstracted behind traits:

```rust
pub trait SpeechToText: Send + Sync {
    fn transcribe(&self, audio: &[f32]) -> Result<String>;
    fn model_info(&self) -> ModelInfo;
}

pub trait TextToSpeech: Send + Sync {
    fn synthesize(&self, text: &str, voice: &str, speed: f32) -> Result<AudioBuffer>;
    fn available_voices(&self) -> Vec<VoiceInfo>;
    fn model_info(&self) -> ModelInfo;
}
```

This means adding new engines (Parakeet, Qwen3-TTS, future models) is just implementing the trait — no UI changes needed. The Model Manager can list engines from a JSON catalog that gets updated independently of app releases.

---

## Tauri v2 Configuration

### `tauri.conf.json` (key sections)

```jsonc
{
  "productName": "Blah³",
  "identifier": "com.blahcubed.app",
  "build": {
    "frontendDist": "../dist"
  },
  "app": {
    "trayIcon": {
      "iconPath": "icons/tray.png",
      "tooltip": "Blah³"
    },
    "windows": [
      {
        "label": "main",
        "title": "Blah³",
        "width": 480,
        "height": 640,
        "resizable": true,
        "decorations": true,
        "visible": false  // Start hidden, show from tray
      }
    ]
  },
  "bundle": {
    "macOS": {
      "minimumSystemVersion": "14.0"  // Sonoma+ for best CoreML
    }
  }
}
```

### `capabilities/default.json`

```json
{
  "$schema": "./schemas/desktop-schema.json",
  "identifier": "desktop-capability",
  "windows": ["main", "overlay"],
  "platforms": ["macOS"],
  "permissions": [
    "global-shortcut:allow-register",
    "global-shortcut:allow-unregister",
    "macos-permissions:default",
    "shell:allow-open",
    "core:tray-icon:default",
    "core:window:default"
  ]
}
```

### `Info.plist` additions

```xml
<key>NSMicrophoneUsageDescription</key>
<string>Blah³ needs microphone access for speech-to-text dictation.</string>
<key>NSAccessibilityUsageDescription</key>
<string>Blah³ needs accessibility access to read selected text for text-to-speech and to paste transcriptions.</string>
```

---

## Build & Development

### Prerequisites

```bash
# Rust toolchain
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Xcode CLI tools (for CoreML, Metal)
xcode-select --install

# Node.js (for frontend)
brew install node

# espeak-ng (for kokoroxide phonemization)
brew install espeak-ng

# Tauri CLI
cargo install tauri-cli --version "^2"
```

### First Run

```bash
# Clone and install
git clone https://github.com/Anomali007/blah3.git
cd blah3
pnpm install

# Dev mode (hot reload frontend + Rust rebuild)
cargo tauri dev

# Production build
cargo tauri build

# Run tests
pnpm test:run           # Frontend (Vitest)
cd src-tauri && cargo test  # Backend

# Lint
pnpm lint               # TypeScript type checking
cd src-tauri && cargo clippy -- -D warnings
```

### Build Notes

- **whisper-rs with CoreML**: Requires full Xcode (not just CLI tools) for CoreML model compilation. The `coreml` feature flag enables ANE acceleration.
- **kokoro-tiny**: Requires `espeak-ng` for phonemization. Install via `brew install espeak-ng`.
- **ONNX Runtime**: Pulled automatically by kokoro-tiny (via `ort` crate). Pin to `2.0.0-rc.10` for rustc < 1.88 compatibility.
- **ort version conflict**: If cargo fails with yanked `ort` versions, run: `cargo update ort@2.0.0-rc.11 --precise 2.0.0-rc.10`

---

## Phased Development Plan

### Phase 1: Foundation ✅
- [x] Scaffold Tauri v2 project with React frontend
- [x] Implement macOS permissions flow (mic + accessibility) — *live status checking via AXIsProcessTrusted FFI + cpal*
- [x] Build model downloader with progress UI
- [x] Set up audio capture pipeline (cpal, 16kHz mono)
- [x] Integrate whisper-rs for basic transcription
- [x] Global hotkey registration for dictation

### Phase 2: STT Polish ✅
- [x] Hold-to-record hotkey behavior
- [x] Floating recording overlay with waveform — *redesigned: real audio waveform, streaming transcripts, timer*
- [x] Auto-paste transcription into active app — *via clipboard + Cmd+V*
- [x] Silence detection for auto-stop — *RMS-based, configurable threshold/duration*
- [x] CoreML model support for speed boost — *downloadable via Model Manager, auto-detected by whisper.cpp*

### Phase 3: TTS Integration ✅
- [x] Integrate kokoro-tiny for speech synthesis — *working with 11 voices*
- [x] Read selected text via Accessibility API — *AppleScript/clipboard method*
- [x] Speed control via sample rate adjustment
- [x] Voice selection UI with preview — *8 voices available in UI*
- [x] Floating player with stop control
- [ ] Streaming playback (chunk + overlap) — *future enhancement*

### Phase 4: Model Manager & Settings ✅
- [x] Model catalog with download/delete
- [x] Persistent settings (JSON in app support dir)
- [x] Menu bar / tray icon mode — *tray icon works, dock icon still shows*
- [x] Launch at login option — *via tauri-plugin-autostart*
- [x] Keyboard shortcut customization — *configurable in settings*

### Phase 5: Polish & Ship
- [x] First-run onboarding flow — *5-step wizard with permissions, model download, hotkeys*
- [ ] Error handling & edge cases — *basic error handling in place*
- [x] DMG packaging — *12 MB unsigned DMG, 25 MB app bundle*
- [ ] Code signing & notarization — *requires Apple Developer account*
- [x] README, screenshots, demo GIF — *README complete*
- [x] GitHub release — *v0.1.0 published with DMG download*

---

## Alternative / Future Engines

The architecture is modular — swap engines without touching the UI:

| Engine | Type | Why Consider |
|--------|------|-------------|
| **Parakeet TDT v3** | STT | Faster than Whisper on Apple Silicon; what FluidVoice uses. NVIDIA NeMo model, would need ONNX/CoreML conversion. |
| **Qwen3-TTS** | TTS | Higher quality than Kokoro, voice cloning, emotion control. 0.6B params — heavier but your 32GB handles it. MLX-native. |
| **Piper TTS** | TTS | Ultra-fast (60MB models), lower quality but near-instant. Good for speed-priority reading. |
| **Candle** | Runtime | Rust-native ML framework by HuggingFace. Could replace ONNX Runtime for tighter integration. |

---

## Key Resources

- **whisper-rs**: https://github.com/tazz4843/whisper-rs
- **kokoro-tiny**: https://crates.io/crates/kokoro-tiny
- **kokoro-tiny GitHub**: https://github.com/8b-is/kokoro-tiny
- **Tauri v2 Docs**: https://v2.tauri.app
- **Tauri Global Shortcut**: https://v2.tauri.app/plugin/global-shortcut/
- **Tauri macOS Permissions**: https://github.com/ayangweb/tauri-plugin-macos-permissions
- **FluidVoice (inspiration)**: https://github.com/altic-dev/FluidVoice
- **Kokoro-82M Model**: https://huggingface.co/hexgrad/Kokoro-82M
- **Whisper CoreML Models**: https://huggingface.co/ggerganov/whisper.cpp
- **espeak-ng**: https://github.com/espeak-ng/espeak-ng
