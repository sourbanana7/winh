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

function createTrayPngBuffer(color1 = '#7000ff', color2 = '#00d2ff', enabled = true, size = 32) {
  const dim = Math.max(16, Math.min(512, Math.round(size)));
  const stride = dim * 4 + 1;
  const raw = Buffer.alloc(stride * dim);

  const c1 = enabled ? parseHexRgb(color1, [112, 0, 255]) : [130, 130, 145];
  const c2 = enabled ? parseHexRgb(color2, [0, 210, 255]) : [95, 95, 110];

  const scale = dim / 32;
  const cx = (dim - 1) / 2;
  const cy = (dim - 1) / 2;
  const outerR = 13.5 * scale;
  const ringMidR = 9.2 * scale;
  const ringHalfW = 2.6 * scale;
  const aaFeather = Math.max(0.8, 1.4 * scale);

  for (let y = 0; y < dim; y++) {
    const rowStart = y * stride;
    raw[rowStart] = 0; // filter type 0
    for (let x = 0; x < dim; x++) {
      const dx = x - cx;
      const dy = y - cy;
      const dist = Math.sqrt(dx * dx + dy * dy);
      const t = Math.max(0, Math.min(1, (x + y) / (2 * (dim - 1))));

      const rCol = Math.round(c1[0] * (1 - t) + c2[0] * t);
      const gCol = Math.round(c1[1] * (1 - t) + c2[1] * t);
      const bCol = Math.round(c1[2] * (1 - t) + c2[2] * t);

      let r = 0;
      let g = 0;
      let b = 0;
      let a = 0;

      // Soft outer glow halo + crisp dark disc backing + neon ring
      if (dist <= outerR + 1.5 * scale) {
        const discAlpha = Math.max(0, Math.min(1, (outerR + 0.5 * scale - dist) / Math.max(1, scale * 0.7)));
        const ringDist = Math.abs(dist - ringMidR);
        const ringAlpha = Math.max(0, Math.min(1, (ringHalfW + 0.8 * scale - ringDist) / aaFeather));
        const glowAlpha = Math.exp(-Math.pow(ringDist / (4.2 * scale), 2)) * 0.55;

        const baseR = 14;
        const baseG = 14;
        const baseB = 20;

        const blend = Math.min(1, ringAlpha + glowAlpha * 0.6);
        r = Math.round(baseR * (1 - blend) + rCol * blend);
        g = Math.round(baseG * (1 - blend) + gCol * blend);
        b = Math.round(baseB * (1 - blend) + bCol * blend);

        // Brighten core of ring
        if (ringAlpha > 0.7) {
          const core = (ringAlpha - 0.7) / 0.3 * 0.35;
          r = Math.min(255, Math.round(r + (255 - r) * core));
          g = Math.min(255, Math.round(g + (255 - g) * core));
          b = Math.min(255, Math.round(b + (255 - b) * core));
        }

        const totalAlpha = Math.max(discAlpha * 0.94, glowAlpha * (dist <= outerR + 1.5 * scale ? 1 : 0));
        a = Math.round(Math.max(0, Math.min(1, totalAlpha)) * 255);
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
