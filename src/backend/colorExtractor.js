// d:/Antigravity/lumn-windows/src/backend/colorExtractor.js

function clamp(v, min, max) {
  return Math.max(min, Math.min(max, v));
}

function rgbToHsv(r, g, b) {
  const rn = r / 255;
  const gn = g / 255;
  const bn = b / 255;
  const max = Math.max(rn, gn, bn);
  const min = Math.min(rn, gn, bn);
  const d = max - min;

  let h = 0;
  if (d > 1e-6) {
    if (max === rn) h = ((gn - bn) / d) % 6;
    else if (max === gn) h = (bn - rn) / d + 2;
    else h = (rn - gn) / d + 4;
    h *= 60;
    if (h < 0) h += 360;
  }
  const s = max <= 1e-6 ? 0 : d / max;
  return { h, s, v: max };
}

function hsvToRgb(h, s, v) {
  const hue = ((h % 360) + 360) % 360;
  const sat = clamp(s, 0, 1);
  const val = clamp(v, 0, 1);
  const c = val * sat;
  const x = c * (1 - Math.abs(((hue / 60) % 2) - 1));
  const m = val - c;

  let rn = 0;
  let gn = 0;
  let bn = 0;
  if (hue < 60) { rn = c; gn = x; }
  else if (hue < 120) { rn = x; gn = c; }
  else if (hue < 180) { gn = c; bn = x; }
  else if (hue < 240) { gn = x; bn = c; }
  else if (hue < 300) { rn = x; bn = c; }
  else { rn = c; bn = x; }

  return {
    r: Math.round((rn + m) * 255),
    g: Math.round((gn + m) * 255),
    b: Math.round((bn + m) * 255)
  };
}

function rgbToHex(r, g, b) {
  const toHex = (n) => clamp(Math.round(n), 0, 255).toString(16).padStart(2, '0');
  return `#${toHex(r)}${toHex(g)}${toHex(b)}`;
}

function colorDistanceSq(a, b) {
  const dr = a.r - b.r;
  const dg = a.g - b.g;
  const db = a.b - b.b;
  // Weighted perceptual distance
  return 2 * dr * dr + 4 * dg * dg + 3 * db * db;
}

function boostForAmbientGlow(rgb) {
  const { h, s, v } = rgbToHsv(rgb.r, rgb.g, rgb.b);
  const boostedS = s > 0.08 ? clamp(Math.max(s, Math.min(0.88, s * 1.25 + 0.12)), 0, 1) : s;
  const boostedV = clamp(Math.max(v, 0.56), 0.56, 0.98);
  return hsvToRgb(h, boostedS, boostedV);
}

function shiftHue(rgb, deg, satScale = 1, valMin = 0.58) {
  const { h, s, v } = rgbToHsv(rgb.r, rgb.g, rgb.b);
  return hsvToRgb(
    (h + deg) % 360,
    clamp(s * satScale, 0.25, 0.95),
    clamp(Math.max(v, valMin), 0.55, 0.96)
  );
}

