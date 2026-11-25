// d:/Antigravity/lumn-windows/src/ui/color-wheel.js
(function (root, factory) {
  const utils = (root && root.LumnColorUtils)
    ? root.LumnColorUtils
    : (typeof module !== 'undefined' && module.exports ? require('./color-utils') : null);
  const api = factory(utils);
  if (typeof module !== 'undefined' && module.exports) {
    module.exports = api;
  }
  if (root) {
    root.LumnColorWheel = api;
  }
})(typeof window !== 'undefined' ? window : globalThis, function (ColorUtils) {
  'use strict';

  /**
   * Interactive circular HSV hue/saturation wheel + brightness slider + hex input.
   */
  class LumnColorWheel {
    /**
     * @param {Object} opts
     * @param {HTMLCanvasElement} opts.canvas
     * @param {HTMLInputElement} opts.brightnessSlider
     * @param {HTMLElement} [opts.swatchEl]
     * @param {HTMLInputElement|HTMLElement} [opts.hexEl]
     * @param {string} [opts.initialColor]
     * @param {(hex: string) => void} [opts.onChange]
     */
    constructor(opts) {
      this.canvas = opts.canvas;
      this.ctx = this.canvas.getContext('2d');
      this.brightnessSlider = opts.brightnessSlider;
      this.swatchEl = opts.swatchEl || null;
      this.hexEl = opts.hexEl || null;
      this.onChange = typeof opts.onChange === 'function' ? opts.onChange : () => {};

      this.size = opts.size || 132;
      this.dpr = Math.min(window.devicePixelRatio || 1, 2);
      this.hsv = ColorUtils.hexToHsv(opts.initialColor || '#4b7bec');
      this.isDragging = false;
      this.isSliderDragging = false;
      this._wheelBuffer = null;

      this._setupCanvas();
      this._bindEvents();
      this._renderAll();
    }

    _setupCanvas() {
      const px = Math.round(this.size * this.dpr);
      this.canvas.width = px;
      this.canvas.height = px;
      this.canvas.style.width = `${this.size}px`;
      this.canvas.style.height = `${this.size}px`;
      this._buildWheelBuffer();
    }

    /** Pre-render the HSV disc at full brightness so dragging is instant and zero-lag. */
    _buildWheelBuffer() {
      const px = this.canvas.width;
      const buf = document.createElement('canvas');
      buf.width = px;
      buf.height = px;
      const bctx = buf.getContext('2d');

      const cx = px / 2;
      const cy = px / 2;
      const radius = px / 2 - Math.round(3 * this.dpr);
      const img = bctx.createImageData(px, px);
      const data = img.data;

      for (let y = 0; y < px; y++) {
        for (let x = 0; x < px; x++) {
          const dx = x - cx;
          const dy = y - cy;
          const dist = Math.sqrt(dx * dx + dy * dy);
          const idx = (y * px + x) * 4;

          if (dist <= radius + 1.2 * this.dpr) {
            let angleDeg = (Math.atan2(dy, dx) * 180) / Math.PI;
            if (angleDeg < 0) angleDeg += 360;
            const sat = ColorUtils.clamp(dist / radius, 0, 1);
            const { r, g, b } = ColorUtils.hsvToRgb(angleDeg, sat, 1);

            // Smooth anti-aliased edge
            const edgeDiff = radius - dist;
            const alpha = edgeDiff >= 0 ? 255 : ColorUtils.clamp((1 + edgeDiff / (1.2 * this.dpr)) * 255, 0, 255);

            data[idx] = r;
            data[idx + 1] = g;
            data[idx + 2] = b;
            data[idx + 3] = Math.round(alpha);
          } else {
            data[idx + 3] = 0;
          }
        }
      }

      bctx.putImageData(img, 0, 0);
      this._wheelBuffer = buf;
    }

    _bindEvents() {
      const pickFromPointer = (clientX, clientY) => {
        const rect = this.canvas.getBoundingClientRect();
        const cx = rect.left + rect.width / 2;
        const cy = rect.top + rect.height / 2;
        const radius = rect.width / 2 - 3;

        const dx = clientX - cx;
        const dy = clientY - cy;
        const dist = Math.sqrt(dx * dx + dy * dy);

        let hue = (Math.atan2(dy, dx) * 180) / Math.PI;
        if (hue < 0) hue += 360;
        const sat = ColorUtils.clamp(dist / radius, 0, 1);

        this.hsv.h = hue;
        this.hsv.s = sat;
        if (this.hsv.v < 0.08) this.hsv.v = 1;
        this._renderAll();
        this.onChange(this.getHex());
      };

      this.canvas.addEventListener('pointerdown', (e) => {
        e.preventDefault();
        this.isDragging = true;
        if (this.canvas.setPointerCapture) {
          this.canvas.setPointerCapture(e.pointerId);
        }
        pickFromPointer(e.clientX, e.clientY);
      });

      this.canvas.addEventListener('pointermove', (e) => {
        if (!this.isDragging) return;
        pickFromPointer(e.clientX, e.clientY);
      });

      const stopDrag = (e) => {
        if (!this.isDragging) return;
        this.isDragging = false;
        if (this.canvas.releasePointerCapture && e.pointerId !== undefined) {
          try {
            this.canvas.releasePointerCapture(e.pointerId);
          } catch (_) {
            // ignore if pointer already released
          }
        }
      };

      this.canvas.addEventListener('pointerup', stopDrag);
      this.canvas.addEventListener('pointercancel', stopDrag);

      const stopSlider = () => {
        this.isSliderDragging = false;
      };
      this.brightnessSlider.addEventListener('pointerdown', () => {
        this.isSliderDragging = true;
      });
      this.brightnessSlider.addEventListener('pointerup', stopSlider);
      this.brightnessSlider.addEventListener('pointercancel', stopSlider);
      this.brightnessSlider.addEventListener('blur', stopSlider);

      this.brightnessSlider.addEventListener('input', (e) => {
        const val = parseFloat(e.target.value);
        this.hsv.v = ColorUtils.clamp(val / 100, 0, 1);
        this._renderAll();
        this.onChange(this.getHex());
      });

      if (this.hexEl && this.hexEl.tagName === 'INPUT') {
        const commitHex = () => {
          const parsed = ColorUtils.normalizeHex(this.hexEl.value, '');
          if (parsed) {
            this.setHex(parsed, true);
            this.onChange(this.getHex());
          } else {
            this._syncDisplays();
          }
        };
        this.hexEl.addEventListener('change', commitHex);
        this.hexEl.addEventListener('keydown', (e) => {
          if (e.key === 'Enter') {
            commitHex();
            this.hexEl.blur();
          }
        });
      }
    }

    /** Set current color from a hex string while preserving hue if grayscale or unchanged. */
    setHex(hex, force = false) {
      if ((this.isDragging || this.isSliderDragging) && !force) return;
      const norm = ColorUtils.normalizeHex(hex);
      if (!force && norm === this.getHex()) return;
      const next = ColorUtils.hexToHsv(norm);
      if (next.s === 0 && this.hsv) {
        next.h = this.hsv.h;
        if (next.v === 0) next.s = this.hsv.s;
      }
      this.hsv = next;
      this._renderAll();
    }

    /** Get current hex color (#rrggbb). */
    getHex() {
      return ColorUtils.hsvToHex(this.hsv.h, this.hsv.s, this.hsv.v);
    }

    _renderAll() {
      this._drawWheel();
      this._syncDisplays();
    }

    _drawWheel() {
      const ctx = this.ctx;
      const px = this.canvas.width;
      const cx = px / 2;
      const cy = px / 2;
      const radius = px / 2 - Math.round(3 * this.dpr);

      ctx.clearRect(0, 0, px, px);

      // Draw pre-rendered hue/sat disc
      if (this._wheelBuffer) {
        ctx.drawImage(this._wheelBuffer, 0, 0);
      }

      // Subtle brightness veil so the wheel reflects brightness without going pitch-black
      const veilAlpha = (1 - this.hsv.v) * 0.72;
      if (veilAlpha > 0.01) {
        ctx.save();
        ctx.beginPath();
        ctx.arc(cx, cy, radius, 0, Math.PI * 2);
        ctx.fillStyle = `rgba(6, 7, 11, ${veilAlpha.toFixed(3)})`;
        ctx.fill();
        ctx.restore();
      }

      // Subtle border around wheel
      ctx.save();
      ctx.beginPath();
      ctx.arc(cx, cy, radius, 0, Math.PI * 2);
      ctx.lineWidth = 1 * this.dpr;
      ctx.strokeStyle = 'rgba(255, 255, 255, 0.14)';
      ctx.stroke();
      ctx.restore();

      // Active reticle position
      const rad = (this.hsv.h * Math.PI) / 180;
      const rDist = this.hsv.s * radius;
      const rx = cx + Math.cos(rad) * rDist;
      const ry = cy + Math.sin(rad) * rDist;
      const hex = this.getHex();

      ctx.save();
      // Drop shadow for reticle
      ctx.beginPath();
      ctx.arc(rx, ry, 6.5 * this.dpr, 0, Math.PI * 2);
      ctx.fillStyle = 'rgba(0, 0, 0, 0.55)';
      ctx.fill();

      // Active color fill + crisp white ring
      ctx.beginPath();
      ctx.arc(rx, ry, 5.2 * this.dpr, 0, Math.PI * 2);
      ctx.fillStyle = hex;
      ctx.fill();
      ctx.lineWidth = 2 * this.dpr;
      ctx.strokeStyle = '#ffffff';
      ctx.stroke();
      ctx.restore();
    }

    _syncDisplays() {
      const hex = this.getHex();
      const brightHex = ColorUtils.hsvToHex(this.hsv.h, this.hsv.s, 1);

      this.brightnessSlider.value = String(Math.round(this.hsv.v * 100));
      this.brightnessSlider.style.background = `linear-gradient(90deg, #09090d 0%, ${brightHex} 100%)`;

      if (this.swatchEl) {
        this.swatchEl.style.background = hex;
      }

      if (this.hexEl) {
        if (this.hexEl.tagName === 'INPUT') {
          if (document.activeElement !== this.hexEl) {
            this.hexEl.value = hex.toUpperCase();
          }
        } else {
          this.hexEl.textContent = hex.toUpperCase();
        }
      }
    }
  }

  return LumnColorWheel;
});
