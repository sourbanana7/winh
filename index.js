// d:/Antigravity/lumn-windows/index.js
const { app, BrowserWindow, screen, ipcMain, Tray, Menu, nativeImage } = require('electron');
const path = require('path');

const smtcService = require('./src/smtc');
const { searchLrclib } = require('./src/lyrics');
const audioMeterService = require('./src/backend/audioMeter');
const { ZERO_LEVEL } = require('./src/backend/audioMeter');
const { initArtworkCache, lookupAlbumArtwork } = require('./src/backend/artworkLookup');
const { extractPaletteFromBase64 } = require('./src/backend/colorExtractor');
const { computeMultiMonitorLayout } = require('./src/backend/monitorLayout');
const settingsStore = require('./src/backend/settingsStore');
const { createTrayPngBuffer } = require('./src/backend/trayIcon');

const APP_START_EPOCH_MS = Date.now();

app.setName('Winh');
if (process.platform === 'win32') {
  app.setAppUserModelId('com.winh.app');
}

const overlayWindows = new Map(); // Map<string, BrowserWindow> keyed by display.id
let settingsWindow = null;
let fullscreenWindow = null;
let tray = null;
let isQuitting = false;

let latestMedia = null;
let currentTrackKey = '';
let currentArtworkTrackKey = '';
let lastArtworkLookupKey = '';
let currentLyricsCache = null;
let lastArtworkFp = '';
let lastTraySignature = '';
let lastBlurTimestamp = 0;

let wasOverlayZeroSent = false;
let wasFullscreenZeroSent = false;

const gotSingleInstanceLock = app.requestSingleInstanceLock();
if (!gotSingleInstanceLock) {
  app.quit();
} else {
  app.on('second-instance', () => {
    showFlyoutWindow();
  });
}

function getDisplayInfoList() {
  const primaryId = String(screen.getPrimaryDisplay().id);
  return screen.getAllDisplays().map((d, idx) => {
    const id = String(d.id);
    const isPrimary = id === primaryId;
    const rawLabel = typeof d.label === 'string' && d.label.trim() ? d.label.trim() : '';
    return {
      id,
      label: rawLabel || (isPrimary ? 'Primary Display' : `Display ${idx + 1}`),
      x: d.bounds.x,
      y: d.bounds.y,
      width: d.bounds.width,
      height: d.bounds.height,
      scaleFactor: d.scaleFactor || 1,
      displayFrequency: Math.max(30, Math.round(Number(d.displayFrequency) || 60)),
      isPrimary
    };
  });
}

function getDisplayConfig(display) {
  if (!display) return null;
  const s = settingsStore.get();
  const primaryId = String(screen.getPrimaryDisplay().id);
  const id = String(display.id);
  const isPrimary = id === primaryId;
  const rawLabel = typeof display.label === 'string' && display.label.trim() ? display.label.trim() : '';

  const activeDisplays = getTargetDisplays(s.displayId);
  const layout = computeMultiMonitorLayout(activeDisplays);
  const showWidget = s.displayId === 'all' ? isPrimary : true;

  return {
    id,
    label: rawLabel || (isPrimary ? 'Primary Display' : `Display ${id}`),
    width: display.bounds.width,
    height: display.bounds.height,
    scaleFactor: display.scaleFactor || 1,
    displayFrequency: Math.max(30, Math.round(Number(display.displayFrequency) || 60)),
    isPrimary,
    showWidget,
    thisBounds: {
      x: display.bounds.x,
      y: display.bounds.y,
      width: display.bounds.width,
      height: display.bounds.height
    },
    layout,
    syncEpochMs: APP_START_EPOCH_MS
  };
}

function getOptimalAudioIntervalMs() {
  const settings = settingsStore.get();
  const targets = getTargetDisplays(settings.displayId);
  let maxHz = 60;
  for (const d of targets) {
    const hz = Math.round(Number(d.displayFrequency) || 60);
    if (hz > maxHz) maxHz = hz;
  }
  if (maxHz >= 110) return 8;
  if (maxHz >= 70) return 12;
  return 16;
}

function getTargetDisplays(displayIdSetting) {
  const all = screen.getAllDisplays();
  if (!displayIdSetting || displayIdSetting === 'all') {
    return all;
  }
  const match = all.filter((d) => String(d.id) === String(displayIdSetting));
  return match.length > 0 ? match : all;
}

