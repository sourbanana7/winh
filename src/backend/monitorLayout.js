// d:/Antigravity/lumn-windows/src/backend/monitorLayout.js

/**
 * Subtracts an array of blocking intervals from [start, end].
 * @param {[number, number]} interval - [start, end] with start < end
 * @param {Array<[number, number]>} blockers - [[b0, b1], ...]
 * @returns {Array<[number, number]>}
 */
function subtractIntervals(interval, blockers) {
  let segments = [[interval[0], interval[1]]];
  for (const [b0, b1] of blockers) {
    if (b0 >= b1) continue;
    const nextSegments = [];
    for (const [s0, s1] of segments) {
      if (b1 <= s0 || b0 >= s1) {
        nextSegments.push([s0, s1]);
      } else {
        if (s0 < b0) {
          nextSegments.push([s0, b0]);
        }
        if (s1 > b1) {
          nextSegments.push([b1, s1]);
        }
      }
    }
    segments = nextSegments;
  }
  return segments.filter(([s0, s1]) => (s1 - s0) >= 1);
}

/**
 * Merges contiguous collinear segments of the same edge type.
 * @param {Array<{ x1: number, y1: number, x2: number, y2: number, edge: string }>} segments
 * @param {number} tolerance
 */
function mergeSegments(segments, tolerance = 4) {
  const merged = [];
  const groups = new Map();

  for (const seg of segments) {
    const isHorizontal = seg.edge === 'top' || seg.edge === 'bottom';
    const key = isHorizontal
      ? `${seg.edge}:${Math.round(seg.y1)}`
      : `${seg.edge}:${Math.round(seg.x1)}`;
    if (!groups.has(key)) groups.set(key, []);
    groups.get(key).push(seg);
  }

  for (const list of groups.values()) {
    const isHorizontal = list[0].edge === 'top' || list[0].edge === 'bottom';
    if (isHorizontal) {
      list.sort((a, b) => Math.min(a.x1, a.x2) - Math.min(b.x1, b.x2));
      let cur = { ...list[0] };
      let curMin = Math.min(cur.x1, cur.x2);
      let curMax = Math.max(cur.x1, cur.x2);

      for (let i = 1; i < list.length; i++) {
        const next = list[i];
        const nextMin = Math.min(next.x1, next.x2);
        const nextMax = Math.max(next.x1, next.x2);

        if (nextMin <= curMax + tolerance) {
          curMax = Math.max(curMax, nextMax);
          cur.x1 = curMin;
          cur.x2 = curMax;
        } else {
          merged.push(cur);
          cur = { ...next };
          curMin = nextMin;
          curMax = nextMax;
        }
      }
      merged.push(cur);
    } else {
      list.sort((a, b) => Math.min(a.y1, a.y2) - Math.min(b.y1, b.y2));
      let cur = { ...list[0] };
      let curMin = Math.min(cur.y1, cur.y2);
      let curMax = Math.max(cur.y1, cur.y2);

      for (let i = 1; i < list.length; i++) {
        const next = list[i];
        const nextMin = Math.min(next.y1, next.y2);
        const nextMax = Math.max(next.y1, next.y2);

        if (nextMin <= curMax + tolerance) {
          curMax = Math.max(curMax, nextMax);
          cur.y1 = curMin;
          cur.y2 = curMax;
        } else {
          merged.push(cur);
          cur = { ...next };
          curMin = nextMin;
          curMax = nextMax;
        }
      }
      merged.push(cur);
    }
  }

  return merged;
}

/**
 * Checks if a point (px, py) is strictly inside any monitor rect.
 * @param {number} px
 * @param {number} py
 * @param {Array<{ x0: number, y0: number, x1: number, y1: number }>} rects
 * @param {number} inset
 */
function pointInAnyMonitor(px, py, rects, inset = 0) {
  for (const r of rects) {
    if (px >= (r.x0 + inset) && px <= (r.x1 - inset) && py >= (r.y0 + inset) && py <= (r.y1 - inset)) {
      return true;
    }
  }
  return false;
}

