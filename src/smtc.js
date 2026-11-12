// d:/Antigravity/lumn-windows/src/smtc.js
const { spawn } = require('child_process');
const path = require('path');
const EventEmitter = require('events');
const { extractTrackAndArtist } = require('./lyrics');
const { resolveUnpackedPath } = require('./backend/paths');

const YOUTUBE_MUSIC_PWA_IDS = ['cinhimbnkkaeohfgghhklpknlkffjgod'];

function resolveAppName(appId) {
  if (!appId || typeof appId !== 'string') return 'System Media';
  const lower = appId.toLowerCase();

  for (const ytmId of YOUTUBE_MUSIC_PWA_IDS) {
    if (lower.includes(ytmId)) return 'YouTube Music';
  }
  if (lower.includes('spotify')) return 'Spotify';
  if (lower.includes('youtubemusic') || lower.includes('youtube music')) return 'YouTube Music';
  if (lower.includes('msedge') || lower.includes('edge')) return 'Microsoft Edge';
  if (lower.includes('chrome')) return 'Google Chrome';
  if (lower.includes('brave')) return 'Brave';
  if (lower.includes('firefox') || lower.includes('308046b0af4a39cb')) return 'Firefox';
  if (lower.includes('opera')) return 'Opera';
  if (lower.includes('vivaldi')) return 'Vivaldi';
  if (lower.includes('arc.exe') || lower.endsWith('!arc')) return 'Arc';
  if (lower.includes('applemusic') || lower.includes('appleinc.applemusic')) return 'Apple Music';
  if (lower.includes('itunes')) return 'iTunes';
  if (lower.includes('tidal')) return 'TIDAL';
  if (lower.includes('deezer')) return 'Deezer';
  if (lower.includes('soundcloud')) return 'SoundCloud';
  if (lower.includes('vlc')) return 'VLC';
  if (lower.includes('foobar2000')) return 'foobar2000';
  if (lower.includes('musicbee')) return 'MusicBee';
  if (lower.includes('zune') || lower.includes('mediaplayer') || lower.includes('wmplayer')) return 'Media Player';

  // Clean up raw AUMID or executable name
  let cleaned = appId;
  if (cleaned.includes('!')) cleaned = cleaned.split('!').pop();
  if (cleaned.includes('\\')) cleaned = cleaned.split('\\').pop();
  cleaned = cleaned.replace(/\.exe$/i, '').replace(/[_-]+/g, ' ').trim();
  return cleaned || 'System Media';
}

function normalizeMediaPayload(raw) {
  const safe = raw && typeof raw === 'object' ? raw : {};
  const title = typeof safe.title === 'string' ? safe.title : '';
  const artist = typeof safe.artist === 'string' ? safe.artist : '';
  const album = typeof safe.album === 'string' ? safe.album : '';
  const appId = typeof safe.appId === 'string' ? safe.appId : '';
  const status = typeof safe.status === 'string' && safe.status ? safe.status : 'Stopped';
  const active = Boolean(safe.active && (title.trim().length > 0 || status === 'Playing'));

  const extracted = extractTrackAndArtist(title, artist);
  const thumbnail = typeof safe.thumbnail === 'string' ? safe.thumbnail : '';
  const thumbnailMime = thumbnail ? (safe.thumbnailMime || 'image/jpeg') : '';
  const thumbnailDataUrl = thumbnail ? `data:${thumbnailMime};base64,${thumbnail}` : '';

  return {
    active,
    appId,
    appName: resolveAppName(appId),
    title,
    artist,
    album,
    cleanTitle: extracted.cleanTitle || title,
    cleanArtist: extracted.cleanArtist || artist,
    status,
    position: Number.isFinite(Number(safe.position)) ? Math.max(0, Number(safe.position)) : 0,
    duration: Number.isFinite(Number(safe.duration)) ? Math.max(0, Number(safe.duration)) : 0,
    lastUpdatedMs: Number.isFinite(Number(safe.lastUpdatedMs)) ? Number(safe.lastUpdatedMs) : Date.now(),
    thumbnail,
    thumbnailMime,
    thumbnailDataUrl,
    thumbnailSource: thumbnail ? 'smtc' : ''
  };
}