function getFullscreenTargetDisplay() {
  const displayId = settingsStore.get().displayId;
  const all = screen.getAllDisplays();
  if (displayId && displayId !== 'all') {
    const match = all.find((d) => String(d.id) === String(displayId));
    if (match) return match;
  }
  return screen.getPrimaryDisplay();
}

function buildEmptyLyricsPayload() {
  return {
    trackKey: '',
    loading: false,
    success: false,
    found: false,
    instrumental: false,
    source: 'none',
    trackName: '',
    artistName: '',
    albumName: '',
    duration: 0,
    isSynced: false,
    syncedLyrics: null,
    plainLyrics: null
  };
}

function sendInitialOverlayState(webContents, targetDisplay = null) {
  if (!webContents || webContents.isDestroyed()) return;

  let disp = targetDisplay;
  if (!disp) {
    if (fullscreenWindow && !fullscreenWindow.isDestroyed() && webContents.id === fullscreenWindow.webContents.id) {
      disp = getFullscreenTargetDisplay();
    } else {
      for (const [dispId, win] of overlayWindows.entries()) {
        if (!win.isDestroyed() && win.webContents.id === webContents.id) {
          disp = screen.getAllDisplays().find((d) => String(d.id) === dispId) || null;
          break;
        }
      }
      if (!disp) {
        try {
          const win = BrowserWindow.fromWebContents(webContents);
          if (win && !win.isDestroyed()) {
            disp = screen.getDisplayMatching(win.getBounds());
          }
        } catch {}
      }
      if (!disp) {
        disp = screen.getPrimaryDisplay();
      }
    }
  }

  webContents.send('settings-updated', settingsStore.get());
  webContents.send('displays-updated', getDisplayInfoList());
  if (disp) {
    webContents.send('display-config', getDisplayConfig(disp));
  }
  if (latestMedia) {
    webContents.send('media-update', latestMedia);
  }
  webContents.send('lyrics-update', currentLyricsCache || buildEmptyLyricsPayload());
}

function createOverlayForDisplay(display) {
  const { x, y, width, height } = display.bounds;

  const win = new BrowserWindow({
    x,
    y,
    width,
    height,
    title: 'Winh Overlay',
    transparent: true,
    frame: false,
    alwaysOnTop: true,
    skipTaskbar: true,
    hasShadow: false,
    resizable: false,
    movable: false,
    focusable: false,
    enableLargerThanScreen: true,
    webPreferences: {
      nodeIntegration: true,
      contextIsolation: false,
      backgroundThrottling: false
    }
  });

  try {
    win.setAlwaysOnTop(true, 'screen-saver');
    win.setVisibleOnAllWorkspaces(true, { visibleOnFullScreen: true });
  } catch {
    // ignore unsupported platform options
  }

  win.setIgnoreMouseEvents(true, { forward: true });
  win.loadFile(path.join(__dirname, 'src', 'overlay.html'));

  win.webContents.on('did-finish-load', () => {
    sendInitialOverlayState(win.webContents, display);
  });

  return win;
}

function syncOverlayWindows() {
  const settings = settingsStore.get();
  const targetDisplays = getTargetDisplays(settings.displayId);
  const targetIds = new Set(targetDisplays.map((d) => String(d.id)));

  for (const [dispId, win] of overlayWindows.entries()) {
    if (!targetIds.has(dispId) || win.isDestroyed()) {
      overlayWindows.delete(dispId);
      if (!win.isDestroyed()) {
        try { win.destroy(); } catch {}
      }
    }
  }

  for (const display of targetDisplays) {
    const dispId = String(display.id);
    const existing = overlayWindows.get(dispId);
    if (existing && !existing.isDestroyed()) {
      const b = existing.getBounds();
      if (
        b.x !== display.bounds.x ||
        b.y !== display.bounds.y ||
        b.width !== display.bounds.width ||
        b.height !== display.bounds.height
      ) {
        existing.setBounds(display.bounds);
      }
      existing.webContents.send('display-config', getDisplayConfig(display));
    } else {
      overlayWindows.set(dispId, createOverlayForDisplay(display));
    }
  }
}

function broadcastOverlays(channel, payload) {
  for (const win of overlayWindows.values()) {
    if (!win.isDestroyed()) {
      win.webContents.send(channel, payload);
    }
  }
}