/**
 * Computes multi-monitor unified perimeter layout and geometry.
 * @param {Array<{ id: any, bounds?: { x: number, y: number, width: number, height: number }, x?: number, y?: number, width?: number, height?: number, scaleFactor?: number, isPrimary?: boolean }>} displays
 * @param {object} [options]
 * @param {number} [options.seamTolerance=4]
 * @returns {{
 *   monitorCount: number,
 *   bounds: { minX: number, minY: number, maxX: number, maxY: number, width: number, height: number, centerX: number, centerY: number },
 *   outerSegments: Array<{ x1: number, y1: number, x2: number, y2: number, edge: 'top'|'bottom'|'left'|'right' }>,
 *   convexCorners: Array<{ x: number, y: number, sx: number, sy: number }>
 * }}
 */
function computeMultiMonitorLayout(displays, options = {}) {
  const seamTolerance = typeof options.seamTolerance === 'number' ? options.seamTolerance : 4;

  if (!Array.isArray(displays) || displays.length === 0) {
    return {
      monitorCount: 0,
      bounds: { minX: 0, minY: 0, maxX: 0, maxY: 0, width: 0, height: 0, centerX: 0, centerY: 0 },
      outerSegments: [],
      convexCorners: []
    };
  }

  const rects = displays.map((d) => {
    const b = d.bounds || d;
    const x = Number(b.x) || 0;
    const y = Number(b.y) || 0;
    const width = Math.max(1, Number(b.width) || 1920);
    const height = Math.max(1, Number(b.height) || 1080);
    return {
      id: d.id != null ? String(d.id) : '',
      x0: x,
      y0: y,
      x1: x + width,
      y1: y + height,
      width,
      height
    };
  });

  let minX = Infinity;
  let minY = Infinity;
  let maxX = -Infinity;
  let maxY = -Infinity;

  for (const r of rects) {
    if (r.x0 < minX) minX = r.x0;
    if (r.y0 < minY) minY = r.y0;
    if (r.x1 > maxX) maxX = r.x1;
    if (r.y1 > maxY) maxY = r.y1;
  }

  const totalWidth = Math.max(1, maxX - minX);
  const totalHeight = Math.max(1, maxY - minY);
  const bounds = {
    minX,
    minY,
    maxX,
    maxY,
    width: totalWidth,
    height: totalHeight,
    centerX: minX + totalWidth / 2,
    centerY: minY + totalHeight / 2
  };

  const rawSegments = [];

  for (let i = 0; i < rects.length; i++) {
    const r = rects[i];
    const topBlockers = [];
    const bottomBlockers = [];
    const leftBlockers = [];
    const rightBlockers = [];

    for (let j = 0; j < rects.length; j++) {
      if (i === j) continue;
      const other = rects[j];

      // Top edge check: other monitor touches or covers top from above
      if (other.y0 <= (r.y0 - 1) && other.y1 >= (r.y0 - seamTolerance)) {
        const bx0 = Math.max(r.x0, other.x0);
        const bx1 = Math.min(r.x1, other.x1);
        if (bx0 < bx1) topBlockers.push([bx0, bx1]);
      }

      // Bottom edge check: other monitor touches or covers bottom from below
      if (other.y0 <= (r.y1 + seamTolerance) && other.y1 >= (r.y1 + 1)) {
        const bx0 = Math.max(r.x0, other.x0);
        const bx1 = Math.min(r.x1, other.x1);
        if (bx0 < bx1) bottomBlockers.push([bx0, bx1]);
      }

      // Left edge check: other monitor touches or covers left from left
      if (other.x0 <= (r.x0 - 1) && other.x1 >= (r.x0 - seamTolerance)) {
        const by0 = Math.max(r.y0, other.y0);
        const by1 = Math.min(r.y1, other.y1);
        if (by0 < by1) leftBlockers.push([by0, by1]);
      }

      // Right edge check: other monitor touches or covers right from right
      if (other.x0 <= (r.x1 + seamTolerance) && other.x1 >= (r.x1 + 1)) {
        const by0 = Math.max(r.y0, other.y0);
        const by1 = Math.min(r.y1, other.y1);
        if (by0 < by1) rightBlockers.push([by0, by1]);
      }
    }

    const exposedTop = subtractIntervals([r.x0, r.x1], topBlockers);
    for (const [xStart, xEnd] of exposedTop) {
      rawSegments.push({ x1: xStart, y1: r.y0, x2: xEnd, y2: r.y0, edge: 'top' });
    }

    const exposedBottom = subtractIntervals([r.x0, r.x1], bottomBlockers);
    for (const [xStart, xEnd] of exposedBottom) {
      rawSegments.push({ x1: xStart, y1: r.y1, x2: xEnd, y2: r.y1, edge: 'bottom' });
    }

    const exposedLeft = subtractIntervals([r.y0, r.y1], leftBlockers);
    for (const [yStart, yEnd] of exposedLeft) {
      rawSegments.push({ x1: r.x0, y1: yStart, x2: r.x0, y2: yEnd, edge: 'left' });
    }

    const exposedRight = subtractIntervals([r.y0, r.y1], rightBlockers);
    for (const [yStart, yEnd] of exposedRight) {
      rawSegments.push({ x1: r.x1, y1: yStart, x2: r.x1, y2: yEnd, edge: 'right' });
    }
  }

  const outerSegments = mergeSegments(rawSegments, seamTolerance);

  // Extract true outer convex corners
  const cornerCandidates = new Map();
  const SNAP = 4;

  const roundSnap = (val) => Math.round(val / SNAP) * SNAP;

  const horizEdges = outerSegments.filter((s) => s.edge === 'top' || s.edge === 'bottom');
  const vertEdges = outerSegments.filter((s) => s.edge === 'left' || s.edge === 'right');

  for (const h of horizEdges) {
    const hPts = [
      { x: Math.min(h.x1, h.x2), y: h.y1, edge: h.edge, isStart: true },
      { x: Math.max(h.x1, h.x2), y: h.y1, edge: h.edge, isStart: false }
    ];

    for (const hp of hPts) {
      for (const v of vertEdges) {
        const vPts = [
          { x: v.x1, y: Math.min(v.y1, v.y2), edge: v.edge, isStart: true },
          { x: v.x1, y: Math.max(v.y1, v.y2), edge: v.edge, isStart: false }
        ];

        for (const vp of vPts) {
          if (Math.abs(hp.x - vp.x) <= seamTolerance && Math.abs(hp.y - vp.y) <= seamTolerance) {
            const key = `${roundSnap(vp.x)}:${roundSnap(hp.y)}`;
            if (!cornerCandidates.has(key)) {
              cornerCandidates.set(key, {
                x: vp.x,
                y: hp.y,
                hEdge: hp.edge,
                vEdge: vp.edge
              });
            }
          }
        }
      }
    }
  }

  const convexCorners = [];
  const DELTA = 8;

  for (const cand of cornerCandidates.values()) {
    let sx = 0;
    let sy = 0;

    if (cand.hEdge === 'top') {
      sy = 1; // interior below
    } else if (cand.hEdge === 'bottom') {
      sy = -1; // interior above
    }

    if (cand.vEdge === 'left') {
      sx = 1; // interior right
    } else if (cand.vEdge === 'right') {
      sx = -1; // interior left
    }

    if (sx === 0 || sy === 0) continue;

    // Verify convex outer corner:
    // (cx + sx*DELTA, cy + sy*DELTA) is INSIDE a monitor
    // The other 3 quadrants are OUTSIDE all monitors!
    const inInterior = pointInAnyMonitor(cand.x + sx * DELTA, cand.y + sy * DELTA, rects, 1);
    const inOpposite = pointInAnyMonitor(cand.x - sx * DELTA, cand.y - sy * DELTA, rects, 1);
    const inSide1 = pointInAnyMonitor(cand.x - sx * DELTA, cand.y + sy * DELTA, rects, 1);
    const inSide2 = pointInAnyMonitor(cand.x + sx * DELTA, cand.y - sy * DELTA, rects, 1);

    if (inInterior && !inOpposite && !inSide1 && !inSide2) {
      convexCorners.push({
        x: Math.round(cand.x),
        y: Math.round(cand.y),
        sx,
        sy
      });
    }
  }

  return {
    monitorCount: rects.length,
    bounds,
    outerSegments,
    convexCorners
  };
}

module.exports = {
  computeMultiMonitorLayout
};
