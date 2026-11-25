// d:/Antigravity/lumn-windows/src/lyrics.js
const https = require('https');

const LRCLIB_HOST = 'lrclib.net';
const USER_AGENT = 'Winh/1.0 (https://github.com/sourbanana7/winh)';
const CACHE_MAX_ENTRIES = 150;
const NOT_FOUND_TTL_MS = 10 * 60 * 1000;

const lyricsCache = new Map();
const inFlightRequests = new Map();

const GENERIC_PLATFORM_ARTISTS = new Set([
  '',
  'youtube',
  'youtube music',
  'google chrome',
  'chrome',
  'microsoft edge',
  'edge',
  'brave',
  'brave browser',
  'firefox',
  'mozilla firefox',
  'opera',
  'vivaldi',
  'arc',
  'spotify',
  'soundcloud',
  'twitch',
  'vimeo',
  'apple music',
  'itunes',
  'tidal',
  'deezer',
  'unknown',
  'unknown artist',
  'system media',
  'media player',
  'various artists'
]);

function stripEmojisAndNoise(str) {
  if (!str || typeof str !== 'string') return '';
  return str
    .replace(/^\(\d+\)\s*/, '') // leading browser notification count "(3) ..."
    .replace(/\p{Extended_Pictographic}|\uFE0F|\u200D/gu, ' ')
    .replace(/[“”]/g, '"')
    .replace(/[‘’]/g, "'")
    .replace(/#[a-z0-9_]+/gi, ' ') // hashtags like #shorts #music
    .replace(/\s*[-–—|]\s*(YouTube\s*Music|YouTube|Spotify|SoundCloud|Apple\s*Music)\s*$/i, '')
    .replace(/\s+/g, ' ')
    .trim();
}

const BRACKET_NOISE_RE = /^(official|music\s*video|lyric\s*video|official\s*(music\s*)?(video|audio|visualizer|lyric\s*video)|lyrics?|visualizer|audio|m\/?v|hd|hq|4k|8k|60fps|1080p|720p|360p|explicit|clean(\s*version)?|remaster(ed)?(\s*\d{4})?|\d{4}\s*(remaster(ed)?|mix|version)|taylor'?s\s*version|from\s*the\s*vault|\d+\s*minute\s*version|sped\s*up|speed\s*up|slowed(\s*\+\s*reverb)?|nightcore|8d\s*audio|live(\s+.*)?|acoustic(\s+.*)?|studio(\s+.*)?|(radio|club|extended|original|dub|vip)\s*(mix|edit|version)|single\s*version|album\s*version|bonus\s*track|deluxe(\s*edition)?|color\s*coded(\s*lyrics?)?|eng\s*sub|sub\s*español|cc|feat\.?.*|ft\.?.*|featuring.*|with\s+.*|prod\.?.*|full\s*(song|album|video)|\d{4})$/i;

const EDITION_DASH_SUFFIX_RE = /^(?:(?:\d{4}\s*)?(?:remaster(?:ed)?|mix|version|edit)|(?:radio|club|extended|original|dub|vip|clean|explicit|album|single|studio|live|acoustic|instrumental|karaoke|deluxe)\s+(?:mix|edit|version)|(?:\S+(?:\s+\S+){0,3}\s+)?remix|\d+\s*[- ]?\s*minute\s+version|taylor'?s\s+version|from\s+the\s+vault|sped\s*up|speed\s*up|slowed(?:\s*\+\s*reverb)?|reverb|nightcore|8d\s*audio|bass\s*boosted|acoustic(?:\s+version)?|live(?:\s+(?:at|from|in|on)\s+.*)?|demo|instrumental|karaoke|mono|stereo|deluxe(?:\s+edition)?|bonus\s*track|explicit|clean|pt\.?\s*(?:\d+|[ivx]+)|part\s*(?:\d+|[ivx]+)|vol\.?\s*(?:\d+|[ivx]+)|chapter\s*(?:\d+|[ivx]+)|from\s+["']?.*)(?:\s*\d{4})?$/i;

function stripBracketNoise(str) {
  if (!str) return '';
  const out = str.replace(/\s*([(\[【])\s*([^)\]】]+?)\s*([)\]】])/g, (full, _open, inner) => {
    const trimmed = inner.trim();
    if (BRACKET_NOISE_RE.test(trimmed)) return '';
    if (/\b(official|lyrics?|visualizer|audio|music\s*video|remaster(ed)?|taylor'?s\s*version|minute\s*version|sped\s*up|slowed|feat\.?|ft\.?|featuring|prod\.?|4k|hd|mv|explicit)\b/i.test(trimmed)) {
      return '';
    }
    return full;
  });
  return out.replace(/\s+/g, ' ').trim();
}

