# Winh

Ambient screen-edge rim lighting, system audio beat sync, floating mini-player with synced lyrics, and a fullscreen lyrics mode for Windows 10/11 built with Rust and Tauri v2.

## Download & Run (.exe)

Download the latest Windows release (`v1.0.3`) from **[GitHub Releases](https://github.com/sourbanana7/winh/releases)**:

- **`Winh-Portable-1.0.3.exe`**: Standalone native binary (runs directly without installation, Node.js, or `.NET` runtime dependencies; uses the built-in Windows WebView2 runtime).
- **`Winh-Setup-1.0.3.exe`**: Standard Windows installer with Start Menu and Desktop shortcuts.

> **Windows SmartScreen note:** Because the release binaries are currently unsigned, Windows Defender SmartScreen may show *"Windows protected your PC"* on first launch. Click **More info** -> **Run anyway** to start Winh.

Once running, click the **Winh** icon in your Windows system tray (near the clock) to open the control flyout.

## Features

- **Unified Multi-Monitor Outer Perimeter Glow (`WebGL2`)**: Single-pass GPU signed-distance shader (`src/ui/rim-renderer.js`) combined with native Win32 display detection (`src-tauri/src/monitor.rs`). Automatically maps monitor bounds in Windows, suppresses glow along internal monitor seams, rounds outer corners, and smoothly orbits colors around the outer display perimeter.
- **Single-Display Mini Player**: When spanning **All Displays**, the floating mini player & lyrics widget appears only on your primary monitor while secondary monitors stay pure perimeter glow and click-through.
- **Refresh-Rate Adaptive Animations (`60 Hz - 240+ Hz`)**: Detects each monitor's exact `displayFrequency`, synchronizes VSync `requestAnimationFrame` deltas (`dt`) and phase across mixed-Hz displays, adapts WASAPI audio polling cadence (`8 ms` / `12 ms` / `16 ms`) with time-normalized smoothing (`src-tauri/src/audio.rs`), and uses GPU-composited `transform: scaleX(...)` progress bars.
- **Native WASAPI System Audio Sync**: Captures live system audio playback in Rust (`IAudioClient` + `IAudioCaptureClient` loopback) locally with no microphone access or `.NET` runtime compilation.
- **Native WinRT SMTC Media Controls**: Reads active media sessions across Spotify, Chrome, Edge, Brave, Firefox, and Windows media players (`src-tauri/src/smtc.rs`), including playback controls and seeking.
- **Automatic Web Album Art & Color Sync**: Uses embedded SMTC thumbnails when valid and detects non-square or letterboxed browser video frames (such as YouTube videos in Chrome, Edge, Brave, or Firefox) to fetch square high-resolution cover art from iTunes, Deezer, and MusicBrainz (`src-tauri/src/artwork.rs`) with native dominant/accent rim color extraction.
- **Synced & Plain Lyrics (`LRCLIB`)**: Cleans Spotify and YouTube/browser titles (`src-tauri/src/artwork.rs`) to fetch timestamped LRC or plain lyrics.
- **Floating Mini Player & Minimize Pill**: Draggable, click-through-safe translucent card (`src/overlay.html`) with hover transport controls and a minimize-to-pill toggle.
- **Two-Column Fullscreen Lyrics**: Blurred full-bleed album backdrop, minimal top window strip (`28px`), cover-aligned scrubbable timeline, and center-aligned depth-blurred synced lyrics (`src/fullscreen.html`).
- **Windows Tray Flyout**: Lives in the Windows system tray (`src/settings.html` + `src-tauri/src/window_manager.rs`) and auto-dismisses when clicking outside, with dual RGB color wheels, rotation speed & audio reactivity sliders, thickness/glow/corner-radius/opacity sliders, per-monitor refresh rate display selector, and mode toggles.

See [CHANGELOG.md](CHANGELOG.md) for version history and notes.

## Build from Source

### Developer Prerequisites

- Windows 10 or Windows 11 (WebView2 runtime)
- Rust toolchain (`rustc` / `cargo`) & Visual Studio 2022 C++ Build Tools
- Node.js 18+ (for `@tauri-apps/cli`)

### Development & Verification

```bash
npm install
npm run check
npm run dev
```

### Build Windows Release Binaries

```bash
npx tauri build
```

Outputs release binaries in `src-tauri/target/release/winh.exe` and `src-tauri/target/release/bundle/nsis/Winh_1.0.3_x64-setup.exe`.

## License

Winh is licensed under the [MIT License](LICENSE).