class SmtcService extends EventEmitter {
  constructor() {
    super();
    this.process = null;
    this.currentMedia = null;
    this.isStopped = true;
    this.restartTimer = null;
    this.restartAttempts = 0;
  }

  start() {
    this.isStopped = false;
    if (this.restartTimer) {
      clearTimeout(this.restartTimer);
      this.restartTimer = null;
    }
    if (this.process) return;

    const scriptPath = resolveUnpackedPath(path.join(__dirname, '..', 'smtc_watcher.ps1'));
    const proc = spawn('powershell.exe', [
      '-ExecutionPolicy', 'Bypass',
      '-NoProfile',
      '-File', scriptPath,
      '-ParentPid', String(process.pid),
      '-PollMs', '750'
    ], {
      stdio: ['pipe', 'pipe', 'pipe'],
      windowsHide: true
    });
    this.process = proc;

    let buffer = '';

    proc.stdout.on('data', (chunk) => {
      this.restartAttempts = 0;
      buffer += chunk.toString('utf8');
      const lines = buffer.split(/\r?\n/);
      buffer = lines.pop() || '';

      for (const line of lines) {
        if (line.startsWith('MEDIA_JSON:')) {
          const jsonStr = line.slice('MEDIA_JSON:'.length).trim();
          try {
            const raw = JSON.parse(jsonStr);
            const normalized = normalizeMediaPayload(raw);
            this.currentMedia = normalized;
            this.emit('media-changed', normalized);
          } catch {
            // ignore partial or malformed JSON line
          }
        } else if (line.startsWith('SMTC_ERROR:')) {
          this.emit('error-log', line.slice('SMTC_ERROR:'.length).trim());
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

      this.restartAttempts += 1;
      const delayMs = Math.min(10000, 1500 * Math.pow(1.5, Math.min(this.restartAttempts - 1, 5)));
      this.restartTimer = setTimeout(() => {
        if (!this.isStopped) this.start();
      }, delayMs);
    });

    proc.on('error', (err) => {
      this.emit('error-log', err.message);
    });
  }

  sendCommand(action) {
    if (!this.process || !this.process.stdin || this.process.stdin.destroyed || !this.process.stdin.writable) {
      return false;
    }
    const map = {
      'play-pause': 'play-pause',
      'playpause': 'play-pause',
      'toggle': 'play-pause',
      'play': 'play',
      'pause': 'pause',
      'next': 'next',
      'skip-next': 'next',
      'previous': 'previous',
      'prev': 'previous',
      'skip-previous': 'previous'
    };
    const key = String(action || '').toLowerCase().trim();
    const normalized = map[key];
    if (!normalized) return false;

    try {
      this.process.stdin.write(`CMD:${normalized}\n`);
      return true;
    } catch {
      return false;
    }
  }

  seek(seconds) {
    if (!this.process || !this.process.stdin || this.process.stdin.destroyed || !this.process.stdin.writable) {
      return false;
    }
    const sec = Number(seconds);
    if (!Number.isFinite(sec) || sec < 0) return false;
    const ticks = Math.round(sec * 10000000);
    try {
      this.process.stdin.write(`CMD:seek:${ticks}\n`);
      return true;
    } catch {
      return false;
    }
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
          proc.stdin.write('CMD:quit\n');
          proc.stdin.end();
        }
      } catch {
        // ignore write errors during shutdown
      }
      setTimeout(() => {
        try {
          if (!proc.killed) proc.kill();
        } catch {
          // ignore
        }
      }, 250);
    }
  }
}

const instance = new SmtcService();
instance.resolveAppName = resolveAppName;
instance.normalizeMediaPayload = normalizeMediaPayload;
module.exports = instance;