function stripTrailingEditionDash(str) {
  if (!str) return '';
  let cur = str.trim();
  for (let i = 0; i < 3; i++) {
    const m = cur.match(/^(.+?)\s+[-–—]\s+([^-–—]+)$/);
    if (!m) break;
    if (EDITION_DASH_SUFFIX_RE.test(m[2].trim())) {
      cur = m[1].trim();
    } else {
      break;
    }
  }
  return cur;
}

function stripFeaturedSuffix(str) {
  if (!str) return '';
  return str
    .replace(/\s+(?:feat\.?|ft\.?|featuring|prod\.?\s*(?:by)?)\s+.*$/i, '')
    .trim();
}

function cleanArtistName(artist) {
  if (!artist || typeof artist !== 'string') return '';
  let cleaned = stripEmojisAndNoise(artist)
    .replace(/\s*[-–—]\s*Topic\s*$/i, '')
    .replace(/\s+Official\s*$/i, '')
    .trim();

  if (/vevo$/i.test(cleaned) && cleaned.length > 4) {
    cleaned = cleaned.replace(/vevo$/i, '').trim();
  }
  return cleaned;
}

function isGenericPlatformArtist(artist) {
  return GENERIC_PLATFORM_ARTISTS.has((artist || '').toLowerCase().trim());
}

function splitCamelCase(str) {
  if (!str || /\s/.test(str)) return str;
  return str.replace(/([a-z])([A-Z])/g, '$1 $2').trim();
}

function normalizeForCompare(str) {
  if (!str) return '';
  return str
    .normalize('NFKD')
    .replace(/[\u0300-\u036f]/g, '')
    .toLowerCase()
    .replace(/[^a-z0-9\p{L}\p{N}]+/gu, ' ')
    .trim();
}

function tokenSimilarity(a, b) {
  const na = normalizeForCompare(a);
  const nb = normalizeForCompare(b);
  if (!na || !nb) return 0;
  if (na === nb) return 1.0;
  if (na.includes(nb) || nb.includes(na)) return 0.85;
  const setA = new Set(na.split(' '));
  const setB = new Set(nb.split(' '));
  let overlap = 0;
  for (const t of setA) {
    if (setB.has(t)) overlap++;
  }
  return overlap / Math.max(setA.size, setB.size);
}

