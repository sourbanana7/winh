// d:/Antigravity/lumn-windows/src/ui/color-utils.js
(function (root, factory) {
  const api = factory();
  if (typeof module !== 'undefined' && module.exports) {
    module.exports = api;
  }
  if (root) {
    root.LumnColorUtils = api;
  }
})(typeof window !== 'undefined' ? window : globalThis, function () {
  'use strict';

  const CURATED_PRESETS = [
    {
      id: 'studio',
      name: 'Studio White',
      color1: '#e2e8f0',
      color2: '#94a3b8',
      color3: '#cbd5e1',
      color4: '#f8fafc'
    },
    {
      id: 'nordic',
      name: 'Cobalt',
      color1: '#4b7bec',
      color2: '#74b9ff',
      color3: '#3867d6',
      color4: '#a5b1c2'
    },
    {
      id: 'tungsten',
      name: 'Tungsten',
      color1: '#e67e22',
      color2: '#f5cd79',
      color3: '#d35400',
      color4: '#f7d794'
    },
    {
      id: 'dusk',
      name: 'Dusk',
      color1: '#5c6bc0',
      color2: '#e07a5f',
      color3: '#81b29a',
      color4: '#f2cc8f'
    },
    {
      id: 'forest',
      name: 'Forest',
      color1: '#2d6a4f',
      color2: '#74c69d',
      color3: '#52b788',
      color4: '#b7e4c7'
    },
    {
      id: 'ember',
      name: 'Ember',
      color1: '#c0392b',
      color2: '#e17055',
      color3: '#d63031',
      color4: '#fab1a0'
    }
  ];

  /** Clamp a number between min and max. */
  function clamp(val, min, max) {
    return Math.min(max, Math.max(min, val));
  }

  /** Normalize a hex string to #rrggbb lowercase, falling back when invalid. */
  function normalizeHex(hex, fallback = '#4b7bec') {
    if (typeof hex !== 'string') return fallback;
    let clean = hex.trim();
    if (!clean.startsWith('#')) clean = '#' + clean;
    if (/^#[0-9a-fA-F]{3}$/.test(clean)) {
      const r = clean[1];
      const g = clean[2];
      const b = clean[3];
      return `#${r}${r}${g}${g}${b}${b}`.toLowerCase();
    }
    if (/^#[0-9a-fA-F]{6}$/.test(clean)) {
      return clean.toLowerCase();
    }
    return fallback;
  }

  /** Convert #rrggbb hex string to { r, g, b } integers (0..255). */
  function hexToRgb(hex) {
    const norm = normalizeHex(hex);
    const num = parseInt(norm.slice(1), 16);
    return {
      r: (num >> 16) & 255,
      g: (num >> 8) & 255,
      b: num & 255
    };
  }

  /** Convert { r, g, b } (0..255) to #rrggbb lowercase string. */
  function rgbToHex(r, g, b) {
    const toHex = (n) => clamp(Math.round(n), 0, 255).toString(16).padStart(2, '0');
    return `#${toHex(r)}${toHex(g)}${toHex(b)}`;
  }

  /** Convert RGB (0..255) to HSV ({ h: 0..360, s: 0..1, v: 0..1 }). */
  function rgbToHsv(r, g, b) {
    const rn = clamp(r, 0, 255) / 255;
    const gn = clamp(g, 0, 255) / 255;
    const bn = clamp(b, 0, 255) / 255;

    const max = Math.max(rn, gn, bn);
    const min = Math.min(rn, gn, bn);
    const d = max - min;

    let h = 0;
    const s = max === 0 ? 0 : d / max;
    const v = max;

    if (d !== 0) {
      if (max === rn) {
        h = ((gn - bn) / d) % 6;
      } else if (max === gn) {
        h = (bn - rn) / d + 2;
      } else {
        h = (rn - gn) / d + 4;
      }
      h *= 60;
      if (h < 0) h += 360;
    }

    return { h, s, v };
  }

  /** Convert HSV ({ h: 0..360, s: 0..1, v: 0..1 }) to RGB ({ r, g, b } 0..255). */
  function hsvToRgb(h, s, v) {
    const hn = ((h % 360) + 360) % 360;
    const sn = clamp(s, 0, 1);
    const vn = clamp(v, 0, 1);

    const c = vn * sn;
    const x = c * (1 - Math.abs(((hn / 60) % 2) - 1));
    const m = vn - c;

    let r1 = 0;
    let g1 = 0;
    let b1 = 0;

    if (hn < 60) {
      r1 = c; g1 = x; b1 = 0;
    } else if (hn < 120) {
      r1 = x; g1 = c; b1 = 0;
    } else if (hn < 180) {
      r1 = 0; g1 = c; b1 = x;
    } else if (hn < 240) {
      r1 = 0; g1 = x; b1 = c;
    } else if (hn < 300) {
      r1 = x; g1 = 0; b1 = c;
    } else {
      r1 = c; g1 = 0; b1 = x;
    }

    return {
      r: Math.round((r1 + m) * 255),
      g: Math.round((g1 + m) * 255),
      b: Math.round((b1 + m) * 255)
    };
  }

  /** Convert hex string directly to HSV. */
  function hexToHsv(hex) {
    const { r, g, b } = hexToRgb(hex);
    return rgbToHsv(r, g, b);
  }

  /** Convert HSV directly to #rrggbb hex string. */
  function hsvToHex(h, s, v) {
    const { r, g, b } = hsvToRgb(h, s, v);
    return rgbToHex(r, g, b);
  }

  /** Linearly interpolate scalar a -> b by factor t. */
  function lerp(a, b, t) {
    return a + (b - a) * t;
  }

  /** Linearly interpolate two RGB objects by factor t. */
  function lerpRgb(c1, c2, t) {
    return {
      r: lerp(c1.r, c2.r, t),
      g: lerp(c1.g, c2.g, t),
      b: lerp(c1.b, c2.b, t)
    };
  }

  /** Format an RGB object + alpha into CSS rgba(...) string. */
  function rgbaString(rgb, alpha = 1) {
    const a = clamp(alpha, 0, 1);
    return `rgba(${Math.round(rgb.r)}, ${Math.round(rgb.g)}, ${Math.round(rgb.b)}, ${a.toFixed(3)})`;
  }

  /** Shift an RGB color brighter/more luminous for inner rim highlights. */
  function brightenRgb(rgb, amount = 0.25) {
    return {
      r: clamp(rgb.r + (255 - rgb.r) * amount, 0, 255),
      g: clamp(rgb.g + (255 - rgb.g) * amount, 0, 255),
      b: clamp(rgb.b + (255 - rgb.b) * amount, 0, 255)
    };
  }

  return {
    CURATED_PRESETS,
    clamp,
    normalizeHex,
    hexToRgb,
    rgbToHex,
    rgbToHsv,
    hsvToRgb,
    hexToHsv,
    hsvToHex,
    lerp,
    lerpRgb,
    rgbaString,
    brightenRgb
  };
});
