// d:/Antigravity/lumn-windows/src/ui/rim-renderer.js
(function (root, factory) {
  const utils = (root && root.LumnColorUtils)
    ? root.LumnColorUtils
    : (typeof module !== 'undefined' && module.exports ? require('./color-utils') : null);
  const api = factory(utils);
  if (typeof module !== 'undefined' && module.exports) {
    module.exports = api;
  }
  if (root) {
    root.LumnRimRenderer = api;
  }
})(typeof window !== 'undefined' ? window : globalThis, function (ColorUtils) {
  'use strict';

  const VERTEX_SHADER_SRC = `#version 300 es
precision highp float;
in vec2 a_pos;
void main() {
  gl_Position = vec4(a_pos, 0.0, 1.0);
}
`;

  const FRAGMENT_SHADER_SRC = `#version 300 es
precision highp float;

uniform vec2 u_res;
uniform float u_dpr;
uniform float u_thickness;
uniform float u_glow;
uniform float u_radius;
uniform float u_opacity;
uniform float u_masterAlpha;
uniform int u_mode;       // 0 = Music Sync, 1 = Idle Glow, 2 = Static
uniform int u_gradMode;   // 0 = 1C, 1 = 2C, 2 = 3C, 3 = 4C, 4 = Dynamic Aura
uniform vec3 u_c1;
uniform vec3 u_c2;
uniform vec3 u_c3;
uniform vec3 u_c4;
uniform float u_rot;
uniform float u_time;
uniform float u_phase1;
uniform float u_phase2;
uniform float u_phase3;
uniform float u_energy;
uniform float u_bass;
uniform float u_mid;
uniform float u_treble;
uniform float u_beat;

out vec4 fragColor;

const float TAU = 6.28318530718;

float sdRoundedBox(vec2 p, vec2 b, float r) {
  vec2 q = abs(p) - b + r;
  return length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - r;
}

vec3 samplePerimeterColor(float t, float orbitAngle) {
  if (u_gradMode == 0) {
    // 1 Color: vivid traveling light wave circling the border
    float crest = 0.5 + 0.5 * cos(orbitAngle);
    float sub = 0.5 + 0.5 * cos(orbitAngle * 2.0);
    float lum = clamp(crest * 0.72 + sub * 0.28, 0.0, 1.0);
    vec3 lo = u_c1 * 0.62;
    vec3 hi = min(vec3(1.0), u_c1 * 1.28 + vec3(0.11));
    return mix(lo, hi, lum);
  }
  if (u_gradMode == 1) {
    // 2 Colors: smooth opposing cosine sweep circling the border
    float f = 0.5 - 0.5 * cos(t * TAU);
    return mix(u_c1, u_c2, f);
  }
  if (u_gradMode == 2) {
    // 3 Colors: continuous smooth 3-stop loop
    float s = fract(t) * 3.0;
    float idx = floor(s);
    float f = smoothstep(0.0, 1.0, fract(s));
    if (idx < 0.5) return mix(u_c1, u_c2, f);
    if (idx < 1.5) return mix(u_c2, u_c3, f);
    return mix(u_c3, u_c1, f);
  }

  // 4 Colors & Dynamic Aura: smooth 4-stop perimeter flow
  float s4 = fract(t) * 4.0;
  float idx4 = floor(s4);
  float f4 = smoothstep(0.0, 1.0, fract(s4));
  vec3 baseCol;
  if (idx4 < 0.5) baseCol = mix(u_c1, u_c2, f4);
  else if (idx4 < 1.5) baseCol = mix(u_c2, u_c3, f4);
  else if (idx4 < 2.5) baseCol = mix(u_c3, u_c4, f4);
  else baseCol = mix(u_c4, u_c1, f4);

  if (u_gradMode == 4) {
    // Organic continuous secondary harmonic color cross-bleed orbiting clockwise
    float crossWave = 0.5 + 0.5 * sin(orbitAngle * 2.0 - u_phase2 * 0.45);
    float trebleSpark = 0.5 + 0.5 * cos(orbitAngle * 3.0 + u_phase3 * 0.5);
    vec3 secondaryMix = mix(u_c3, u_c4, crossWave);
    float bleedAmt = 0.18 + 0.24 * u_mid + 0.14 * u_treble * trebleSpark;
    baseCol = mix(baseCol, secondaryMix, clamp(bleedAmt, 0.0, 0.55));
  }

  return baseCol;
}

void main() {
  float alphaMaster = u_opacity * u_masterAlpha;
  if (alphaMaster <= 0.002) {
    fragColor = vec4(0.0);
    return;
  }

  // Convert fragment coord to top-left origin in device pixels
  vec2 px = vec2(gl_FragCoord.x, u_res.y - gl_FragCoord.y);
  vec2 halfSize = u_res * 0.5;
  vec2 centered = px - halfSize;

  float maxRad = min(halfSize.x, halfSize.y);
  float rad = clamp(u_radius, 0.0, maxRad);
  float sd = sdRoundedBox(centered, halfSize, rad);

  // Cleanly mask outside rounded corner contour with 1px AA feather
  if (sd > 1.0) {
    fragColor = vec4(0.0);
    return;
  }
  float cornerAA = 1.0 - smoothstep(-0.75, 0.75, sd);
  float distIn = max(0.0, -sd);

  // Aspect-normalized perimeter angle (integer harmonic multiples guarantee 0 seam)
  vec2 normDir = centered / max(halfSize, vec2(1.0));
  float theta = (dot(normDir, normDir) > 1e-6) ? atan(normDir.y, normDir.x) : 0.0;
  // Clockwise orbital angle in top-left screen coordinates
  float orbitAngle = theta - u_rot;

  // Continuous organic edge bleed + traveling clockwise perimeter wave
  float wave = 0.0;
  float edgeSparkle = 0.0;
  float orbitLum = 1.0;
  if (u_mode == 0) {
    // Music Sync: traveling clockwise tide + multi-harmonic bass/mid/treble waves
    float travel = 0.22 * cos(orbitAngle) + 0.10 * cos(orbitAngle * 2.0);
    float w1 = sin(orbitAngle * 2.0 + u_phase1 * 0.65) * (0.08 + 0.25 * u_bass);
    float w2 = cos(orbitAngle * 3.0 - u_phase2 * 0.55) * (0.06 + 0.20 * u_mid);
    float w3 = sin(orbitAngle * 5.0 + u_phase3 * 0.55) * (0.03 + 0.13 * u_treble);
    wave = travel + w1 + w2 + w3;
    edgeSparkle = max(0.0, sin(orbitAngle * 4.0 + u_phase3 * 0.5)) * u_treble * 0.22;
    orbitLum = 0.80 + 0.34 * (0.5 + 0.5 * cos(orbitAngle));
  } else if (u_mode == 1) {
    // Idle Glow: serene continuous clockwise wave around all 4 edges
    float travel = 0.20 * cos(orbitAngle);
    float w1 = sin(orbitAngle * 2.0 + u_time * 0.4) * 0.10;
    wave = travel + w1;
    orbitLum = 0.82 + 0.30 * (0.5 + 0.5 * cos(orbitAngle));
  }

  float localThick = max(4.0 * u_dpr, u_thickness * (1.0 + wave + u_beat * 0.30));
  float localGlow = max(6.0 * u_dpr, u_glow * (1.0 + wave * 0.7 + u_beat * 0.28));

  // Early discard for interior pixels well beyond glow tail (keeps GPU work minimal)
  float maxReach = localThick * 2.4 + localGlow * 4.2;
  if (distIn > maxReach) {
    fragColor = vec4(0.0);
    return;
  }

  // Tight luminous bezel core + wide atmospheric wash (continuous exponential decay)
  float core = exp(-distIn / (localThick * 0.52));
  float wash = exp(-pow(distIn / (localThick * 0.65 + localGlow * 1.45), 1.38));
  float hairline = exp(-distIn / max(1.5, 2.2 * u_dpr));

  float t = fract(orbitAngle / TAU);
  vec3 col = samplePerimeterColor(t, orbitAngle);

  // Subtle brightening near the bezel rim, along orbiting crest, and on beat peaks
  float beatBoost = (1.0 + u_beat * 0.26 + u_energy * 0.14 + edgeSparkle) * orbitLum;
  float glowAlpha = clamp((core * 0.68 + wash * 0.46) * alphaMaster * beatBoost, 0.0, 0.96);
  float specAlpha = hairline * alphaMaster * (0.20 + u_beat * 0.22);

  vec3 finalRgb = mix(col, vec3(1.0), clamp(hairline * 0.32 + u_beat * 0.10, 0.0, 0.42));
  float finalAlpha = clamp(glowAlpha + specAlpha * (1.0 - glowAlpha), 0.0, 1.0) * cornerAA;

  // Triangular dither prevents subtle 8-bit gradient banding on dark wallpapers
  float dither = (fract(sin(dot(gl_FragCoord.xy, vec2(12.9898, 78.233))) * 43758.5453) - 0.5) / 255.0;
  finalAlpha = clamp(finalAlpha + dither, 0.0, 1.0);

  // Premultiplied alpha output for Chromium WebGL2 compositing
  fragColor = vec4(finalRgb * finalAlpha, finalAlpha);
}
`;

  function compileShader(gl, type, source) {
    const shader = gl.createShader(type);
    if (!shader) return null;
    gl.shaderSource(shader, source);
    gl.compileShader(shader);
    if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) {
      gl.deleteShader(shader);
      return null;
    }
    return shader;
  }

  function createProgram(gl, vsSrc, fsSrc) {
    const vs = compileShader(gl, gl.VERTEX_SHADER, vsSrc);
    const fs = compileShader(gl, gl.FRAGMENT_SHADER, fsSrc);
    if (!vs || !fs) {
      if (vs) gl.deleteShader(vs);
      if (fs) gl.deleteShader(fs);
      return null;
    }
    const prog = gl.createProgram();
    gl.attachShader(prog, vs);
    gl.attachShader(prog, fs);
    gl.linkProgram(prog);
    gl.deleteShader(vs);
    gl.deleteShader(fs);
    if (!gl.getProgramParameter(prog, gl.LINK_STATUS)) {
      gl.deleteProgram(prog);
      return null;
    }
    return prog;
  }

  function roundedRectPath(ctx, x, y, w, h, r) {
    const radius = Math.max(0, Math.min(r, w / 2, h / 2));
    ctx.beginPath();
    if (typeof ctx.roundRect === 'function') {
      ctx.roundRect(x, y, w, h, radius);
    } else {
      ctx.moveTo(x + radius, y);
      ctx.lineTo(x + w - radius, y);
      ctx.quadraticCurveTo(x + w, y, x + w, y + radius);
      ctx.lineTo(x + w, y + h - radius);
      ctx.quadraticCurveTo(x + w, y + h, x + w - radius, y + h);
      ctx.lineTo(x + radius, y + h);
      ctx.quadraticCurveTo(x, y + h, x, y + h - radius);
      ctx.lineTo(x, y + radius);
      ctx.quadraticCurveTo(x, y, x + radius, y);
      ctx.closePath();
    }
  }

  /**
   * Single-pass WebGL2 signed-distance rim glow renderer with automatic
   * lightweight Canvas2D gradient fallback and smart idle/static sleep.
   */
  class LumnRimRenderer {
    /**
     * @param {HTMLCanvasElement} canvas
     */
    constructor(canvas) {
      this.canvas = canvas;
      this.gl = null;
      this.program = null;
      this.uniforms = {};
      this.ctx2d = null;
      this.backend = 'none';

      this.width = 0;
      this.height = 0;
      this.dpr = 1;

      this.target = {
        enabled: true,
        mode: 'Music Sync',
        gradientMode: '2 Colors',
        thickness: 32,
        glow: 40,
        cornerRadius: 16,
        opacity: 0.85,
        rotationSpeed: 1.0,
        reactivity: 1.0,
        colors: [
          ColorUtils.hexToRgb('#7000ff'),
          ColorUtils.hexToRgb('#00d2ff'),
          ColorUtils.hexToRgb('#ff007f'),
          ColorUtils.hexToRgb('#10b981')
        ]
      };

      this.live = {
        masterAlpha: 1,
        thickness: 32,
        glow: 40,
        cornerRadius: 16,
        opacity: 0.85,
        rotationSpeed: 1.0,
        reactivity: 1.0,
        colors: this.target.colors.map((c) => ({ ...c }))
      };

      this.audio = {
        level: 0,
        peak: 0,
        bass: 0,
        mid: 0,
        treble: 0,
        active: false
      };

      this.smoothAudio = {
        level: 0,
        bass: 0,
        mid: 0,
        treble: 0,
        beatEnv: 0
      };

      this.isMediaPlaying = false;
      this.rot = 0;
      this.time = 0;
      this.phase1 = 0;
      this.phase2 = 0;
      this.phase3 = 0;
      this._dirty = true;
      this._cleared = false;

      this._initContext();
    }

    _initContext() {
      let gl = null;
      try {
        gl = this.canvas.getContext('webgl2', {
          alpha: true,
          premultipliedAlpha: true,
          antialias: false,
          depth: false,
          stencil: false,
          powerPreference: 'low-power'
        });
      } catch {
        gl = null;
      }

      if (gl) {
        const program = createProgram(gl, VERTEX_SHADER_SRC, FRAGMENT_SHADER_SRC);
        if (program) {
          this.gl = gl;
          this.program = program;
          this.backend = 'webgl2';

          const buf = gl.createBuffer();
          gl.bindBuffer(gl.ARRAY_BUFFER, buf);
          gl.bufferData(
            gl.ARRAY_BUFFER,
            new Float32Array([-1, -1, 3, -1, -1, 3]),
            gl.STATIC_DRAW
          );

          const posLoc = gl.getAttribLocation(program, 'a_pos');
          gl.enableVertexAttribArray(posLoc);
          gl.vertexAttribPointer(posLoc, 2, gl.FLOAT, false, 0, 0);

          const uNames = [
            'u_res', 'u_dpr', 'u_thickness', 'u_glow', 'u_radius',
            'u_opacity', 'u_masterAlpha', 'u_mode', 'u_gradMode',
            'u_c1', 'u_c2', 'u_c3', 'u_c4',
            'u_rot', 'u_time', 'u_phase1', 'u_phase2', 'u_phase3',
            'u_energy', 'u_bass', 'u_mid', 'u_treble', 'u_beat'
          ];
          for (const name of uNames) {
            this.uniforms[name] = gl.getUniformLocation(program, name);
          }

          if (typeof this.canvas.addEventListener === 'function') {
            this.canvas.addEventListener('webglcontextlost', (e) => {
              e.preventDefault();
              this._fallbackTo2D();
            });
          }
          return;
        }
      }

      this._fallbackTo2D();
    }

    _fallbackTo2D() {
      this.gl = null;
      this.program = null;
      this.ctx2d = this.canvas.getContext('2d', { alpha: true });
      this.backend = 'canvas2d';
      this._dirty = true;
    }

    /** Resize backing canvas cleanly for monitor DPI & dimensions. */
    resize(width, height, devicePixelRatio = 1) {
      const w = Math.max(1, Math.floor(width));
      const h = Math.max(1, Math.floor(height));
      const dpr = Math.min(devicePixelRatio || 1, 2);

      if (w === this.width && h === this.height && dpr === this.dpr) return;

      this.width = w;
      this.height = h;
      this.dpr = dpr;

      this.canvas.width = Math.round(w * dpr);
      this.canvas.height = Math.round(h * dpr);
      this.canvas.style.width = `${w}px`;
      this.canvas.style.height = `${h}px`;

      if (this.gl) {
        this.gl.viewport(0, 0, this.canvas.width, this.canvas.height);
      }

      this._dirty = true;
      this._cleared = false;
    }

    /** Update settings and wake renderer cleanly. */
    updateSettings(settings) {
      if (!settings) return;
      if (typeof settings.enabled === 'boolean') this.target.enabled = settings.enabled;
      if (settings.mode) this.target.mode = settings.mode;
      if (settings.gradientMode) this.target.gradientMode = settings.gradientMode;
      if (typeof settings.thickness === 'number') this.target.thickness = ColorUtils.clamp(settings.thickness, 4, 120);
      if (typeof settings.glow === 'number') this.target.glow = ColorUtils.clamp(settings.glow, 4, 140);
      if (typeof settings.cornerRadius === 'number') this.target.cornerRadius = ColorUtils.clamp(settings.cornerRadius, 0, 80);
      if (typeof settings.opacity === 'number') this.target.opacity = ColorUtils.clamp(settings.opacity, 0.05, 1.0);
      if (typeof settings.rotationSpeed === 'number') this.target.rotationSpeed = ColorUtils.clamp(settings.rotationSpeed, 0.1, 4.0);
      if (typeof settings.reactivity === 'number') this.target.reactivity = ColorUtils.clamp(settings.reactivity, 0.1, 3.0);

      const keys = ['color1', 'color2', 'color3', 'color4'];
      keys.forEach((key, idx) => {
        if (settings[key]) {
          this.target.colors[idx] = ColorUtils.hexToRgb(settings[key]);
        }
      });

      this._dirty = true;
    }

    /** Update active playback indicator for subtle fallback pulse when loopback is quiet. */
    setMediaPlaying(playing) {
      const next = Boolean(playing);
      if (next !== this.isMediaPlaying) {
        this.isMediaPlaying = next;
        this._dirty = true;
      }
    }

    /** Feed real-time WASAPI audio meter telemetry. */
    pushAudioLevel(data) {
      if (!data) return;
      this.audio.level = ColorUtils.clamp(Number(data.level) || 0, 0, 1);
      this.audio.peak = ColorUtils.clamp(Number(data.peak) || 0, 0, 1);
      this.audio.bass = ColorUtils.clamp(Number(data.bass) || 0, 0, 1);
      this.audio.mid = ColorUtils.clamp(Number(data.mid) || 0, 0, 1);
      this.audio.treble = ColorUtils.clamp(Number(data.treble) || 0, 0, 1);
      this.audio.active = Boolean(data.active);

      if (data.beat) {
        const boost = 0.72 + this.audio.peak * 0.28;
        this.smoothAudio.beatEnv = Math.max(this.smoothAudio.beatEnv, boost);
      }
      this._dirty = true;
    }

    /** Return true when renderer can safely skip frames (disabled fade complete or Static converged). */
    isSettled() {
      if (this._dirty) return false;
      if (!this.target.enabled && this.live.masterAlpha <= 0.004 && this._cleared) {
        return true;
      }
      if (this.target.enabled && this.target.mode === 'Static' && !this._hasPendingLerp()) {
        return true;
      }
      return false;
    }

    _hasPendingLerp() {
      if (Math.abs(this.live.masterAlpha - (this.target.enabled ? 1 : 0)) > 0.004) return true;
      if (Math.abs(this.live.thickness - this.target.thickness) > 0.08) return true;
      if (Math.abs(this.live.glow - this.target.glow) > 0.08) return true;
      if (Math.abs(this.live.cornerRadius - this.target.cornerRadius) > 0.08) return true;
      if (Math.abs(this.live.opacity - this.target.opacity) > 0.003) return true;
      if (Math.abs(this.live.rotationSpeed - this.target.rotationSpeed) > 0.005) return true;
      if (Math.abs(this.live.reactivity - this.target.reactivity) > 0.005) return true;
      if (this.target.mode === 'Music Sync' && (this.smoothAudio.beatEnv > 0.005 || this.smoothAudio.level > 0.005)) {
        return true;
      }
      for (let i = 0; i < 4; i++) {
        const a = this.live.colors[i];
        const b = this.target.colors[i];
        if (Math.abs(a.r - b.r) > 0.4 || Math.abs(a.g - b.g) > 0.4 || Math.abs(a.b - b.b) > 0.4) {
          return true;
        }
      }
      return false;
    }

    /** Advance delta-time normalized state and draw a single pass. */
    render(dtSec) {
      if (this.width <= 0 || this.height <= 0) return;
      if (this.isSettled()) return;

      const dt = Math.min(0.05, Math.max(0.001, dtSec || 0.016));
      const colorK = 1 - Math.exp(-dt * 5.2);
      const paramK = 1 - Math.exp(-dt * 10.0);
      const audioK = 1 - Math.exp(-dt * 18.0);

      const targetAlpha = this.target.enabled ? 1 : 0;
      this.live.masterAlpha = ColorUtils.lerp(this.live.masterAlpha, targetAlpha, 1 - Math.exp(-dt * 8.5));

      if (this.live.masterAlpha <= 0.004 && !this.target.enabled) {
        this.live.masterAlpha = 0;
        if (!this._cleared) {
          this._clearCanvas();
          this._cleared = true;
        }
        this._dirty = false;
        return;
      }
      this._cleared = false;

      this.live.thickness = ColorUtils.lerp(this.live.thickness, this.target.thickness, paramK);
      this.live.glow = ColorUtils.lerp(this.live.glow, this.target.glow, paramK);
      this.live.cornerRadius = ColorUtils.lerp(this.live.cornerRadius, this.target.cornerRadius, paramK);
      this.live.opacity = ColorUtils.lerp(this.live.opacity, this.target.opacity, paramK);
      this.live.rotationSpeed = ColorUtils.lerp(this.live.rotationSpeed, this.target.rotationSpeed, paramK);
      this.live.reactivity = ColorUtils.lerp(this.live.reactivity, this.target.reactivity, paramK);

      for (let i = 0; i < 4; i++) {
        this.live.colors[i] = ColorUtils.lerpRgb(this.live.colors[i], this.target.colors[i], colorK);
      }

      // Exponentially smooth audio bands & beat decay (refresh-rate independent)
      this.smoothAudio.level = ColorUtils.lerp(this.smoothAudio.level, this.audio.level, audioK);
      this.smoothAudio.bass = ColorUtils.lerp(this.smoothAudio.bass, this.audio.bass, audioK);
      this.smoothAudio.mid = ColorUtils.lerp(this.smoothAudio.mid, this.audio.mid, audioK);
      this.smoothAudio.treble = ColorUtils.lerp(this.smoothAudio.treble, this.audio.treble, audioK);
      this.smoothAudio.beatEnv *= Math.exp(-dt * 7.5);
      if (this.smoothAudio.beatEnv < 0.002) this.smoothAudio.beatEnv = 0;

      const mode = this.target.mode;
      const rSpeed = this.live.rotationSpeed;
      const react = this.live.reactivity;
      const phaseScale = 0.4 + 0.6 * rSpeed;

      let energy = ColorUtils.clamp((this.smoothAudio.level * 0.55 + this.smoothAudio.bass * 0.45) * react, 0, 1.4);
      let effBass = ColorUtils.clamp(this.smoothAudio.bass * react, 0, 1.4);
      let effMid = ColorUtils.clamp(this.smoothAudio.mid * react, 0, 1.4);
      let effTreble = ColorUtils.clamp(this.smoothAudio.treble * react, 0, 1.4);
      let effBeat = ColorUtils.clamp(this.smoothAudio.beatEnv * react, 0, 1.4);
      let dynamicAlpha = 1.0;

      if (mode === 'Music Sync') {
        if (this.audio.active) {
          const rotSpeed = (1.15 + energy * 1.45 + effBeat * 0.95) * rSpeed;
          this.rot = (this.rot + dt * rotSpeed) % (Math.PI * 2);
          this.phase1 = (this.phase1 + dt * (1.2 + effBass * 2.2) * phaseScale) % (Math.PI * 2);
          this.phase2 = (this.phase2 + dt * (0.9 + effMid * 1.9) * phaseScale) % (Math.PI * 2);
          this.phase3 = (this.phase3 + dt * (1.5 + effTreble * 2.6) * phaseScale) % (Math.PI * 2);
          const baseFloor = ColorUtils.clamp(0.82 - 0.14 * Math.min(react, 1.2), 0.62, 0.85);
          dynamicAlpha = ColorUtils.clamp(baseFloor + energy * 0.26 + effBeat * 0.20, 0.50, 1.25);
        } else if (this.isMediaPlaying) {
          this.time += dt * phaseScale;
          const wave = 0.5 + 0.5 * Math.sin(this.time * 2.0);
          energy = ColorUtils.clamp((0.22 + wave * 0.14) * react, 0, 1.2);
          effBass = ColorUtils.clamp((0.25 + wave * 0.16) * react, 0, 1.2);
          effMid = ColorUtils.clamp((0.22 + wave * 0.14) * react, 0, 1.2);
          effTreble = ColorUtils.clamp((0.14 + wave * 0.10) * react, 0, 1.2);
          effBeat = ColorUtils.clamp(effBeat, 0, 1.2);
          const rotSpeed = (1.15 + energy * 0.85) * rSpeed;
          this.rot = (this.rot + dt * rotSpeed) % (Math.PI * 2);
          this.phase1 = (this.phase1 + dt * 1.35 * phaseScale) % (Math.PI * 2);
          this.phase2 = (this.phase2 + dt * 1.05 * phaseScale) % (Math.PI * 2);
          this.phase3 = (this.phase3 + dt * 1.55 * phaseScale) % (Math.PI * 2);
          dynamicAlpha = ColorUtils.clamp(0.74 + wave * 0.18 * Math.min(react, 1.5), 0.60, 1.15);
        } else {
          this.time += dt * phaseScale;
          const rest = 0.5 + 0.5 * Math.sin(this.time * 0.9);
          energy = ColorUtils.clamp(0.05 * react, 0, 0.4);
          effBass = ColorUtils.clamp(0.08 * react, 0, 0.4);
          effMid = ColorUtils.clamp(0.06 * react, 0, 0.4);
          effTreble = ColorUtils.clamp(0.03 * react, 0, 0.4);
          this.rot = (this.rot + dt * 0.55 * rSpeed) % (Math.PI * 2);
          this.phase1 = (this.phase1 + dt * 0.6 * phaseScale) % (Math.PI * 2);
          this.phase2 = (this.phase2 + dt * 0.45 * phaseScale) % (Math.PI * 2);
          this.phase3 = (this.phase3 + dt * 0.75 * phaseScale) % (Math.PI * 2);
          dynamicAlpha = 0.52 + rest * 0.12;
        }
      } else if (mode === 'Idle Glow') {
        this.time += dt * phaseScale;
        const breathe = 0.5 + 0.5 * Math.sin(this.time * 1.15);
        this.rot = (this.rot + dt * 0.65 * rSpeed) % (Math.PI * 2);
        this.phase2 = (this.phase2 + dt * 0.55 * phaseScale) % (Math.PI * 2);
        this.phase3 = (this.phase3 + dt * 0.75 * phaseScale) % (Math.PI * 2);
        dynamicAlpha = 0.78 + breathe * 0.20;
        energy = 0;
        effBass = 0;
        effMid = 0.18;
        effTreble = 0.1;
        effBeat = 0;
      } else {
        // Static: smoothly snap audio overlays down & freeze rotation
        energy = 0;
        effBass = 0;
        effMid = 0;
        effTreble = 0;
        effBeat = 0;
        dynamicAlpha = 1.0;
      }

      if (this.backend === 'webgl2' && this.gl && this.program) {
        this._renderWebGL2(energy, effBass, effMid, effTreble, effBeat, dynamicAlpha);
      } else if (this.ctx2d) {
        this._renderCanvas2DFallback(dynamicAlpha);
      }

      this._dirty = mode !== 'Static' || this._hasPendingLerp();
    }

    _clearCanvas() {
      if (this.backend === 'webgl2' && this.gl) {
        const gl = this.gl;
        gl.clearColor(0, 0, 0, 0);
        gl.clear(gl.COLOR_BUFFER_BIT);
      } else if (this.ctx2d) {
        this.ctx2d.clearRect(0, 0, this.canvas.width, this.canvas.height);
      }
    }

    _renderWebGL2(energy, bass, mid, treble, beat, dynamicAlpha) {
      const gl = this.gl;
      const u = this.uniforms;
      const dpr = this.dpr;

      gl.useProgram(this.program);
      gl.uniform2f(u.u_res, this.canvas.width, this.canvas.height);
      gl.uniform1f(u.u_dpr, dpr);
      gl.uniform1f(u.u_thickness, this.live.thickness * dpr);
      gl.uniform1f(u.u_glow, this.live.glow * dpr);
      gl.uniform1f(u.u_radius, this.live.cornerRadius * dpr);
      gl.uniform1f(u.u_opacity, ColorUtils.clamp(this.live.opacity * dynamicAlpha, 0, 1));
      gl.uniform1f(u.u_masterAlpha, this.live.masterAlpha);

      const modeInt = this.target.mode === 'Idle Glow' ? 1 : this.target.mode === 'Static' ? 2 : 0;
      let gradInt = 1;
      if (this.target.gradientMode === '1 Color') gradInt = 0;
      else if (this.target.gradientMode === '3 Colors') gradInt = 2;
      else if (this.target.gradientMode === '4 Colors') gradInt = 3;
      else if (this.target.gradientMode === 'Dynamic Aura') gradInt = 4;

      gl.uniform1i(u.u_mode, modeInt);
      gl.uniform1i(u.u_gradMode, gradInt);

      const c = this.live.colors;
      gl.uniform3f(u.u_c1, c[0].r / 255, c[0].g / 255, c[0].b / 255);
      gl.uniform3f(u.u_c2, c[1].r / 255, c[1].g / 255, c[1].b / 255);
      gl.uniform3f(u.u_c3, c[2].r / 255, c[2].g / 255, c[2].b / 255);
      gl.uniform3f(u.u_c4, c[3].r / 255, c[3].g / 255, c[3].b / 255);

      gl.uniform1f(u.u_rot, this.rot);
      gl.uniform1f(u.u_time, this.time);
      gl.uniform1f(u.u_phase1, this.phase1);
      gl.uniform1f(u.u_phase2, this.phase2);
      gl.uniform1f(u.u_phase3, this.phase3);
      gl.uniform1f(u.u_energy, energy);
      gl.uniform1f(u.u_bass, bass);
      gl.uniform1f(u.u_mid, mid);
      gl.uniform1f(u.u_treble, treble);
      gl.uniform1f(u.u_beat, beat);

      gl.drawArrays(gl.TRIANGLES, 0, 3);
    }

    _renderCanvas2DFallback(dynamicAlpha) {
      const ctx = this.ctx2d;
      const W = this.width;
      const H = this.height;
      const dpr = this.dpr;
      const alpha = ColorUtils.clamp(this.live.opacity * dynamicAlpha * this.live.masterAlpha, 0, 1);

      ctx.save();
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      ctx.clearRect(0, 0, W, H);
      if (alpha <= 0.002) {
        ctx.restore();
        return;
      }

      roundedRectPath(ctx, 0, 0, W, H, this.live.cornerRadius);
      ctx.clip();

      const depth = this.live.thickness + this.live.glow * 0.85;
      const c1 = this.live.colors[0];
      const c2 = this.target.gradientMode === '1 Color' ? ColorUtils.brightenRgb(c1, 0.28) : this.live.colors[1];
      const mixStop = (angleOffset) => {
        const w = 0.5 + 0.5 * Math.cos(angleOffset - this.rot);
        return ColorUtils.lerpRgb(c1, c2, w);
      };

      const tl = mixStop(-Math.PI * 0.75);
      const tr = mixStop(-Math.PI * 0.25);
      const br = mixStop(Math.PI * 0.25);
      const bl = mixStop(Math.PI * 0.75);

      const drawEdge = (x0, y0, x1, y1, rx, ry, rw, rh, cStart, cEnd) => {
        const mid = ColorUtils.lerpRgb(cStart, cEnd, 0.5);
        const g = ctx.createLinearGradient(x0, y0, x1, y1);
        g.addColorStop(0, ColorUtils.rgbaString(mid, alpha * 0.78));
        g.addColorStop(0.35, ColorUtils.rgbaString(mid, alpha * 0.32));
        g.addColorStop(1, ColorUtils.rgbaString(mid, 0));
        ctx.fillStyle = g;
        ctx.fillRect(rx, ry, rw, rh);
      };

      drawEdge(0, 0, 0, depth, 0, 0, W, depth, tl, tr);
      drawEdge(0, H, 0, H - depth, 0, H - depth, W, depth, bl, br);
      drawEdge(0, 0, depth, 0, 0, 0, depth, H, tl, bl);
      drawEdge(W, 0, W - depth, 0, W - depth, 0, depth, H, tr, br);

      ctx.restore();
    }
  }

  return LumnRimRenderer;
});
