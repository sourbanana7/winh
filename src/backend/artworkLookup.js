// d:/Antigravity/lumn-windows/src/backend/artworkLookup.js
const crypto = require('crypto');
const fs = require('fs');
const http = require('http');
const https = require('https');
const path = require('path');
const { extractTrackAndArtist, tokenSimilarity, normalizeForCompare } = require('../lyrics');

const USER_AGENT = 'Winh/1.0 (https://github.com/sourbanana7/winh)';
const MEMORY_CACHE_MAX = 100;
const DISK_CACHE_MAX = 80;
const NEGATIVE_TTL_MS = 10 * 60 * 1000;
const MAX_JSON_BYTES = 1024 * 1024;
const MAX_IMAGE_BYTES = 4 * 1024 * 1024;

const memoryCache = new Map();
const inFlightLookups = new Map();
let diskCacheDir = null;

function initArtworkCache(userDataDir) {
  if (!userDataDir) return;
  diskCacheDir = path.join(userDataDir, 'artwork-cache');
  try {
    fs.mkdirSync(diskCacheDir, { recursive: true });
  } catch {
    diskCacheDir = null;
  }
}

function detectImageMime(buf) {
  if (!Buffer.isBuffer(buf) || buf.length < 4) return '';
  if (buf[0] === 0x89 && buf[1] === 0x50 && buf[2] === 0x4e && buf[3] === 0x47) return 'image/png';
  if (buf[0] === 0xff && buf[1] === 0xd8 && buf[2] === 0xff) return 'image/jpeg';
  if (buf[0] === 0x47 && buf[1] === 0x49 && buf[2] === 0x46) return 'image/gif';
  if (buf[0] === 0x42 && buf[1] === 0x4d) return 'image/bmp';
  if (
    buf.length >= 12 &&
    buf[0] === 0x52 && buf[1] === 0x49 && buf[2] === 0x46 && buf[3] === 0x46 &&
    buf[8] === 0x57 && buf[9] === 0x45 && buf[10] === 0x42 && buf[11] === 0x50
  ) {
    return 'image/webp';
  }
  return '';
}

function requestBuffer(targetUrl, { timeoutMs = 5500, maxBytes = MAX_JSON_BYTES, maxRedirects = 4, accept = '*/*' } = {}) {
  return new Promise((resolve) => {
    let parsedUrl;
    try {
      parsedUrl = new URL(targetUrl);
    } catch {
      return resolve({ ok: false, error: 'invalid_url', buffer: null });
    }

    if (parsedUrl.protocol !== 'https:' && parsedUrl.protocol !== 'http:') {
      return resolve({ ok: false, error: 'unsupported_protocol', buffer: null });
    }

    const transport = parsedUrl.protocol === 'https:' ? https : http;
    const req = transport.request(
      parsedUrl,
      {
        method: 'GET',
        headers: {
          'User-Agent': USER_AGENT,
          'Accept': accept
        },
        timeout: timeoutMs
      },
      (res) => {
        const status = res.statusCode || 0;
        if ([301, 302, 303, 307, 308].includes(status) && res.headers.location) {
          res.resume();
          if (maxRedirects <= 0) {
            return resolve({ ok: false, status, error: 'too_many_redirects', buffer: null });
          }
          let nextUrl;
          try {
            nextUrl = new URL(res.headers.location, parsedUrl).toString();
          } catch {
            return resolve({ ok: false, status, error: 'bad_redirect', buffer: null });
          }
          return resolve(requestBuffer(nextUrl, { timeoutMs, maxBytes, maxRedirects: maxRedirects - 1, accept }));
        }

        if (status >= 400) {
          res.resume();
          return resolve({ ok: false, status, error: `http_${status}`, buffer: null });
        }

        const chunks = [];
        let total = 0;
        let aborted = false;

        res.on('data', (chunk) => {
          if (aborted) return;
          total += chunk.length;
          if (total > maxBytes) {
            aborted = true;
            req.destroy();
            return resolve({ ok: false, status, error: 'size_limit_exceeded', buffer: null });
          }
          chunks.push(chunk);
        });

        res.on('end', () => {
          if (aborted) return;
          resolve({ ok: true, status, buffer: Buffer.concat(chunks) });
        });
      }
    );

    req.on('error', (err) => resolve({ ok: false, error: err.message, buffer: null }));
    req.on('timeout', () => {
      req.destroy();
      resolve({ ok: false, error: 'timeout', buffer: null });
    });
    req.end();
  });
}