function prepareTitleBase(title) {
  let base = stripEmojisAndNoise(title);
  if (base.includes('|')) {
    const segments = base.split('|').map((s) => s.trim()).filter(Boolean);
    if (segments.length > 1) {
      const kept = [];
      for (const seg of segments) {
        if (kept.length === 0 || !/\b(official|lyrics?|video|audio|hd|4k|topic|vevo|music|20\d{2}|19\d{2})\b/i.test(seg)) {
          kept.push(seg);
        }
      }
      base = kept[0] || segments[0];
    }
  }
  base = stripBracketNoise(base);
  return base.replace(/^["'“”「」『』]+|["'“”「」『』]+$/g, '').trim();
}

function buildArtistVariants(artist) {
  if (!artist) return [];
  const primaryArtist = artist
    .split(/\s*(?:,|&|\/|\bx\b|\bfeat\.?\b|\bft\.?\b|\band\b)\s*/i)[0]
    .trim();
  const spaced = splitCamelCase(artist);
  const spacedPrimary = splitCamelCase(primaryArtist);
  return Array.from(new Set([artist, primaryArtist, spaced, spacedPrimary].filter(Boolean)));
}

function extractTrackAndArtist(rawTitle, rawArtist) {
  const cleanedArtistRaw = cleanArtistName(rawArtist);
  const effectiveArtist = isGenericPlatformArtist(cleanedArtistRaw) ? '' : cleanedArtistRaw;

  const baseWithDash = prepareTitleBase(rawTitle);
  const editionStripped = stripTrailingEditionDash(baseWithDash);
  const editionChanged = normalizeForCompare(editionStripped) !== normalizeForCompare(baseWithDash);

  const cornerMatch = editionStripped.match(/^(.+?)\s*[「『](.+?)[」』]\s*$/);
  const dashMatch = !cornerMatch ? editionStripped.match(/^(.+?)\s+[-–—]\s+(.+)$/) : null;

  let cleanTitle = '';
  let cleanArtistVal = effectiveArtist;
  const fallbackHypotheses = [];

  if (cornerMatch) {
    const cornerArtist = cleanArtistName(cornerMatch[1]);
    cleanArtistVal = cornerArtist || effectiveArtist;
    cleanTitle = stripFeaturedSuffix(cornerMatch[2]).trim();
  } else if (dashMatch) {
    const left = stripFeaturedSuffix(dashMatch[1]).replace(/^["']|["']$/g, '').trim();
    const right = stripFeaturedSuffix(dashMatch[2]).replace(/^["']|["']$/g, '').trim();

    if (effectiveArtist) {
      const spacedArtist = splitCamelCase(effectiveArtist);
      const leftSim = Math.max(tokenSimilarity(left, effectiveArtist), tokenSimilarity(left, spacedArtist));
      const rightSim = Math.max(tokenSimilarity(right, effectiveArtist), tokenSimilarity(right, spacedArtist));

      if (leftSim >= 0.65 && leftSim >= rightSim) {
        // Explicit "Artist - Title" matching the reported artist
        cleanArtistVal = left.length >= effectiveArtist.length ? left : effectiveArtist;
        cleanTitle = right;
      } else if (rightSim >= 0.65 && rightSim > leftSim) {
        // Explicit "Title - Artist" matching the reported artist
        cleanArtistVal = effectiveArtist;
        cleanTitle = left;
      } else {
        // Reported artist is non-generic and neither side matches it:
        // preserve the hyphenated track title and keep the reported artist!
        cleanTitle = stripFeaturedSuffix(editionStripped);
        cleanArtistVal = effectiveArtist;
        if (!editionChanged) {
          // Still allow YouTube channel uploader fallback if primary artist search fails
          fallbackHypotheses.push({ title: right, artists: buildArtistVariants(left), strictMatch: true });
          fallbackHypotheses.push({ title: left, artists: buildArtistVariants(right), strictMatch: true });
        }
      }
    } else {
      // No reliable artist from SMTC (empty or generic like "YouTube"):
      // Default to "Artist - Title", and include swapped "Title - Artist" as co-equal hypothesis.
      cleanArtistVal = left;
      cleanTitle = right;
      fallbackHypotheses.push({ title: left, artists: buildArtistVariants(right), strictMatch: true, coEqual: true });
    }
  } else {
    cleanTitle = stripFeaturedSuffix(editionStripped).replace(/^["']|["']$/g, '').trim();
  }

  if (!cleanTitle && rawTitle) {
    cleanTitle = stripEmojisAndNoise(rawTitle);
  }

  const strictTitle = cleanTitle.replace(/\s*\([^)]*\)\s*/g, ' ').replace(/\s+/g, ' ').trim() || cleanTitle;
  const rawTrackVariant = stripFeaturedSuffix(baseWithDash).replace(/^["']|["']$/g, '').trim();
  const artistCandidates = buildArtistVariants(cleanArtistVal);

  return {
    cleanTitle,
    strictTitle,
    rawTrackVariant,
    cleanArtist: cleanArtistVal,
    artistCandidates,
    fallbackHypotheses
  };
}

function cleanSongTitle(title, artist = '') {
  if (!title) return '';
  return extractTrackAndArtist(title, artist).cleanTitle;
}

function cleanArtist(artist, originalTitle = '') {
  if (!originalTitle) return cleanArtistName(artist);
  return extractTrackAndArtist(originalTitle, artist).cleanArtist;
}

function parseLrc(lrcText) {
  if (!lrcText || typeof lrcText !== 'string') return [];

  let offsetSec = 0;
  const offsetMatch = lrcText.match(/\[offset:\s*([+-]?\d+)\s*\]/i);
  if (offsetMatch) {
    offsetSec = parseInt(offsetMatch[1], 10) / 1000;
  }

  const lines = lrcText.split(/\r?\n/);
  const parsed = [];
  const timestampRe = /\[(\d{1,3}):(\d{2})(?:[.:](\d{1,3}))?\]/g;
  const metaTagRe = /^\[(ar|ti|al|au|by|length|offset|re|ve):.*\]$/i;

  for (const rawLine of lines) {
    const trimmed = rawLine.trim();
    if (!trimmed || metaTagRe.test(trimmed)) continue;

    const matches = Array.from(trimmed.matchAll(timestampRe));
    if (matches.length === 0) continue;

    const text = trimmed
      .replace(timestampRe, '')
      .replace(/<\d{1,3}:\d{2}(?:[.:]\d{1,3})?>/g, '')
      .trim();

    if (!text) continue;

    for (const m of matches) {
      const minutes = parseInt(m[1], 10);
      const seconds = parseInt(m[2], 10);
      const fracStr = m[3] || '0';
      const frac = parseFloat('0.' + fracStr);
      const totalSeconds = Math.max(0, minutes * 60 + seconds + frac + offsetSec);
      parsed.push({ time: Number(totalSeconds.toFixed(3)), text });
    }
  }

  return parsed.sort((a, b) => a.time - b.time);
}

function fetchJson(path, timeoutMs = 5000) {
  return new Promise((resolve) => {
    const options = {
      hostname: LRCLIB_HOST,
      path,
      method: 'GET',
      headers: {
        'User-Agent': USER_AGENT,
        'Accept': 'application/json'
      },
      timeout: timeoutMs
    };

    const req = https.request(options, (res) => {
      let data = '';
      res.on('data', (chunk) => { data += chunk; });
      res.on('end', () => {
        if (res.statusCode && res.statusCode >= 400) {
          return resolve({ ok: false, status: res.statusCode, data: null });
        }
        try {
          const parsed = JSON.parse(data);
          resolve({ ok: true, status: res.statusCode, data: parsed });
        } catch (err) {
          resolve({ ok: false, status: res.statusCode, error: err.message, data: null });
        }
      });
    });

    req.on('error', (err) => resolve({ ok: false, error: err.message, data: null }));
    req.on('timeout', () => {
      req.destroy();
      resolve({ ok: false, error: 'timeout', data: null });
    });
    req.end();
  });
}

function scoreCandidateForHypothesis(candidate, titleVariants, artistCandidates, targetDuration, strictMatch = false) {
  if (!candidate) return -999;

  const syncedList = candidate.syncedLyrics ? parseLrc(candidate.syncedLyrics) : [];
  const hasSynced = syncedList.length >= 2;
  const hasPlain = Boolean(candidate.plainLyrics && candidate.plainLyrics.trim().length > 0);
  const isInstrumental = Boolean(candidate.instrumental);

  if (!hasSynced && !hasPlain && !isInstrumental) return -999;

  let bestTitleSim = 0;
  for (const tv of titleVariants) {
    const sim = tokenSimilarity(candidate.trackName || candidate.name || '', tv);
    if (sim > bestTitleSim) bestTitleSim = sim;
  }

  if (strictMatch && bestTitleSim < 0.7) return -999;
  if (bestTitleSim < 0.45) return -999;

  let bestArtistSim = 0;
  if (artistCandidates && artistCandidates.length > 0) {
    for (const ac of artistCandidates) {
      const sim = tokenSimilarity(candidate.artistName || '', ac);
      if (sim > bestArtistSim) bestArtistSim = sim;
    }
    if (strictMatch && bestArtistSim < 0.7) return -999;
  }

  let score = 0;
  if (hasSynced) score += 60;
  else if (hasPlain) score += 25;
  else if (isInstrumental) score += 15;

  if (bestTitleSim >= 0.99) score += 45;
  else if (bestTitleSim >= 0.75) score += 28;
  else score += 12;

  if (artistCandidates && artistCandidates.length > 0) {
    if (bestArtistSim >= 0.9) score += 35;
    else if (bestArtistSim >= 0.6) score += 20;
    else if (bestArtistSim >= 0.35) score += 5;
    else score -= 30;
  }

  if (targetDuration > 0 && candidate.duration > 0) {
    const diff = Math.abs(candidate.duration - targetDuration);
    if (diff <= 2) score += 45;
    else if (diff <= 5) score += 30;
    else if (diff <= 10) score += 15;
    else if (diff <= 20) score += 2;
    else if (diff > 30) score -= 35;
  }

  return score;
}

function buildResultPayload(candidate, trackKey, source = 'lrclib') {
  const syncedList = candidate.syncedLyrics ? parseLrc(candidate.syncedLyrics) : [];
  const isSynced = syncedList.length >= 2;
  const plainLyrics = candidate.plainLyrics ? candidate.plainLyrics.trim() : null;
  const instrumental = Boolean(candidate.instrumental && !isSynced && !plainLyrics);

  return {
    trackKey,
    loading: false,
    success: isSynced || Boolean(plainLyrics) || instrumental,
    found: isSynced || Boolean(plainLyrics) || instrumental,
    instrumental,
    source,
    trackName: candidate.trackName || candidate.name || '',
    artistName: candidate.artistName || '',
    albumName: candidate.albumName || '',
    duration: Number(candidate.duration || 0),
    isSynced,
    syncedLyrics: isSynced ? syncedList : null,
    plainLyrics
  };
}

function setCacheEntry(key, value) {
  if (lyricsCache.has(key)) lyricsCache.delete(key);
  lyricsCache.set(key, { value, storedAt: Date.now() });
  if (lyricsCache.size > CACHE_MAX_ENTRIES) {
    const oldestKey = lyricsCache.keys().next().value;
    lyricsCache.delete(oldestKey);
  }
}

function getCacheEntry(key) {
  const entry = lyricsCache.get(key);
  if (!entry) return null;
  if (!entry.value.success && (Date.now() - entry.storedAt) > NOT_FOUND_TTL_MS) {
    lyricsCache.delete(key);
    return null;
  }
  lyricsCache.delete(key);
  lyricsCache.set(key, entry);
  return { ...entry.value, source: 'cache' };
}

async function searchLrclib(title, artist, options = {}) {
  const targetDuration = Number(options.duration || 0);
  const album = options.album ? String(options.album).trim() : '';
  const extracted = extractTrackAndArtist(title, artist);
  const {
    cleanTitle,
    strictTitle,
    rawTrackVariant,
    cleanArtist: primaryCleanArtist,
    artistCandidates,
    fallbackHypotheses
  } = extracted;

  if (!cleanTitle) {
    return {
      trackKey: '',
      loading: false,
      success: false,
      found: false,
      instrumental: false,
      source: 'none',
      trackName: '',
      artistName: '',
      albumName: '',
      duration: 0,
      isSynced: false,
      syncedLyrics: null,
      plainLyrics: null
    };
  }

  const durBucket = targetDuration > 0 ? Math.round(targetDuration / 5) * 5 : 0;
  const trackKey = options.trackKey || `${normalizeForCompare(primaryCleanArtist)}::${normalizeForCompare(cleanTitle)}::${durBucket}`;

  const cached = getCacheEntry(trackKey);
  if (cached) return cached;

  if (inFlightRequests.has(trackKey)) {
    return inFlightRequests.get(trackKey);
  }

  const runSearch = async () => {
    const allCandidates = new Map();
    let hadNetworkError = false;

    const addResults = (items) => {
      if (!Array.isArray(items)) return;
      for (const item of items) {
        if (item && item.id != null) {
          allCandidates.set(item.id, item);
        }
      }
    };

    const primaryTitleVariants = Array.from(new Set([rawTrackVariant, cleanTitle, strictTitle].filter(Boolean)));
    const activeHypotheses = [
      { titleVariants: primaryTitleVariants, artists: artistCandidates, strictMatch: false }
    ];
    for (const h of fallbackHypotheses) {
      if (h.coEqual) {
        activeHypotheses.push({ titleVariants: [h.title], artists: h.artists, strictMatch: true });
      }
    }

    const pickBest = (hypotheses) => {
      let best = null;
      let bestScore = -999;
      for (const item of allCandidates.values()) {
        for (const h of hypotheses) {
          const s = scoreCandidateForHypothesis(item, h.titleVariants, h.artists, targetDuration, h.strictMatch);
          if (s > bestScore) {
            bestScore = s;
            best = item;
          }
        }
      }
      return { best, bestScore };
    };

    const runQueryBatch = async (queryPaths, evalHypotheses) => {
      for (const qPath of queryPaths) {
        const resp = await fetchJson(qPath);
        if (!resp.ok) {
          if (resp.error) hadNetworkError = true;
          continue;
        }
        if (qPath.startsWith('/api/get?') && resp.data && !Array.isArray(resp.data)) {
          addResults([resp.data]);
        } else {
          addResults(resp.data);
        }
        const { best, bestScore } = pickBest(evalHypotheses);
        if (best && best.syncedLyrics && bestScore >= 120) {
          return { earlyMatch: best, bestScore };
        }
      }
      return { earlyMatch: null, ...pickBest(evalHypotheses) };
    };

    const seenPaths = new Set();
    const makePath = (endpoint, paramsObj) => {
      const p = `${endpoint}?${new URLSearchParams(paramsObj).toString()}`;
      if (seenPaths.has(p)) return null;
      seenPaths.add(p);
      return p;
    };

    const primaryQueries = [];
    const pushPrimary = (endpoint, paramsObj) => {
      const p = makePath(endpoint, paramsObj);
      if (p) primaryQueries.push(p);
    };

    if (primaryCleanArtist && targetDuration > 0) {
      for (const tv of primaryTitleVariants.slice(0, 2)) {
        const getParams = {
          track_name: tv,
          artist_name: primaryCleanArtist,
          duration: String(Math.round(targetDuration))
        };
        if (album) getParams.album_name = album;
        pushPrimary('/api/get', getParams);
      }
    }

    for (const ac of artistCandidates.slice(0, 3)) {
      for (const tv of primaryTitleVariants) {
        pushPrimary('/api/search', { track_name: tv, artist_name: ac });
      }
    }

    for (const h of fallbackHypotheses.filter((fh) => fh.coEqual)) {
      for (const ac of h.artists.slice(0, 2)) {
        pushPrimary('/api/search', { track_name: h.title, artist_name: ac });
      }
    }

    if (primaryCleanArtist) {
      pushPrimary('/api/search', { q: `${primaryCleanArtist} ${strictTitle}` });
    }
    if (artistCandidates.length === 0) {
      pushPrimary('/api/search', { q: strictTitle });
      pushPrimary('/api/search', { track_name: strictTitle });
    }

    const primaryRun = await runQueryBatch(primaryQueries, activeHypotheses);
    if (primaryRun.earlyMatch) {
      const payload = buildResultPayload(primaryRun.earlyMatch, trackKey, 'lrclib');
      setCacheEntry(trackKey, payload);
      return payload;
    }

    // Only attempt secondary YouTube-channel fallback hypotheses when primary did not yield a confident match
    const secondaryFallbacks = fallbackHypotheses.filter((fh) => !fh.coEqual);
    if (primaryRun.bestScore < 85 && secondaryFallbacks.length > 0) {
      const fallbackQueries = [];
      for (const h of secondaryFallbacks) {
        activeHypotheses.push({ titleVariants: [h.title], artists: h.artists, strictMatch: true });
        for (const ac of h.artists.slice(0, 2)) {
          const p = makePath('/api/search', { track_name: h.title, artist_name: ac });
          if (p) fallbackQueries.push(p);
        }
      }
      const fallbackRun = await runQueryBatch(fallbackQueries, activeHypotheses);
      if (fallbackRun.earlyMatch) {
        const payload = buildResultPayload(fallbackRun.earlyMatch, trackKey, 'lrclib');
        setCacheEntry(trackKey, payload);
        return payload;
      }
    }

    const { best, bestScore } = pickBest(activeHypotheses);
    if (best && bestScore >= 20) {
      const payload = buildResultPayload(best, trackKey, 'lrclib');
      setCacheEntry(trackKey, payload);
      return payload;
    }

    const notFound = {
      trackKey,
      loading: false,
      success: false,
      found: false,
      instrumental: false,
      source: 'none',
      trackName: cleanTitle,
      artistName: primaryCleanArtist,
      albumName: album,
      duration: targetDuration,
      isSynced: false,
      syncedLyrics: null,
      plainLyrics: null,
      ...(hadNetworkError ? { error: 'network_error' } : {})
    };

    if (!hadNetworkError) {
      setCacheEntry(trackKey, notFound);
    }
    return notFound;
  };

  const promise = runSearch().finally(() => {
    inFlightRequests.delete(trackKey);
  });
  inFlightRequests.set(trackKey, promise);
  return promise;
}

function clearLyricsCache() {
  lyricsCache.clear();
}

module.exports = {
  cleanSongTitle,
  cleanArtist,
  cleanArtistName,
  extractTrackAndArtist,
  tokenSimilarity,
  normalizeForCompare,
  searchLrclib,
  parseLrc,
  clearLyricsCache
};
