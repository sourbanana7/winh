# Changelog

All notable changes and fixes to **Winh** are documented in this file.

## [1.0.2] - 2026-11-25

### Fixed & Added

1. **True OS Fullscreen Over Non-Auto-Hide Windows Taskbars (`index.js`)**
   - **Problem:** `Winh Fullscreen Lyrics` previously opened as a non-fullscreen frameless window (`resizable: false`, without calling `setFullScreen(true)`), causing Windows to keep non-auto-hide taskbars layered over the bottom of the screen. Forcing `setFullScreen(true)` with `resizable: false` also caused Electron/Windows to overflow bounds (`1934x1087` on `1920x1080`).
   - **Fix:** Updated `createFullscreenWindow()` with `resizable: true`, `maximizable: true`, `fullscreenable: true`, `autoHideMenuBar: true`, and `enableLargerThanScreen: true`, and introduced `applyFullscreenDisplay()` to set display bounds before entering true OS fullscreen (`win.setFullScreen(true)`) across initial launch, display switching, and display metrics changes.

2. **Refresh-Rate Adaptive Animations & Audio Telemetry (`index.js`, `WasapiAudioMeter.cs`, `audioMeter.js`, `rim-renderer.js`, `overlay.html`, `fullscreen.html`, `settings.html`)**
   - **Problem:** Audio loopback polling was fixed at `30 ms` (~33 Hz) with per-tick smoothing weights, causing stair-stepping on 60 Hz / 144 Hz screens and inconsistent decay if polled faster. Progress bars also triggered CSS layout reflows (`style.width` + transitions) inside `requestAnimationFrame`.
   - **Fix:**
     - Exposed `displayFrequency` via `getDisplayInfoList()` and `display-config` IPC per overlay/fullscreen window.
     - Updated `WasapiAudioMeter.cs` to use `Stopwatch`-measured `dt` time-normalized exponential smoothing (`1 - Math.Exp(-rate * dt)`) and added live `INTERVAL:<ms>` stdin updates via `audioMeterService.setIntervalMs()`.
     - Added `getOptimalAudioIntervalMs()` (`8 ms` for `>= 110 Hz`, `12 ms` for `>= 70 Hz`, `16 ms` for `60 Hz`).
     - Added `setTargetRefreshRate(hz)` and rolling VSync delta tracking (`effectiveHz`) to `LumnRimRenderer`, fixed wake-up first-frame `dt` (`1 / targetHz`), replaced `style.width` progress updates with GPU-composited `transform: scaleX(...)`, and displayed monitor refresh rates (`@ <Hz>`) in the tray flyout selector and footer.

3. **Unified Multi-Monitor Perimeter Glow & Single-Display Widget (`monitorLayout.js`, `index.js`, `rim-renderer.js`, `overlay.html`, `settings.html`)**
   - **Problem:** Spanning multiple monitors previously drew isolated 4-sided glows on every display (creating unwanted glow along shared internal monitor seams), let mixed-Hz displays drift out of color phase, and duplicated the floating album cover widget across every monitor.
   - **Fix:**
     - Added `src/backend/monitorLayout.js` (`computeMultiMonitorLayout`) to compute the union bounding box, strip shared internal borders (with `4 px` DPI tolerance), merge exposed edges into `outerSegments`, and detect true outer `convexCorners`.
     - Upgraded the WebGL2 GLSL shader (and Canvas2D fallback) in `src/ui/rim-renderer.js` to map fragments into virtual desktop coordinates, render glow strictly along `outerSegments` with rounded `convexCorners`, orbit colors seamlessly around `u_desktopCenter`, and stay phase-aligned across displays via `syncEpochMs` and `phase-sync` heartbeats.
     - Added `showWidget` gating in `display-config` so in `All Displays` mode only the primary monitor renders the floating album cover & lyrics widget while secondary displays render pure perimeter glow and stay click-through.

## [1.0.1] - 2026-11-13

- Added **Rotation Speed** (`0.2x – 3.0x`) and **Audio Reactivity** (`20% – 250%`) sliders in the tray flyout with persistent `settings.json` storage.
- Updated app logo across multi-resolution `build/icon.ico` (`16px – 256px`) and PNG icon assets.

## [1.0.0] - 2026-11-12

- Initial Windows release of **Winh** (rebranded from Lumn Windows): WebGL2 continuous bezel edge bleed, WASAPI loopback audio capture, Spotify & browser SMTC detection, automatic web album cover fallback, LRCLIB synced lyrics, Fullscreen 3D Art Lyrics mode, draggable mini-widget with minimize-to-pill, and Windows tray flyout controls.