function extractPaletteFromPixels(buffer, width, height, order = 'bgra') {
  if (!buffer || buffer.length < 4 || width <= 0 || height <= 0) return null;

  const totalPixels = Math.min(width * height, Math.floor(buffer.length / 4));
  const buckets = new Map();
  const fallbackBuckets = new Map();

  for (let p = 0; p < totalPixels; p++) {
    const idx = p * 4;
    const r = order === 'bgra' ? buffer[idx + 2] : buffer[idx];
    const g = buffer[idx + 1];
    const b = order === 'bgra' ? buffer[idx] : buffer[idx + 2];
    const a = buffer[idx + 3];

    if (a < 128) continue;

    const key = ((r >> 4) << 8) | ((g >> 4) << 4) | (b >> 4);
    let fb = fallbackBuckets.get(key);
    if (!fb) {
      fb = { count: 0, sumR: 0, sumG: 0, sumB: 0 };
      fallbackBuckets.set(key, fb);
    }
    fb.count++;
    fb.sumR += r;
    fb.sumG += g;
    fb.sumB += b;

    const { s, v } = rgbToHsv(r, g, b);
    // Filter pure blacks and washed-out whites in main pass
    if (v < 0.09 || (s < 0.08 && v > 0.92)) continue;

    let bk = buckets.get(key);
    if (!bk) {
      bk = { count: 0, sumR: 0, sumG: 0, sumB: 0 };
      buckets.set(key, bk);
    }
    bk.count++;
    bk.sumR += r;
    bk.sumG += g;
    bk.sumB += b;
  }

  const activeMap = buckets.size > 0 ? buckets : fallbackBuckets;
  if (activeMap.size === 0) return null;

  const candidates = [];
  for (const bk of activeMap.values()) {
    const r = bk.sumR / bk.count;
    const g = bk.sumG / bk.count;
    const b = bk.sumB / bk.count;
    const { s, v } = rgbToHsv(r, g, b);
    const weight = bk.count * (0.3 + 2.1 * Math.pow(s, 1.25)) * (0.45 + 0.55 * v);
    candidates.push({ r, g, b, weight });
  }

  candidates.sort((a, b) => b.weight - a.weight);

  const chosen = [];
  const minDistSqPass1 = 3 * 65 * 65; // ~12675 weighted distance
  for (const c of candidates) {
    if (chosen.every((sel) => colorDistanceSq(sel, c) >= minDistSqPass1)) {
      chosen.push(c);
      if (chosen.length >= 4) break;
    }
  }

  if (chosen.length < 4) {
    const minDistSqPass2 = 3 * 30 * 30;
    for (const c of candidates) {
      if (chosen.includes(c)) continue;
      if (chosen.every((sel) => colorDistanceSq(sel, c) >= minDistSqPass2)) {
        chosen.push(c);
        if (chosen.length >= 4) break;
      }
    }
  }

  if (chosen.length === 0) return null;

  const c1 = boostForAmbientGlow(chosen[0]);
  const c2 = chosen[1] ? boostForAmbientGlow(chosen[1]) : shiftHue(c1, 48, 0.95);
  const c3 = chosen[2] ? boostForAmbientGlow(chosen[2]) : shiftHue(c1, -48, 0.9);
  const c4 = chosen[3] ? boostForAmbientGlow(chosen[3]) : shiftHue(c2, 135, 0.88);

  return {
    color1: rgbToHex(c1.r, c1.g, c1.b),
    color2: rgbToHex(c2.r, c2.g, c2.b),
    color3: rgbToHex(c3.r, c3.g, c3.b),
    color4: rgbToHex(c4.r, c4.g, c4.b)
  };
}

let lastThumbnailFp = '';
let lastPalette = null;

function extractPaletteFromBase64(base64OrDataUrl) {
  if (!base64OrDataUrl || typeof base64OrDataUrl !== 'string') return null;

  const rawB64 = base64OrDataUrl.includes(',')
    ? base64OrDataUrl.split(',').pop().trim()
    : base64OrDataUrl.trim();
  if (!rawB64) return null;

  const fp = `${rawB64.length}:${rawB64.slice(0, 64)}:${rawB64.slice(-64)}`;
  if (fp === lastThumbnailFp && lastPalette) {
    return lastPalette;
  }

  try {
    const electron = require('electron');
    const nativeImage = electron && electron.nativeImage;
    if (!nativeImage || typeof nativeImage.createFromBuffer !== 'function') {
      return null;
    }

    const buf = Buffer.from(rawB64, 'base64');
    if (buf.length === 0) return null;

    let img = nativeImage.createFromBuffer(buf);
    if (!img || img.isEmpty()) return null;

    const size = img.getSize();
    if (size.width > 48 || size.height > 48) {
      const scale = 48 / Math.max(size.width, size.height);
      img = img.resize({
        width: Math.max(1, Math.round(size.width * scale)),
        height: Math.max(1, Math.round(size.height * scale)),
        quality: 'good'
      });
    }

    const resizedSize = img.getSize();
    const bitmap = img.toBitmap();
    const palette = extractPaletteFromPixels(bitmap, resizedSize.width, resizedSize.height, 'bgra');
    if (palette) {
      lastThumbnailFp = fp;
      lastPalette = palette;
    }
    return palette;
  } catch {
    return null;
  }
}

module.exports = {
  extractPaletteFromBase64,
  extractPaletteFromPixels,
  rgbToHex,
  rgbToHsv,
  hsvToRgb
};