function sendToFullscreen(channel, payload) {
  if (fullscreenWindow && !fullscreenWindow.isDestroyed()) {
    fullscreenWindow.webContents.send(channel, payload);
  }
}

function broadcastDisplays() {
  const displays = getDisplayInfoList();
  broadcastOverlays('displays-updated', displays);
  sendToFullscreen('displays-updated', displays);
  if (settingsWindow && !settingsWindow.isDestroyed()) {
    settingsWindow.webContents.send('displays-updated', displays);
  }
  for (const [dispId, win] of overlayWindows.entries()) {
    if (!win.isDestroyed()) {
      const match = screen.getAllDisplays().find((d) => String(d.id) === dispId);
      if (match) {
        win.webContents.send('display-config', getDisplayConfig(match));
      }
    }
  }
  sendToFullscreen('display-config', getDisplayConfig(getFullscreenTargetDisplay()));
  audioMeterService.setIntervalMs(getOptimalAudioIntervalMs());
}

function syncAudioMeterActivity() {
  const s = settingsStore.get();
  const overlayNeedsAudio = Boolean(s.enabled && s.mode === 'Music Sync');
  const fullscreenNeedsAudio = Boolean(fullscreenWindow && !fullscreenWindow.isDestroyed());
  audioMeterService.setPaused(!overlayNeedsAudio && !fullscreenNeedsAudio);
}

function computeFlyoutBounds(anchorBounds) {
  const margin = 10;
  const winW = 376;

  let anchor = null;
  if (anchorBounds && anchorBounds.width > 0 && anchorBounds.height > 0) {
    anchor = anchorBounds;
  } else if (tray && !tray.isDestroyed()) {
    const tb = tray.getBounds();
    if (tb && tb.width > 0 && tb.height > 0) {
      anchor = tb;
    }
  }

  let display;
  if (anchor) {
    display = screen.getDisplayMatching(anchor);
  } else {
    const cursor = screen.getCursorScreenPoint();
    display = screen.getDisplayNearestPoint(cursor);
  }

  const wa = display.workArea;
  const db = display.bounds;
  const winH = Math.min(600, Math.max(440, wa.height - margin * 2));

  // Detect taskbar edge via workArea inset relative to full display bounds
  const taskbarBottom = (wa.y + wa.height) < (db.y + db.height);
  const taskbarTop = wa.y > db.y;
  const taskbarLeft = wa.x > db.x;
  const taskbarRight = (wa.x + wa.width) < (db.x + db.width);

  let x = taskbarLeft ? (wa.x + margin) : (wa.x + wa.width - winW - margin);
  let y = taskbarTop ? (wa.y + margin) : (wa.y + wa.height - winH - margin);

  if (anchor) {
    const anchorCenterX = anchor.x + Math.round(anchor.width / 2);
    const anchorCenterY = anchor.y + Math.round(anchor.height / 2);

    if (taskbarTop) {
      x = anchorCenterX - Math.round(winW / 2);
      y = wa.y + margin;
    } else if (taskbarLeft) {
      x = wa.x + margin;
      y = anchorCenterY - Math.round(winH / 2);
    } else if (taskbarRight) {
      x = wa.x + wa.width - winW - margin;
      y = anchorCenterY - Math.round(winH / 2);
    } else {
      x = anchorCenterX - Math.round(winW / 2);
      y = wa.y + wa.height - winH - margin;
    }
  }

  const clampedX = Math.max(wa.x + margin, Math.min(wa.x + wa.width - winW - margin, x));
  const clampedY = Math.max(wa.y + margin, Math.min(wa.y + wa.height - winH - margin, y));

  return { x: Math.round(clampedX), y: Math.round(clampedY), width: winW, height: winH };
}