async function fetchJson(url) {
  const res = await requestBuffer(url, { timeoutMs: 5000, maxBytes: MAX_JSON_BYTES, accept: 'application/json' });
  if (!res.ok || !res.buffer) return { ok: false, error: res.error || 'fetch_failed', data: null };
  try {
    return { ok: true, data: JSON.parse(res.buffer.toString('utf8')) };
  } catch (err) {
    return { ok: false, error: err.message, data: null };
  }
}

async function downloadImageAsBase64(urlCandidates) {
  const urls = Array.isArray(urlCandidates) ? urlCandidates.filter(Boolean) : [urlCandidates].filter(Boolean);
  for (const url of urls) {
    const res = await requestBuffer(url, {
      timeoutMs: 6500,
      maxBytes: MAX_IMAGE_BYTES,
      accept: 'image/webp,image/png,image/jpeg,image/*,*/*;q=0.8'
    });
    if (!res.ok || !res.buffer) continue;
    const mime = detectImageMime(res.buffer);
    if (!mime) continue;
    const thumbnail = res.buffer.toString('base64');
    return {
      ok: true,
      thumbnail,
      thumbnailMime: mime,
      thumbnailDataUrl: `data:${mime};base64,${thumbnail}`,
      sourceUrl: url
    };
  }
  return { ok: false };
}

function scoreArtworkCandidate({ candTitle, candArtist, candAlbum = '', candDurationSec = 0 }, hypothesis, targetAlbum, targetDuration) {
  let bestTitleSim = 0;
  for (const tv of hypothesis.titleVariants) {
    const sim = tokenSimilarity(candTitle || '', tv);
    if (sim > bestTitleSim) bestTitleSim = sim;
  }

  if (bestTitleSim < 0.45) return -999;
  if (hypothesis.strictMatch && bestTitleSim < 0.65) return -999;

  let bestArtistSim = 0;
  if (hypothesis.artists.length > 0) {
    for (const ac of hypothesis.artists) {
      const sim = tokenSimilarity(candArtist || '', ac);
      if (sim > bestArtistSim) bestArtistSim = sim;
    }
    if (hypothesis.strictMatch && bestArtistSim < 0.65) return -999;
  }

  let score = 0;
  if (bestTitleSim >= 0.99) score += 50;
  else if (bestTitleSim >= 0.75) score += 32;
  else score += 14;

  if (hypothesis.artists.length > 0) {
    if (bestArtistSim >= 0.9) score += 40;
    else if (bestArtistSim >= 0.6) score += 22;
    else if (bestArtistSim >= 0.35) score += 5;
    else score -= 35;
  }

  if (targetAlbum && candAlbum) {
    const albSim = tokenSimilarity(candAlbum, targetAlbum);
    if (albSim >= 0.8) score += 15;
  }

  if (targetDuration > 0 && candDurationSec > 0) {
    const diff = Math.abs(candDurationSec - targetDuration);
    if (diff <= 4) score += 20;
    else if (diff <= 10) score += 10;
    else if (diff > 45) score -= 20;
  }

  const minThreshold = hypothesis.artists.length > 0 ? 45 : 30;
  return score >= minThreshold ? score : -999;
}

