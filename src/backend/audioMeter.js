// d:/Antigravity/lumn-windows/src/backend/audioMeter.js
const { spawn, execFile } = require('child_process');
const crypto = require('crypto');
const EventEmitter = require('events');
const fs = require('fs');
const os = require('os');
const path = require('path');
const { resolveUnpackedPath } = require('./paths');

const ZERO_LEVEL = {
  level: 0,
  peak: 0,
  bass: 0,
  mid: 0,
  treble: 0,
  beat: false,
  active: false,
  source: 'fallback'
};

function findCscExe() {
  const winDir = process.env.WINDIR || 'C:\\Windows';
  const candidates = [
    path.join(winDir, 'Microsoft.NET', 'Framework64', 'v4.0.30319', 'csc.exe'),
    path.join(winDir, 'Microsoft.NET', 'Framework', 'v4.0.30319', 'csc.exe')
  ];
  for (const p of candidates) {
    if (fs.existsSync(p)) return p;
  }
  return null;
}

function compileHelper(csPath, cacheDir) {
  return new Promise((resolve) => {
    const cscPath = findCscExe();
    if (!cscPath) {
      return resolve({ ok: false, error: 'csc.exe not found' });
    }

    let source;
    try {
      source = fs.readFileSync(csPath, 'utf8');
    } catch (err) {
      return resolve({ ok: false, error: err.message });
    }

    const hash = crypto.createHash('sha256').update(source).digest('hex').slice(0, 12);
    const outDir = cacheDir || path.join(os.tmpdir(), 'lumn-windows-cache');
    fs.mkdirSync(outDir, { recursive: true });

    const exeName = `lumn-audio-meter-${hash}.exe`;
    const exePath = path.join(outDir, exeName);

    if (fs.existsSync(exePath)) {
      return resolve({ ok: true, exePath });
    }

    // Clean stale previous builds
    try {
      for (const f of fs.readdirSync(outDir)) {
        if (f.startsWith('lumn-audio-meter-') && f.endsWith('.exe') && f !== exeName) {
          try { fs.unlinkSync(path.join(outDir, f)); } catch {}
        }
      }
    } catch {}

    execFile(
      cscPath,
      ['/nologo', '/optimize+', '/target:exe', `/out:${exePath}`, csPath],
      { timeout: 15000, windowsHide: true },
      (err, stdout, stderr) => {
        if (err || !fs.existsSync(exePath)) {
          return resolve({ ok: false, error: (stderr || stdout || (err && err.message) || 'csc failed').trim() });
        }
        resolve({ ok: true, exePath });
      }
    );
  });
}

class AudioMeterService extends EventEmitter {
  constructor() {
    super();
    this.process = null;
    this.isStopped = true;
    this.isPaused = false;
    this.lastLevel = { ...ZERO_LEVEL };
    this.restartTimer = null;
    this.cacheDir = null;
    this.intervalMs = 30;
  }

  async start(options = {}) {
    this.isStopped = false;
    if (options.cacheDir) this.cacheDir = options.cacheDir;
    if (options.intervalMs) this.intervalMs = options.intervalMs;

    if (this.restartTimer) {
      clearTimeout(this.restartTimer);
      this.restartTimer = null;
    }
    if (this.process) return true;

    const csPath = resolveUnpackedPath(path.join(__dirname, 'WasapiAudioMeter.cs'));
    const compiled = await compileHelper(csPath, this.cacheDir);
    if (this.isStopped) return false;

    if (!compiled.ok) {
      this.emit('error-log', `Audio meter compile failed: ${compiled.error}`);
      this.lastLevel = { ...ZERO_LEVEL };
      this.emit('level', this.lastLevel);
      return false;
    }

    const proc = spawn(
      compiled.exePath,
      ['--parent-pid', String(process.pid), '--interval-ms', String(this.intervalMs)],
      {
        stdio: ['pipe', 'pipe', 'pipe'],
        windowsHide: true
      }
    );
    this.process = proc;

    let buffer = '';
    proc.stdout.on('data', (chunk) => {
      buffer += chunk.toString('utf8');
      const lines = buffer.split(/\r?\n/);
      buffer = lines.pop() || '';

      for (const line of lines) {
        if (!line.startsWith('AUDIO_JSON:')) continue;
        try {
          const parsed = JSON.parse(line.slice('AUDIO_JSON:'.length));
          const payload = {
            level: Number(parsed.level) || 0,
            peak: Number(parsed.peak) || 0,
            bass: Number(parsed.bass) || 0,
            mid: Number(parsed.mid) || 0,
            treble: Number(parsed.treble) || 0,
            beat: Boolean(parsed.beat),
            active: Boolean(parsed.active),
            source: String(parsed.source || 'wasapi-loopback')
          };
          this.lastLevel = payload;
          this.emit('level', payload);
        } catch {
          // ignore malformed line
        }
      }
    });

    proc.stderr.on('data', (err) => {
      const msg = err.toString('utf8').trim();
      if (msg) this.emit('error-log', msg);
    });

    proc.on('close', () => {
      if (this.process === proc) {
        this.process = null;
      }
      if (this.isStopped) return;
      this.restartTimer = setTimeout(() => {
        if (!this.isStopped) this.start();
      }, 3000);
    });

    proc.on('error', (err) => {
      this.emit('error-log', err.message);
    });

    if (this.isPaused && proc.stdin && !proc.stdin.destroyed && proc.stdin.writable) {
      try { proc.stdin.write('PAUSE\n'); } catch {}
    }

    return true;
  }

  setPaused(paused) {
    const next = Boolean(paused);
    if (this.isPaused === next) return;
    this.isPaused = next;
    if (next) {
      this.lastLevel = { ...ZERO_LEVEL };
    }
    if (this.process && this.process.stdin && !this.process.stdin.destroyed && this.process.stdin.writable) {
      try {
        this.process.stdin.write(next ? 'PAUSE\n' : 'RESUME\n');
      } catch {}
    }
  }

  getLastLevel() {
    return this.lastLevel;
  }

  stop() {
    this.isStopped = true;
    if (this.restartTimer) {
      clearTimeout(this.restartTimer);
      this.restartTimer = null;
    }
    const proc = this.process;
    this.process = null;
    if (proc) {
      try {
        if (proc.stdin && !proc.stdin.destroyed && proc.stdin.writable) {
          proc.stdin.write('QUIT\n');
          proc.stdin.end();
        }
      } catch {}
      setTimeout(() => {
        try {
          if (!proc.killed) proc.kill();
        } catch {}
      }, 200);
    }
  }
}

module.exports = new AudioMeterService();
module.exports.AudioMeterService = AudioMeterService;
module.exports.ZERO_LEVEL = ZERO_LEVEL;