function createSettingsWindow() {
  const initialBounds = computeFlyoutBounds(null);

  settingsWindow = new BrowserWindow({
    width: initialBounds.width,
    height: initialBounds.height,
    x: initialBounds.x,
    y: initialBounds.y,
    title: 'Winh Flyout',
    type: 'toolbar',
    frame: false,
    resizable: false,
    movable: false,
    minimizable: false,
    maximizable: false,
    transparent: true,
    alwaysOnTop: true,
    show: false,
    skipTaskbar: true,
    hasShadow: false,
    webPreferences: {
      nodeIntegration: true,
      contextIsolation: false
    }
  });

  try {
    settingsWindow.setAlwaysOnTop(true, 'pop-up-menu');
  } catch {
    settingsWindow.setAlwaysOnTop(true);
  }

  settingsWindow.loadFile(path.join(__dirname, 'src', 'settings.html'));

  settingsWindow.webContents.on('did-finish-load', () => {
    settingsWindow.webContents.send('sync-settings', settingsStore.get());
    settingsWindow.webContents.send('displays-updated', getDisplayInfoList());
    if (latestMedia) {
      settingsWindow.webContents.send('media-update', latestMedia);
    }
    if (settingsWindow.isVisible()) {
      settingsWindow.webContents.send('flyout-opened');
    }
  });

  settingsWindow.on('blur', () => {
    lastBlurTimestamp = Date.now();
    hideFlyoutWindow();
  });

  settingsWindow.on('close', (e) => {
    if (!isQuitting) {
      e.preventDefault();
      hideFlyoutWindow();
    }
  });
}

function showFlyoutWindow(anchorBounds) {
  if (!settingsWindow || settingsWindow.isDestroyed()) {
    createSettingsWindow();
  }
  const bounds = computeFlyoutBounds(anchorBounds);
  settingsWindow.setBounds(bounds);

  if (!settingsWindow.webContents.isLoading()) {
    settingsWindow.webContents.send('sync-settings', settingsStore.get());
    settingsWindow.webContents.send('displays-updated', getDisplayInfoList());
    if (latestMedia) {
      settingsWindow.webContents.send('media-update', latestMedia);
    }
    settingsWindow.webContents.send('flyout-opened');
  }

  settingsWindow.show();
  settingsWindow.focus();
}

function hideFlyoutWindow() {
  if (settingsWindow && !settingsWindow.isDestroyed() && settingsWindow.isVisible()) {
    settingsWindow.hide();
    if (!settingsWindow.webContents.isDestroyed()) {
      settingsWindow.webContents.send('flyout-closed');
    }
  }
}

function applyFullscreenDisplay(win, display) {
  if (!win || win.isDestroyed() || !display) return;
  const target = display.bounds;
  const cur = win.getBounds();
  const sameBounds =
    cur.x === target.x &&
    cur.y === target.y &&
    cur.width === target.width &&
    cur.height === target.height;

  if (win.isFullScreen() && sameBounds) {
    return;
  }

  if (win.isFullScreen()) {
    win.setFullScreen(false);
  }
  win.setBounds(target);
  if (win.isVisible()) {
    win.setFullScreen(true);
  }
}

function createFullscreenWindow() {
  if (fullscreenWindow && !fullscreenWindow.isDestroyed()) {
    return fullscreenWindow;
  }

  const disp = getFullscreenTargetDisplay();
  const { x, y, width, height } = disp.bounds;

  const win = new BrowserWindow({
    x,
    y,
    width,
    height,
    title: 'Winh Fullscreen Lyrics',
    frame: false,
    transparent: false,
    backgroundColor: '#07080d',
    resizable: true,
    movable: false,
    maximizable: true,
    skipTaskbar: false,
    focusable: true,
    show: false,
    fullscreenable: true,
    autoHideMenuBar: true,
    enableLargerThanScreen: true,
    webPreferences: {
      nodeIntegration: true,
      contextIsolation: false,
      backgroundThrottling: false
    }
  });

  win.setMenuBarVisibility(false);
  fullscreenWindow = win;
  win.loadFile(path.join(__dirname, 'src', 'fullscreen.html'));

  win.once('ready-to-show', () => {
    if (!win.isDestroyed()) {
      const targetDisp = getFullscreenTargetDisplay();
      win.setBounds(targetDisp.bounds);
      win.setFullScreen(true);
      win.show();
      win.focus();
    }
  });

  win.webContents.on('did-finish-load', () => {
    sendInitialOverlayState(win.webContents);
    win.webContents.send('audio-level', audioMeterService.getLastLevel());
  });

  win.on('closed', () => {
    if (fullscreenWindow === win) {
      fullscreenWindow = null;
    }
    if (!isQuitting && settingsStore.get().fullscreenLyrics) {
      applySettingsPatch({ fullscreenLyrics: false }, null);
    }
    syncAudioMeterActivity();
  });

  return win;
}