async function searchItunes(hypotheses, targetAlbum, targetDuration) {
  const seenTerms = new Set();
  const rankedCandidates = [];

  for (const h of hypotheses) {
    const title = h.titleVariants[0];
    const artist = h.artists[0] || '';
    const terms = artist ? [`${artist} ${title}`, title] : [title];

    for (const term of terms) {
      const cleanTerm = term.trim();
      if (!cleanTerm || seenTerms.has(cleanTerm.toLowerCase())) continue;
      seenTerms.add(cleanTerm.toLowerCase());

      const url = `https://itunes.apple.com/search?term=${encodeURIComponent(cleanTerm)}&entity=song&limit=6`;
      const resp = await fetchJson(url);
      if (!resp.ok || !resp.data || !Array.isArray(resp.data.results)) continue;

      for (const item of resp.data.results) {
        const rawArt = item.artworkUrl100 || item.artworkUrl60;
        if (!rawArt) continue;
        const score = scoreArtworkCandidate(
          {
            candTitle: item.trackName,
            candArtist: item.artistName,
            candAlbum: item.collectionName,
            candDurationSec: item.trackTimeMillis ? item.trackTimeMillis / 1000 : 0
          },
          h,
          targetAlbum,
          targetDuration
        );
        if (score > 0) {
          rankedCandidates.push({
            score,
            matchedTitle: item.trackName || '',
            matchedArtist: item.artistName || '',
            urls: [
              rawArt.replace(/\/\d+x\d+bb\./i, '/600x600bb.'),
              rawArt
            ]
          });
        }
      }

      if (rankedCandidates.some((c) => c.score >= 85)) break;
    }
    if (rankedCandidates.some((c) => c.score >= 85)) break;
  }

  rankedCandidates.sort((a, b) => b.score - a.score);
  for (const cand of rankedCandidates.slice(0, 3)) {
    const img = await downloadImageAsBase64(cand.urls);
    if (img.ok) {
      return {
        found: true,
        source: 'itunes',
        thumbnail: img.thumbnail,
        thumbnailMime: img.thumbnailMime,
        thumbnailDataUrl: img.thumbnailDataUrl,
        matchedTitle: cand.matchedTitle,
        matchedArtist: cand.matchedArtist
      };
    }
  }
  return null;
}

async function searchDeezer(hypotheses, targetAlbum, targetDuration) {
  const seenQueries = new Set();
  const rankedCandidates = [];

  for (const h of hypotheses) {
    const title = h.titleVariants[0];
    const artist = h.artists[0] || '';
    const queries = artist ? [`artist:"${artist}" track:"${title}"`, `${artist} ${title}`] : [title];

    for (const q of queries) {
      const cleanQ = q.trim();
      if (!cleanQ || seenQueries.has(cleanQ.toLowerCase())) continue;
      seenQueries.add(cleanQ.toLowerCase());

      const url = `https://api.deezer.com/search?q=${encodeURIComponent(cleanQ)}&limit=6`;
      const resp = await fetchJson(url);
      if (!resp.ok || !resp.data || !Array.isArray(resp.data.data)) continue;

      for (const item of resp.data.data) {
        const alb = item.album || {};
        const urls = [alb.cover_xl, alb.cover_big, alb.cover_medium].filter(Boolean);
        if (urls.length === 0) continue;

        const score = scoreArtworkCandidate(
          {
            candTitle: item.title || item.title_short,
            candArtist: item.artist && item.artist.name,
            candAlbum: alb.title,
            candDurationSec: item.duration || 0
          },
          h,
          targetAlbum,
          targetDuration
        );
        if (score > 0) {
          rankedCandidates.push({
            score,
            matchedTitle: item.title || '',
            matchedArtist: (item.artist && item.artist.name) || '',
            urls
          });
        }
      }

      if (rankedCandidates.some((c) => c.score >= 85)) break;
    }
    if (rankedCandidates.some((c) => c.score >= 85)) break;
  }

  rankedCandidates.sort((a, b) => b.score - a.score);
  for (const cand of rankedCandidates.slice(0, 3)) {
    const img = await downloadImageAsBase64(cand.urls);
    if (img.ok) {
      return {
        found: true,
        source: 'deezer',
        thumbnail: img.thumbnail,
        thumbnailMime: img.thumbnailMime,
        thumbnailDataUrl: img.thumbnailDataUrl,
        matchedTitle: cand.matchedTitle,
        matchedArtist: cand.matchedArtist
      };
    }
  }
  return null;
}

