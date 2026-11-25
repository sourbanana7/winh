# Winh

Ambient screen-edge rim lighting, system audio beat sync, floating mini-player with synced lyrics, and a Fullscreen Lyrics mode for Windows 10/11.

## Download & Run (.exe)

Download the latest Windows release (`v1.0.2`) from **[GitHub Releases](https://github.com/sourbanana7/winh/releases)**:

- **`Winh-Portable-1.0.2.exe`** — Standalone portable executable; double-click to run immediately with zero installation.
- **`Winh-Setup-1.0.2.exe`** — Standard Windows installer with Start Menu and Desktop shortcuts.

> **Windows SmartScreen note:** Because the release binaries are currently unsigned, Windows Defender SmartScreen may show *"Windows protected your PC"* on first launch. Click **More info** -> **Run anyway** to start Winh.

Once running, click the **Winh** icon in your Windows system tray (near the clock) to open the control flyout.

## Features

- **Unified Multi-Monitor Outer Perimeter Glow (`WebGL2`)**: Single-pass GPU signed-distance shader (`src/ui/rim-renderer.js`) combined with multi-monitor geometry detection (`src/backend/monitorLayout.js`). Automatically maps how your monitors are arranged in Windows, suppresses glow along internal monitor seams, rounds outer corners cleanly, and smoothly orbits colors around the entire outer display perimeter.
- **Single-Display Mini Player**: When spanning **All Displays**, the floating mini player & lyrics widget displays only on your primary monitor while secondary monitors stay pure perimeter glow and click-through.
- **Refresh-Rate Adaptive Animations (`60 Hz – 240+ Hz`)**: Automatically detects each monitor's `displayFrequency` (`index.js`), synchronizes VSync `requestAnimationFrame` deltas (`dt`) and phase across mixed-Hz displays, adapts WASAPI audio polling cadence (`8 ms` / `12 ms` / `16 ms`) with time-normalized smoothing (`WasapiAudioMeter.cs`), and uses GPU-composited `transform: scaleX(...)` progress bars with zero layout reflow.
- **WASAPI System Audio Sync**: Captures live system audio playback locally with zero microphone access.
- **Spotify & Browser Media Detection (`SMTC`)**: Reads active media sessions across Spotify, Chrome, Edge, Brave, Firefox, and Windows media players (`smtc_watcher.ps1`), including playback controls and seeking.
- **Automatic Web Album Art & Color Sync**: Uses SMTC thumbnails when present and falls back to iTunes / Deezer / MusicBrainz cover lookup (`src/backend/artworkLookup.js`) to import missing album art and extract adaptive rim colors (`src/backend/colorExtractor.js`).
- **Synced & Plain Lyrics (`LRCLIB`)**: Cleans Spotify and YouTube/browser titles (`src/lyrics.js`) to fetch timestamped LRC or plain lyrics.
- **Floating Mini Player & Minimize Pill**: Draggable, click-through-safe frosted glass tile (`src/overlay.html`) with hover transport controls and a minimize-to-pill toggle.
- **Fullscreen Lyrics**: Apple Music / Spotify-inspired two-column fullscreen view (`src/fullscreen.html`) with blurred album backdrop, scrubbable timeline, and depth-layered scrolling lyrics — enters true OS fullscreen over non-auto-hide taskbars.
- **Windows Tray Flyout**: Lives in the Windows system tray (`src/settings.html`) and auto-dismisses when clicking outside, with dual RGB color wheels, rotation speed & audio reactivity sliders, thickness/glow/corner-radius/opacity sliders, per-monitor refresh rate display selector, and mode toggles.

See [CHANGELOG.md](CHANGELOG.md) for detailed version history and technical notes.

## Run from Source

### Requirements

- Windows 10 or Windows 11 (`.NET Framework 4.x` `csc.exe` built in for WASAPI loopback compilation)
- Node.js 18+

### Commands

```bash
npm install
npm start
```

### Build Windows `.exe` Locally

```bash
npm run dist
```

Outputs `dist/Winh-Portable-1.0.2.exe` and `dist/Winh-Setup-1.0.2.exe`.

### Verification

```bash
npm run check
```