function openFullscreenWindow() {
  hideFlyoutWindow();
  const isNew = !fullscreenWindow || fullscreenWindow.isDestroyed();
  const win = createFullscreenWindow();
  const disp = getFullscreenTargetDisplay();
  if (!isNew && !win.webContents.isLoading()) {
    applyFullscreenDisplay(win, disp);
    if (!win.isFullScreen()) {
      win.setFullScreen(true);
    }
    win.show();
    win.focus();
  }
  syncAudioMeterActivity();
}

function closeFullscreenWindow() {
  if (fullscreenWindow && !fullscreenWindow.isDestroyed()) {
    fullscreenWindow.close();
  }
  syncAudioMeterActivity();
}

function toggleFullscreenLyrics(desiredState) {
  const currentOpen = Boolean(
    settingsStore.get().fullscreenLyrics &&
    fullscreenWindow &&
    !fullscreenWindow.isDestroyed()
  );
  const nextOpen = typeof desiredState === 'boolean' ? desiredState : !currentOpen;

  applySettingsPatch({ fullscreenLyrics: nextOpen }, null);
  if (nextOpen) {
    openFullscreenWindow();
  } else {
    closeFullscreenWindow();
  }
  return nextOpen;
}

function buildTrayImage(settings) {
  const pngBuffer = createTrayPngBuffer(settings.color1, settings.color2, settings.enabled);
  return nativeImage.createFromBuffer(pngBuffer);
}

function refreshTray() {
  if (!tray || tray.isDestroyed()) return;
  const s = settingsStore.get();
  const iconSig = `${s.enabled}:${s.color1}:${s.color2}`;
  if (iconSig !== lastTraySignature) {
    lastTraySignature = iconSig;
    try {
      tray.setImage(buildTrayImage(s));
    } catch {
      // ignore transient tray icon update errors
    }
  }

  tray.setToolTip(
    s.enabled
      ? 'Winh — Ambient Edge Light & Synced Lyrics'
      : 'Winh — Paused (Click tray icon for controls)'
  );

  const contextMenu = Menu.buildFromTemplate([
    {
      label: 'Open Winh',
      click: () => showFlyoutWindow()
    },
    {
      label: 'Fullscreen 3D Lyrics',
      type: 'checkbox',
      checked: Boolean(s.fullscreenLyrics),
      click: (item) => toggleFullscreenLyrics(item.checked)
    },
    {
      label: 'Toggle Mini Widget',
      type: 'checkbox',
      checked: Boolean(s.widgetVisible),
      click: (item) => {
        applySettingsPatch({ widgetVisible: item.checked }, null);
      }
    },
    { type: 'separator' },
    {
      label: s.enabled ? 'Pause Edge Light' : 'Resume Edge Light',
      click: () => {
        applySettingsPatch({ enabled: !settingsStore.get().enabled }, null);
      }
    },
    {
      label: 'Show Synced Lyrics',
      type: 'checkbox',
      checked: s.lyricsVisible !== false,
      click: (item) => {
        applySettingsPatch({ lyricsVisible: item.checked }, null);
      }
    },
    {
      label: 'Auto-Extract Album Colors',
      type: 'checkbox',
      checked: Boolean(s.autoColor),
      click: (item) => {
        applySettingsPatch({ autoColor: item.checked }, null);
      }
    },
    {
      label: 'Reset Widget Position',
      enabled: Boolean(s.widgetPosition),
      click: () => {
        applySettingsPatch({ widgetPosition: null }, null);
      }
    },
    { type: 'separator' },
    {
      label: 'Quit Winh',
      click: () => shutdownApp()
    }
  ]);

  tray.setContextMenu(contextMenu);
}

function createTray() {
  const s = settingsStore.get();
  lastTraySignature = `${s.enabled}:${s.color1}:${s.color2}`;
  tray = new Tray(buildTrayImage(s));
  refreshTray();

  tray.on('click', (_event, bounds) => {
    if (settingsWindow && !settingsWindow.isDestroyed() && settingsWindow.isVisible()) {
      hideFlyoutWindow();
      return;
    }
    if (Date.now() - lastBlurTimestamp < 220) {
      return;
    }
    showFlyoutWindow(bounds);
  });
}