async function searchMusicBrainz(hypotheses, targetAlbum, targetDuration) {
  for (const h of hypotheses.slice(0, 2)) {
    const title = h.titleVariants[0];
    const artist = h.artists[0] || '';
    if (!title) continue;

    const lucene = artist
      ? `recording:"${title.replace(/"/g, '')}" AND artist:"${artist.replace(/"/g, '')}"`
      : `recording:"${title.replace(/"/g, '')}"`;
    const url = `https://musicbrainz.org/ws/2/recording/?query=${encodeURIComponent(lucene)}&fmt=json&limit=4`;
    const resp = await fetchJson(url);
    if (!resp.ok || !resp.data || !Array.isArray(resp.data.recordings)) continue;

    for (const rec of resp.data.recordings) {
      const creditArtist = Array.isArray(rec['artist-credit']) && rec['artist-credit'][0]
        ? rec['artist-credit'][0].name
        : '';
      const firstRelease = Array.isArray(rec.releases) && rec.releases[0] ? rec.releases[0] : null;
      if (!firstRelease || !firstRelease.id) continue;

      const score = scoreArtworkCandidate(
        {
          candTitle: rec.title,
          candArtist: creditArtist,
          candAlbum: firstRelease.title || '',
          candDurationSec: rec.length ? rec.length / 1000 : 0
        },
        h,
        targetAlbum,
        targetDuration
      );
      if (score <= 0) continue;

      const caaUrl = `https://coverartarchive.org/release/${firstRelease.id}/front-500`;
      const img = await downloadImageAsBase64([caaUrl]);
      if (img.ok) {
        return {
          found: true,
          source: 'musicbrainz',
          thumbnail: img.thumbnail,
          thumbnailMime: img.thumbnailMime,
          thumbnailDataUrl: img.thumbnailDataUrl,
          matchedTitle: rec.title || '',
          matchedArtist: creditArtist
        };
      }
    }
  }
  return null;
}

function getCacheKey(cleanArtist, cleanTitle) {
  return `${normalizeForCompare(cleanArtist)}::${normalizeForCompare(cleanTitle)}`;
}

function setMemoryCache(key, value) {
  if (memoryCache.has(key)) memoryCache.delete(key);
  memoryCache.set(key, { value, storedAt: Date.now() });
  if (memoryCache.size > MEMORY_CACHE_MAX) {
    const oldest = memoryCache.keys().next().value;
    memoryCache.delete(oldest);
  }
}

function getMemoryCache(key) {
  const entry = memoryCache.get(key);
  if (!entry) return null;
  if (!entry.value.found && (Date.now() - entry.storedAt) > NEGATIVE_TTL_MS) {
    memoryCache.delete(key);
    return null;
  }
  memoryCache.delete(key);
  memoryCache.set(key, entry);
  return entry.value.found ? { ...entry.value, source: 'cache' } : entry.value;
}

function getDiskFilePath(cacheKey) {
  if (!diskCacheDir) return null;
  const hash = crypto.createHash('sha256').update(cacheKey).digest('hex').slice(0, 16);
  return path.join(diskCacheDir, `${hash}.json`);
}

function readDiskCache(cacheKey) {
  const filePath = getDiskFilePath(cacheKey);
  if (!filePath || !fs.existsSync(filePath)) return null;
  try {
    const parsed = JSON.parse(fs.readFileSync(filePath, 'utf8'));
    if (parsed && parsed.found && parsed.thumbnail && parsed.thumbnailMime) {
      return {
        ...parsed,
        thumbnailDataUrl: `data:${parsed.thumbnailMime};base64,${parsed.thumbnail}`,
        source: 'cache'
      };
    }
  } catch {
    // ignore corrupt disk entry
  }
  return null;
}

