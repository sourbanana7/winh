// d:/Antigravity/lumn-windows/src/backend/settingsStore.js
const fs = require('fs');
const path = require('path');

const VALID_MODES = ['Music Sync', 'Idle Glow', 'Static'];
const VALID_GRADIENT_MODES = ['1 Color', '2 Colors', '3 Colors', '4 Colors', 'Dynamic Aura'];
const COLOR_KEYS = ['color1', 'color2', 'color3', 'color4'];

const DEFAULT_SETTINGS = {
  enabled: true,
  mode: 'Music Sync',
  gradientMode: '2 Colors',
  color1: '#7000ff',
  color2: '#00d2ff',
  color3: '#ff007f',
  color4: '#10b981',
  thickness: 32,
  glow: 40,
  cornerRadius: 16,
  opacity: 0.85,
  rotationSpeed: 1.0,
  reactivity: 1.0,
  displayId: 'all',
  widgetVisible: true,
  lyricsVisible: true,
  autoColor: true,
  fullscreenLyrics: false,
  widgetPosition: null,
  extractedColors: null
};

function clampNumber(val, min, max, fallback) {
  const num = Number(val);
  if (!Number.isFinite(num)) return fallback;
  return Math.max(min, Math.min(max, num));
}

function normalizeHex(val, fallback) {
  if (typeof val !== 'string') return fallback;
  const trimmed = val.trim().toLowerCase();
  if (/^#[0-9a-f]{6}$/.test(trimmed)) return trimmed;
  if (/^#[0-9a-f]{3}$/.test(trimmed)) {
    return `#${trimmed[1]}${trimmed[1]}${trimmed[2]}${trimmed[2]}${trimmed[3]}${trimmed[3]}`;
  }
  return fallback;
}

function sanitizeWidgetPosition(pos) {
  if (!pos || typeof pos !== 'object') return null;
  const x = Number(pos.x);
  const y = Number(pos.y);
  if (!Number.isFinite(x) || !Number.isFinite(y)) return null;
  return { x: Math.round(x), y: Math.round(y) };
}

function sanitizePalette(pal, fallback = null) {
  if (!pal || typeof pal !== 'object') return fallback;
  return {
    color1: normalizeHex(pal.color1, DEFAULT_SETTINGS.color1),
    color2: normalizeHex(pal.color2, DEFAULT_SETTINGS.color2),
    color3: normalizeHex(pal.color3, DEFAULT_SETTINGS.color3),
    color4: normalizeHex(pal.color4, DEFAULT_SETTINGS.color4)
  };
}

class SettingsStore {
  constructor() {
    this.filePath = null;
    this.saveTimer = null;
    this.state = { ...DEFAULT_SETTINGS };
    this.manualColors = {
      color1: DEFAULT_SETTINGS.color1,
      color2: DEFAULT_SETTINGS.color2,
      color3: DEFAULT_SETTINGS.color3,
      color4: DEFAULT_SETTINGS.color4
    };
  }

  init(userDataDir) {
    this.filePath = path.join(userDataDir, 'settings.json');
    if (!fs.existsSync(this.filePath)) {
      const legacyPath = path.join(path.dirname(userDataDir), 'lumn-windows', 'settings.json');
      if (fs.existsSync(legacyPath)) {
        try {
          fs.mkdirSync(userDataDir, { recursive: true });
          fs.copyFileSync(legacyPath, this.filePath);
        } catch {}
      }
    }
    this.load();
    return this.get();
  }

  load() {
    if (!this.filePath || !fs.existsSync(this.filePath)) {
      return this.get();
    }
    try {
      const raw = JSON.parse(fs.readFileSync(this.filePath, 'utf8'));
      if (raw && typeof raw === 'object') {
        const merged = { ...DEFAULT_SETTINGS, ...raw, fullscreenLyrics: false, extractedColors: null };
        this.state = this.sanitizeState(merged);
        if (raw.manualColors && typeof raw.manualColors === 'object') {
          this.manualColors = sanitizePalette(raw.manualColors, this.manualColors);
        } else {
          this.manualColors = {
            color1: this.state.color1,
            color2: this.state.color2,
            color3: this.state.color3,
            color4: this.state.color4
          };
        }
        // If autoColor is off on startup, ensure manualColors are active
        if (!this.state.autoColor) {
          Object.assign(this.state, this.manualColors);
        }
      }
    } catch {
      // ignore corrupted file and fall back to defaults
    }
    return this.get();
  }

  sanitizeState(raw) {
    return {
      enabled: typeof raw.enabled === 'boolean' ? raw.enabled : DEFAULT_SETTINGS.enabled,
      mode: VALID_MODES.includes(raw.mode) ? raw.mode : DEFAULT_SETTINGS.mode,
      gradientMode: VALID_GRADIENT_MODES.includes(raw.gradientMode) ? raw.gradientMode : DEFAULT_SETTINGS.gradientMode,
      color1: normalizeHex(raw.color1, DEFAULT_SETTINGS.color1),
      color2: normalizeHex(raw.color2, DEFAULT_SETTINGS.color2),
      color3: normalizeHex(raw.color3, DEFAULT_SETTINGS.color3),
      color4: normalizeHex(raw.color4, DEFAULT_SETTINGS.color4),
      thickness: Math.round(clampNumber(raw.thickness, 8, 72, DEFAULT_SETTINGS.thickness)),
      glow: Math.round(clampNumber(raw.glow, 10, 90, DEFAULT_SETTINGS.glow)),
      cornerRadius: Math.round(clampNumber(raw.cornerRadius, 0, 48, DEFAULT_SETTINGS.cornerRadius)),
      opacity: Number(clampNumber(raw.opacity, 0.1, 1.0, DEFAULT_SETTINGS.opacity).toFixed(2)),
      rotationSpeed: Number(clampNumber(raw.rotationSpeed, 0.1, 4.0, DEFAULT_SETTINGS.rotationSpeed).toFixed(2)),
      reactivity: Number(clampNumber(raw.reactivity, 0.1, 3.0, DEFAULT_SETTINGS.reactivity).toFixed(2)),
      displayId: raw.displayId != null && String(raw.displayId).trim() !== '' ? String(raw.displayId) : 'all',
      widgetVisible: typeof raw.widgetVisible === 'boolean' ? raw.widgetVisible : DEFAULT_SETTINGS.widgetVisible,
      lyricsVisible: typeof raw.lyricsVisible === 'boolean' ? raw.lyricsVisible : DEFAULT_SETTINGS.lyricsVisible,
      autoColor: typeof raw.autoColor === 'boolean' ? raw.autoColor : DEFAULT_SETTINGS.autoColor,
      fullscreenLyrics: typeof raw.fullscreenLyrics === 'boolean' ? raw.fullscreenLyrics : DEFAULT_SETTINGS.fullscreenLyrics,
      widgetPosition: sanitizeWidgetPosition(raw.widgetPosition),
      extractedColors: raw.extractedColors ? sanitizePalette(raw.extractedColors, null) : null
    };
  }

  get() {
    return {
      ...this.state,
      widgetPosition: this.state.widgetPosition ? { ...this.state.widgetPosition } : null,
      extractedColors: this.state.extractedColors ? { ...this.state.extractedColors } : null
    };
  }

  update(patch) {
    if (!patch || typeof patch !== 'object') return this.get();

    const prevAutoColor = this.state.autoColor;
    const prevColors = {
      color1: this.state.color1,
      color2: this.state.color2,
      color3: this.state.color3,
      color4: this.state.color4
    };

    const incomingColors = {};
    let colorsChanged = false;
    for (const k of COLOR_KEYS) {
      if (patch[k] !== undefined) {
        const hex = normalizeHex(patch[k], prevColors[k]);
        incomingColors[k] = hex;
        if (hex !== prevColors[k]) colorsChanged = true;
      }
    }

    // Never allow client IPC to overwrite backend-owned extractedColors
    const safePatch = { ...patch };
    delete safePatch.extractedColors;
    delete safePatch.manualColors;

    const next = this.sanitizeState({ ...this.state, ...safePatch });

    if (patch.autoColor === true && !prevAutoColor) {
      // Switched Auto-Extract ON -> immediately apply extracted palette if available
      next.autoColor = true;
      if (next.extractedColors) {
        Object.assign(next, next.extractedColors);
      }
    } else if (patch.autoColor === false && prevAutoColor) {
      // Switched Auto-Extract OFF -> apply new colors if changed, else restore saved manual colors
      next.autoColor = false;
      if (colorsChanged) {
        for (const k of COLOR_KEYS) {
          if (incomingColors[k]) {
            next[k] = incomingColors[k];
          }
          this.manualColors[k] = next[k];
        }
      } else {
        Object.assign(next, this.manualColors);
      }
    } else if (colorsChanged) {
      // Direct manual color pick or preset selection
      for (const k of COLOR_KEYS) {
        if (incomingColors[k]) {
          next[k] = incomingColors[k];
        }
        this.manualColors[k] = next[k];
      }
      if (prevAutoColor) {
        next.autoColor = false;
      }
    }

    this.state = next;
    this.scheduleSave();
    return this.get();
  }

  applyExtractedColors(palette) {
    const sanitized = palette ? sanitizePalette(palette, null) : null;
    const prevExtracted = this.state.extractedColors;
    this.state.extractedColors = sanitized;

    let activeColorsChanged = false;
    if (this.state.autoColor && sanitized) {
      for (const k of COLOR_KEYS) {
        if (this.state[k] !== sanitized[k]) {
          this.state[k] = sanitized[k];
          activeColorsChanged = true;
        }
      }
    }

    const extractedMetaChanged = JSON.stringify(prevExtracted) !== JSON.stringify(sanitized);
    return {
      changed: activeColorsChanged || extractedMetaChanged,
      settings: this.get()
    };
  }

  scheduleSave() {
    if (!this.filePath) return;
    if (this.saveTimer) clearTimeout(this.saveTimer);
    this.saveTimer = setTimeout(() => {
      this.saveTimer = null;
      this.saveSync();
    }, 200);
  }

  saveSync() {
    if (!this.filePath) return;
    if (this.saveTimer) {
      clearTimeout(this.saveTimer);
      this.saveTimer = null;
    }
    try {
      const dir = path.dirname(this.filePath);
      fs.mkdirSync(dir, { recursive: true });
      const dataToPersist = {
        ...this.get(),
        fullscreenLyrics: false,
        extractedColors: undefined,
        manualColors: { ...this.manualColors }
      };
      const tmpPath = `${this.filePath}.tmp`;
      fs.writeFileSync(tmpPath, JSON.stringify(dataToPersist, null, 2), 'utf8');
      fs.renameSync(tmpPath, this.filePath);
    } catch {
      // ignore disk write failures gracefully
    }
  }
}

module.exports = new SettingsStore();
module.exports.SettingsStore = SettingsStore;
module.exports.DEFAULT_SETTINGS = DEFAULT_SETTINGS;