function applySettingsPatch(patch, senderWebContents) {
  const prev = settingsStore.get();
  const next = settingsStore.update(patch);

  if (prev.displayId !== next.displayId) {
    syncOverlayWindows();
    if (fullscreenWindow && !fullscreenWindow.isDestroyed()) {
      applyFullscreenDisplay(fullscreenWindow, getFullscreenTargetDisplay());
    }
    broadcastDisplays();
  }

  broadcastOverlays('settings-updated', next);
  sendToFullscreen('settings-updated', next);

  const isFromSettings = Boolean(
    senderWebContents &&
    settingsWindow &&
    !settingsWindow.isDestroyed() &&
    senderWebContents.id === settingsWindow.webContents.id
  );
  const needsSettingsEcho = !isFromSettings ||
    (patch && typeof patch.autoColor === 'boolean' && patch.autoColor !== prev.autoColor) ||
    prev.color1 !== next.color1 ||
    prev.color2 !== next.color2 ||
    prev.color3 !== next.color3 ||
    prev.color4 !== next.color4 ||
    prev.fullscreenLyrics !== next.fullscreenLyrics;

  if (needsSettingsEcho && settingsWindow && !settingsWindow.isDestroyed()) {
    settingsWindow.webContents.send('sync-settings', next);
  }

  if ((prev.enabled && prev.mode === 'Music Sync') && (!next.enabled || next.mode !== 'Music Sync')) {
    broadcastOverlays('audio-level', { ...ZERO_LEVEL, active: false });
    wasOverlayZeroSent = true;
  }

  syncAudioMeterActivity();
  refreshTray();
  return next;
}

function shutdownApp() {
  if (isQuitting) return;
  isQuitting = true;
  settingsStore.saveSync();
  smtcService.stop();
  audioMeterService.stop();
  if (fullscreenWindow && !fullscreenWindow.isDestroyed()) {
    try { fullscreenWindow.destroy(); } catch {}
  }
  if (settingsWindow && !settingsWindow.isDestroyed()) {
    try { settingsWindow.destroy(); } catch {}
  }
  if (tray && !tray.isDestroyed()) {
    try { tray.destroy(); } catch {}
  }
  app.quit();
}

// IPC Handlers
ipcMain.on('overlay-ready', (e) => {
  sendInitialOverlayState(e.sender);
});

ipcMain.on('fullscreen-ready', (e) => {
  if (!e.sender.isDestroyed()) {
    sendInitialOverlayState(e.sender, getFullscreenTargetDisplay());
    e.sender.send('audio-level', audioMeterService.getLastLevel());
  }
});

ipcMain.on('settings-ready', (e) => {
  if (!e.sender.isDestroyed()) {
    e.sender.send('sync-settings', settingsStore.get());
    e.sender.send('displays-updated', getDisplayInfoList());
    if (latestMedia) {
      e.sender.send('media-update', latestMedia);
    }
  }
});

ipcMain.on('update-settings', (e, patch) => {
  applySettingsPatch(patch, e.sender);
});

ipcMain.on('toggle-fullscreen-lyrics', (_e, arg) => {
  let desired;
  if (typeof arg === 'boolean') {
    desired = arg;
  } else if (arg && typeof arg === 'object' && typeof arg.open === 'boolean') {
    desired = arg.open;
  }
  toggleFullscreenLyrics(desired);
});

ipcMain.on('set-ignore-mouse-events', (e, arg1, arg2) => {
  const win = BrowserWindow.fromWebContents(e.sender);
  if (!win || win.isDestroyed()) return;

  let ignore = true;
  let forward = true;
  if (arg1 && typeof arg1 === 'object') {
    ignore = Boolean(arg1.ignore);
    forward = arg1.forward !== false;
  } else {
    ignore = Boolean(arg1);
    forward = !arg2 || arg2.forward !== false;
  }

  if (ignore) {
    win.setIgnoreMouseEvents(true, { forward });
  } else {
    win.setIgnoreMouseEvents(false);
  }
});

ipcMain.on('media-control', (_e, payload) => {
  const action = payload && typeof payload === 'object' ? payload.action : payload;
  smtcService.sendCommand(action);
});

ipcMain.on('seek-media', (_e, payload) => {
  const seconds = payload && typeof payload === 'object' ? payload.seconds : payload;
  smtcService.seek(seconds);
});