function writeDiskCache(cacheKey, result) {
  const filePath = getDiskFilePath(cacheKey);
  if (!filePath || !result || !result.found || !result.thumbnail) return;
  try {
    fs.mkdirSync(diskCacheDir, { recursive: true });
    const files = fs.readdirSync(diskCacheDir)
      .filter((f) => f.endsWith('.json'))
      .map((f) => {
        const full = path.join(diskCacheDir, f);
        try {
          return { full, mtimeMs: fs.statSync(full).mtimeMs };
        } catch {
          return { full, mtimeMs: 0 };
        }
      })
      .sort((a, b) => a.mtimeMs - b.mtimeMs);

    while (files.length >= DISK_CACHE_MAX) {
      const victim = files.shift();
      try { fs.unlinkSync(victim.full); } catch {}
    }

    fs.writeFileSync(
      filePath,
      JSON.stringify({
        found: true,
        thumbnail: result.thumbnail,
        thumbnailMime: result.thumbnailMime,
        matchedTitle: result.matchedTitle || '',
        matchedArtist: result.matchedArtist || ''
      }),
      'utf8'
    );
  } catch {
    // ignore disk cache write failures
  }
}

async function lookupAlbumArtwork(media = {}) {
  const rawTitle = media.title || media.cleanTitle || '';
  const rawArtist = media.artist || media.cleanArtist || '';
  const targetAlbum = media.album ? String(media.album).trim() : '';
  const targetDuration = Number(media.duration || 0);

  const extracted = extractTrackAndArtist(rawTitle, rawArtist);
  const titleVariants = Array.from(new Set([
    extracted.cleanTitle,
    extracted.strictTitle,
    extracted.rawTrackVariant
  ].filter(Boolean)));

  if (titleVariants.length === 0) {
    return { found: false, source: 'none', thumbnail: '', thumbnailMime: '', thumbnailDataUrl: '' };
  }

  const cacheKey = getCacheKey(extracted.cleanArtist, extracted.cleanTitle);
  const memHit = getMemoryCache(cacheKey);
  if (memHit) return memHit;

  const diskHit = readDiskCache(cacheKey);
  if (diskHit) {
    setMemoryCache(cacheKey, diskHit);
    return diskHit;
  }

  if (inFlightLookups.has(cacheKey)) {
    return inFlightLookups.get(cacheKey);
  }

  const runLookup = async () => {
    const primaryHypotheses = [
      { titleVariants, artists: extracted.artistCandidates, strictMatch: false }
    ];
    for (const fh of extracted.fallbackHypotheses) {
      if (fh.coEqual) {
        primaryHypotheses.push({ titleVariants: [fh.title], artists: fh.artists, strictMatch: true });
      }
    }

    const secondaryHypotheses = extracted.fallbackHypotheses
      .filter((fh) => !fh.coEqual)
      .map((fh) => ({ titleVariants: [fh.title], artists: fh.artists, strictMatch: true }));

    const allHypotheses = [...primaryHypotheses, ...secondaryHypotheses];

    let result = await searchItunes(allHypotheses, targetAlbum, targetDuration);
    if (!result) {
      result = await searchDeezer(allHypotheses, targetAlbum, targetDuration);
    }
    if (!result) {
      result = await searchMusicBrainz(allHypotheses, targetAlbum, targetDuration);
    }

    if (result && result.found) {
      setMemoryCache(cacheKey, result);
      writeDiskCache(cacheKey, result);
      return result;
    }

    const miss = { found: false, source: 'none', thumbnail: '', thumbnailMime: '', thumbnailDataUrl: '' };
    setMemoryCache(cacheKey, miss);
    return miss;
  };

  const promise = runLookup().finally(() => {
    inFlightLookups.delete(cacheKey);
  });
  inFlightLookups.set(cacheKey, promise);
  return promise;
}

function clearArtworkCache() {
  memoryCache.clear();
}

module.exports = {
  initArtworkCache,
  lookupAlbumArtwork,
  clearArtworkCache,
  detectImageMime
};
