// d:/Antigravity/lumn-windows/src/backend/trayIcon.js
const zlib = require('zlib');

const CRC_TABLE = (() => {
  const table = new Uint32Array(256);
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) {
      c = (c & 1) ? (0xedb88320 ^ (c >>> 1)) : (c >>> 1);
    }
    table[n] = c >>> 0;
  }
  return table;
})();

function crc32(buf) {
  let c = 0xffffffff;
  for (let i = 0; i < buf.length; i++) {
    c = CRC_TABLE[(c ^ buf[i]) & 0xff] ^ (c >>> 8);
  }
  return (c ^ 0xffffffff) >>> 0;
}

function pngChunk(type, data) {
  const lenBuf = Buffer.alloc(4);
  lenBuf.writeUInt32BE(data.length, 0);
  const typeBuf = Buffer.from(type, 'ascii');
  const crcInput = Buffer.concat([typeBuf, data]);
  const crcBuf = Buffer.alloc(4);
  crcBuf.writeUInt32BE(crc32(crcInput), 0);
  return Buffer.concat([lenBuf, typeBuf, data, crcBuf]);
}

function parseHexRgb(hex, fallback) {
  if (typeof hex === 'string' && /^#[0-9a-fA-F]{6}$/.test(hex)) {
    return [
      parseInt(hex.slice(1, 3), 16),
      parseInt(hex.slice(3, 5), 16),
      parseInt(hex.slice(5, 7), 16)
    ];
  }
  return fallback;
}

function sdRoundedRect(px, py, hw, hh, r) {
  const qx = Math.abs(px) - hw + r;
  const qy = Math.abs(py) - hh + r;
  return Math.hypot(Math.max(qx, 0), Math.max(qy, 0)) + Math.min(Math.max(qx, qy), 0) - r;
}

function clamp01(v) {
  return v < 0 ? 0 : (v > 1 ? 1 : v);
}

function createTrayPngBuffer(color1 = '#4b7bec', color2 = '#74b9ff', enabled = true, size = 32) {
  const dim = Math.max(16, Math.min(512, Math.round(size)));
  const stride = dim * 4 + 1;
  const raw = Buffer.alloc(stride * dim);

  const c1 = enabled ? parseHexRgb(color1, [75, 123, 236]) : [132, 138, 150];
  const c2 = enabled ? parseHexRgb(color2, [116, 185, 255]) : [104, 110, 122];

  const scale = dim / 32;
  const cx = (dim - 1) / 2;
  const cy = (dim - 1) / 2;
  const aa = Math.max(0.65, 0.85 * scale);

  // Crisp rounded screen-frame badge
  const outerHw = 14.0 * scale;
  const outerHh = 11.5 * scale;
  const outerR = 3.8 * scale;

  const frameHw = 10.2 * scale;
  const frameHh = 7.7 * scale;
  const frameR = 2.6 * scale;
  const frameHalfStroke = Math.max(0.85, 1.15 * scale);

  for (let y = 0; y < dim; y++) {
    const rowStart = y * stride;
    raw[rowStart] = 0; // filter type 0
    for (let x = 0; x < dim; x++) {
      const dx = x - cx;
      const dy = y - cy;
      const sdOuter = sdRoundedRect(dx, dy, outerHw, outerHh, outerR);

      let r = 0;
      let g = 0;
      let b = 0;
      let a = 0;

      if (sdOuter <= aa) {
        const badgeAlpha = clamp01(0.5 - sdOuter / aa);
        const t = clamp01((x + y) / (2 * (dim - 1)));
        const rCol = c1[0] * (1 - t) + c2[0] * t;
        const gCol = c1[1] * (1 - t) + c2[1] * t;
        const bCol = c1[2] * (1 - t) + c2[2] * t;

        // Matte dark slate backing
        r = 22;
        g = 23;
        b = 28;

        // Subtle outer border hairline so icon reads cleanly on dark taskbars
        const borderBand = clamp01(1 - Math.abs(sdOuter + 0.6 * scale) / (0.85 * scale));
        if (borderBand > 0) {
          const borderMix = borderBand * 0.2;
          r = r * (1 - borderMix) + 255 * borderMix;
          g = g * (1 - borderMix) + 255 * borderMix;
          b = b * (1 - borderMix) + 255 * borderMix;
        }

        // Crisp inner screen bezel frame
        const sdFrame = sdRoundedRect(dx, dy, frameHw, frameHh, frameR);
        const frameStrokeAlpha = clamp01(0.5 + (frameHalfStroke - Math.abs(sdFrame)) / aa);
        if (frameStrokeAlpha > 0) {
          const mixAmt = enabled ? frameStrokeAlpha : frameStrokeAlpha * 0.65;
          r = r * (1 - mixAmt) + rCol * mixAmt;
          g = g * (1 - mixAmt) + gCol * mixAmt;
          b = b * (1 - mixAmt) + bCol * mixAmt;
        }

        r = Math.round(r);
        g = Math.round(g);
        b = Math.round(b);
        a = Math.round(badgeAlpha * 255);
      }

      const px = rowStart + 1 + x * 4;
      raw[px] = r;
      raw[px + 1] = g;
      raw[px + 2] = b;
      raw[px + 3] = a;
    }
  }

  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(dim, 0);
  ihdr.writeUInt32BE(dim, 4);
  ihdr[8] = 8; // bit depth
  ihdr[9] = 6; // RGBA
  ihdr[10] = 0;
  ihdr[11] = 0;
  ihdr[12] = 0;

  const idat = zlib.deflateSync(raw);
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    pngChunk('IHDR', ihdr),
    pngChunk('IDAT', idat),
    pngChunk('IEND', Buffer.alloc(0))
  ]);
}

module.exports = { createTrayPngBuffer };