ipcMain.on('hide-settings', () => {
  hideFlyoutWindow();
});

ipcMain.on('open-settings', () => {
  showFlyoutWindow();
});

ipcMain.on('open-flyout', () => {
  showFlyoutWindow();
});

ipcMain.on('quit-app', () => {
  shutdownApp();
});

// App Startup
if (gotSingleInstanceLock) {
  app.whenReady().then(() => {
    const userDataDir = app.getPath('userData');
    settingsStore.init(userDataDir);
    initArtworkCache(userDataDir);

    syncOverlayWindows();

    screen.on('display-added', () => {
      syncOverlayWindows();
      broadcastDisplays();
    });
    screen.on('display-removed', () => {
      syncOverlayWindows();
      if (fullscreenWindow && !fullscreenWindow.isDestroyed()) {
        applyFullscreenDisplay(fullscreenWindow, getFullscreenTargetDisplay());
      }
      broadcastDisplays();
    });
    screen.on('display-metrics-changed', () => {
      syncOverlayWindows();
      if (fullscreenWindow && !fullscreenWindow.isDestroyed()) {
        applyFullscreenDisplay(fullscreenWindow, getFullscreenTargetDisplay());
      }
      broadcastDisplays();
    });

    createSettingsWindow();
    createTray();

    let lastPhaseSyncMs = 0;
    let sharedMusicRot = 0;
    let lastRotTickMs = Date.now();

    // Start lightweight Windows WASAPI system audio level/beat capture
    audioMeterService.on('level', (lvl) => {
      const s = settingsStore.get();
      const isZero = lvl.level <= 0 && lvl.peak <= 0 && lvl.bass <= 0 && lvl.mid <= 0 && lvl.treble <= 0 && !lvl.beat;

      if (s.enabled && s.mode === 'Music Sync') {
        const now = Date.now();
        const dtSec = Math.min(0.1, Math.max(0.001, (now - lastRotTickMs) / 1000));
        lastRotTickMs = now;
        const rotSpeed = Number(s.rotationSpeed) || 1.0;
        const energyRate = 0.45 + (Number(lvl.level) || 0) * 0.85;
        sharedMusicRot = (sharedMusicRot + dtSec * energyRate * rotSpeed) % (Math.PI * 2);

        if (!isZero || !wasOverlayZeroSent) {
          broadcastOverlays('audio-level', lvl);
          wasOverlayZeroSent = isZero;
        }

        if (overlayWindows.size > 1 && (now - lastPhaseSyncMs) >= 1000) {
          lastPhaseSyncMs = now;
          broadcastOverlays('phase-sync', { rot: sharedMusicRot, timestamp: now });
          sendToFullscreen('phase-sync', { rot: sharedMusicRot, timestamp: now });
        }
      } else {
        lastRotTickMs = Date.now();
      }

      if (fullscreenWindow && !fullscreenWindow.isDestroyed()) {
        if (!isZero || !wasFullscreenZeroSent) {
          sendToFullscreen('audio-level', lvl);
          wasFullscreenZeroSent = isZero;
        }
      }
    });
    audioMeterService.on('error-log', (msg) => {
      console.warn('[Winh Audio]', msg);
    });
    audioMeterService.start({ cacheDir: userDataDir, intervalMs: getOptimalAudioIntervalMs() }).then(() => {
      syncAudioMeterActivity();
    });

    // Start Windows SMTC Watcher
    smtcService.on('error-log', (msg) => {
      console.warn('[Winh SMTC]', msg);
    });

    const broadcastMedia = (mediaPayload) => {
      broadcastOverlays('media-update', mediaPayload);
      sendToFullscreen('media-update', mediaPayload);
      if (settingsWindow && !settingsWindow.isDestroyed()) {
        settingsWindow.webContents.send('media-update', mediaPayload);
      }
    };

    const applyArtworkColors = (rawThumb) => {
      const thumbFp = rawThumb ? `${rawThumb.length}:${rawThumb.slice(0, 48)}:${rawThumb.slice(-48)}` : '';
      if (thumbFp === lastArtworkFp) return;
      lastArtworkFp = thumbFp;
      const extracted = rawThumb ? extractPaletteFromBase64(rawThumb) : null;
      const colorUpdate = settingsStore.applyExtractedColors(extracted);
      if (colorUpdate.changed) {
        broadcastOverlays('settings-updated', colorUpdate.settings);
        sendToFullscreen('settings-updated', colorUpdate.settings);
        if (settingsWindow && !settingsWindow.isDestroyed()) {
          settingsWindow.webContents.send('sync-settings', colorUpdate.settings);
        }
        refreshTray();
      }
    };

    smtcService.on('media-changed', async (media) => {
      if (!media || !media.active || !media.title) {
        latestMedia = media ? { ...media, thumbnailSource: '' } : null;
        currentArtworkTrackKey = '';
        lastArtworkLookupKey = '';
        applyArtworkColors('');
        broadcastMedia(latestMedia);

        if (currentTrackKey !== '' || currentLyricsCache !== null) {
          currentTrackKey = '';
          currentLyricsCache = null;
          const emptyLyrics = buildEmptyLyricsPayload();
          broadcastOverlays('lyrics-update', emptyLyrics);
          sendToFullscreen('lyrics-update', emptyLyrics);
        }
        return;
      }

      const durBucket = media.duration > 0 ? Math.round(media.duration / 5) * 5 : 0;
      const trackKey = `${(media.cleanArtist || media.artist || '').toLowerCase()}::${(media.cleanTitle || media.title || '').toLowerCase()}::${durBucket}`;

      let enrichedMedia;
      if (media.thumbnail) {
        enrichedMedia = { ...media, thumbnailSource: 'smtc' };
      } else if (latestMedia && currentArtworkTrackKey === trackKey && latestMedia.thumbnail) {
        enrichedMedia = {
          ...media,
          thumbnail: latestMedia.thumbnail,
          thumbnailMime: latestMedia.thumbnailMime,
          thumbnailDataUrl: latestMedia.thumbnailDataUrl,
          thumbnailSource: latestMedia.thumbnailSource || 'web'
        };
      } else {
        enrichedMedia = { ...media, thumbnailSource: '' };
      }

      latestMedia = enrichedMedia;
      currentArtworkTrackKey = trackKey;
      applyArtworkColors(enrichedMedia.thumbnail);
      broadcastMedia(latestMedia);

      if (!enrichedMedia.thumbnail && trackKey !== lastArtworkLookupKey) {
        lastArtworkLookupKey = trackKey;
        lookupAlbumArtwork({ ...enrichedMedia, trackKey }).then((art) => {
          if (currentArtworkTrackKey !== trackKey || !latestMedia) return;
          if (latestMedia.thumbnail) return;
          if (!art || !art.found || !art.thumbnail) return;

          latestMedia = {
            ...latestMedia,
            thumbnail: art.thumbnail,
            thumbnailMime: art.thumbnailMime,
            thumbnailDataUrl: art.thumbnailDataUrl,
            thumbnailSource: 'web'
          };
          applyArtworkColors(latestMedia.thumbnail);
          broadcastMedia(latestMedia);
        }).catch(() => {});
      }

      if (trackKey !== currentTrackKey) {
        currentTrackKey = trackKey;
        const loadingPayload = {
          trackKey,
          loading: true,
          success: false,
          found: false,
          instrumental: false,
          source: 'none',
          trackName: media.cleanTitle || media.title,
          artistName: media.cleanArtist || media.artist,
          albumName: media.album || '',
          duration: media.duration || 0,
          isSynced: false,
          syncedLyrics: null,
          plainLyrics: null
        };
        currentLyricsCache = loadingPayload;
        broadcastOverlays('lyrics-update', loadingPayload);
        sendToFullscreen('lyrics-update', loadingPayload);

        const lyricsRes = await searchLrclib(media.title, media.artist, {
          duration: media.duration,
          album: media.album,
          trackKey
        });

        if (currentTrackKey !== trackKey) return;

        currentLyricsCache = lyricsRes;
        broadcastOverlays('lyrics-update', lyricsRes);
        sendToFullscreen('lyrics-update', lyricsRes);
      }
    });

    smtcService.start();
  });

  app.on('before-quit', () => {
    isQuitting = true;
    settingsStore.saveSync();
    smtcService.stop();
    audioMeterService.stop();
  });

  process.on('SIGINT', shutdownApp);
  process.on('SIGTERM', shutdownApp);

  app.on('window-all-closed', (e) => {
    if (!isQuitting) {
      e.preventDefault();
    }
  });
}
