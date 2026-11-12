# Winh

Ambient screen-edge rim lighting, system audio beat sync, floating mini-player with synced lyrics, and a 3D Fullscreen Lyrics mode for Windows 10/11.

## Features

- **Orbiting Ambient Rim Bleed (`WebGL2`)**: Single-pass GPU signed-distance (`sdRoundedBox`) bezel glow that smoothly circles all four screen edges and reacts to audio frequencies without visible circular artifacts.
- **WASAPI System Audio Sync**: Captures live system audio playback (`WasapiAudioMeter.cs`) locally with zero microphone access.
- **Spotify & Browser Media Detection (`SMTC`)**: Reads active media sessions across Spotify, Chrome, Edge, Brave, Firefox, and Windows media players (`smtc_watcher.ps1`), including playback controls and seeking.
- **Automatic Web Album Art & Color Sync**: Uses SMTC thumbnails when present and falls back to iTunes / Deezer / MusicBrainz cover lookup (`src/backend/artworkLookup.js`) to import missing album art and extract adaptive rim colors (`src/backend/colorExtractor.js`).
- **Synced & Plain Lyrics (`LRCLIB`)**: Cleans Spotify and YouTube/browser titles (`src/lyrics.js`) to fetch timestamped LRC or plain lyrics.
- **Floating Mini-Widget & Minimize Pill**: Draggable, click-through-safe frosted glass tile (`src/overlay.html`) with hover transport controls and a minimize-to-pill toggle.
- **Fullscreen 3D Art Lyrics**: Apple Music / Spotify-inspired two-column fullscreen view (`src/fullscreen.html`) with blurred album backdrop, scrubbable timeline, and depth-layered scrolling lyrics.
- **Windows Tray Flyout**: Lives in the Windows system tray (`src/settings.html`) and auto-dismisses when clicking outside, with dual RGB color wheels, thickness/glow/corner-radius/opacity sliders, display selector, and mode toggles.

## Requirements

- Windows 10 or Windows 11 (`.NET Framework 4.x` `csc.exe` built in for WASAPI loopback compilation)
- Node.js 18+

## Getting Started

```bash
npm install
npm start
```

Click the **Winh** icon in your Windows system tray to open the control flyout.

## Verification

```bash
npm run check
```
