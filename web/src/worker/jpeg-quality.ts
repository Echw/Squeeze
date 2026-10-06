/**
 * Chooses the JPEG quality from a few encoded tiles instead of the whole
 * image. The weakest tile must reach the target SSIMULACRA2, so smooth skies
 * and fine textures both keep their detail; the full image is encoded once,
 * at the chosen quality.
 */

/**
 * Minimum SSIMULACRA2 of the weakest tile. Smooth images (skies, mist, soft
 * backgrounds) get a higher target: block edges and banding are most visible
 * there, and the extra quality costs few bytes. Calibrated on 36 public
 * photos against TinyJPG: at 71 the total size matches TinyJPG's and the mean
 * SSIMULACRA2 is about 2 higher; at 68 files are 7% smaller but only equal in
 * quality.
 */
export const TARGET_TILE_SCORE = 71;
export const SMOOTH_TARGET_TILE_SCORE = 77;
/** 75th percentile of tile detail below which an image counts as smooth. */
const SMOOTH_DETAIL = 6;
/** The former fixed photo quality: the search only ever raises it. */
export const MIN_QUALITY = 68;
export const MAX_QUALITY = 92;
const START_QUALITY = 72;
/** Typical SSIMULACRA2 gain per quality step; it only sizes the first jump. */
const SCORE_PER_QUALITY = 0.5;
const MAX_EVALUATIONS = 6;
const TILE_SIZE = 256;
const TILE_COUNT = 6;
/** Below this size one tile would be the whole image; the search is skipped. */
const MIN_TILE_SIZE = 64;
export const FALLBACK_QUALITY = 80;

export interface Tile {
  x: number;
  y: number;
  size: number;
}

export interface TileSample {
  tiles: Tile[];
  /** The score the weakest tile must reach. */
  target: number;
}

/**
 * Picks tiles spread over the range of local detail, from the smoothest to
 * the busiest area, and sets the target from how smooth the image is. Tiles
 * start on 16-pixel boundaries so their 8×8 blocks and 4:2:0 chroma match the
 * full image.
 */
export function sampleTiles(rgba: Uint8ClampedArray, width: number, height: number, count = TILE_COUNT): TileSample {
  const size = Math.min(TILE_SIZE, Math.floor(width / 16) * 16, Math.floor(height / 16) * 16);
  if (size < MIN_TILE_SIZE) return { tiles: [], target: TARGET_TILE_SCORE };
  const candidates: { x: number; y: number; activity: number }[] = [];
  for (let y = 0; y + size <= height; y += size) {
    for (let x = 0; x + size <= width; x += size) {
      candidates.push({ x, y, activity: activity(rgba, width, x, y, size) });
    }
  }
  candidates.sort((a, b) => a.activity - b.activity);
  const detail = candidates[Math.floor((candidates.length - 1) * 0.75)]!.activity;
  const target = detail < SMOOTH_DETAIL ? SMOOTH_TARGET_TILE_SCORE : TARGET_TILE_SCORE;
  if (candidates.length <= count) return { tiles: candidates.map(({ x, y }) => ({ x, y, size })), target };
  const tiles = Array.from({ length: count }, (_, index) => {
    const { x, y } = candidates[Math.round(index * (candidates.length - 1) / (count - 1))]!;
    return { x, y, size };
  });
  return { tiles, target };
}

/** Mean absolute horizontal and vertical green difference, sampled every 2 px. */
function activity(rgba: Uint8ClampedArray, width: number, left: number, top: number, size: number): number {
  let sum = 0, count = 0;
  for (let y = top; y < top + size - 1; y += 2) {
    for (let x = left; x < left + size - 1; x += 2) {
      const offset = (y * width + x) * 4 + 1;
      const value = rgba[offset] ?? 0;
      sum += Math.abs(value - (rgba[offset + 4] ?? 0)) + Math.abs(value - (rgba[offset + width * 4] ?? 0));
      count++;
    }
  }
  return sum / Math.max(1, count);
}

export function extractTile(rgba: Uint8ClampedArray, width: number, tile: Tile): Uint8Array {
  const out = new Uint8Array(tile.size * tile.size * 4);
  for (let row = 0; row < tile.size; row++) {
    const start = ((tile.y + row) * width + tile.x) * 4;
    out.set(rgba.subarray(start, start + tile.size * 4), row * tile.size * 4);
  }
  return out;
}

/**
 * Returns the lowest quality whose score reaches the target, assuming the
 * score grows with quality. It brackets the answer and interpolates, so a
 * search usually needs three scores.
 */
export function searchQuality(score: (quality: number) => number, target: number): number {
  let pass: { quality: number; score: number } | undefined;
  let fail: { quality: number; score: number } | undefined;
  const evaluate = (quality: number) => {
    const result = { quality, score: score(quality) };
    if (result.score >= target) pass = result;
    else fail = result;
  };

  evaluate(START_QUALITY);
  for (let evaluations = 1; evaluations < MAX_EVALUATIONS; evaluations++) {
    let next: number;
    if (pass && fail) {
      if (pass.quality - fail.quality <= 2) break;
      const step = (target - fail.score) / Math.max(0.01, pass.score - fail.score);
      next = Math.round(fail.quality + step * (pass.quality - fail.quality));
      next = Math.min(pass.quality - 1, Math.max(fail.quality + 1, next));
    } else if (pass) {
      if (pass.quality <= MIN_QUALITY) break;
      const jump = Math.max(3, Math.ceil((pass.score - target) / SCORE_PER_QUALITY));
      next = Math.max(MIN_QUALITY, pass.quality - jump);
    } else {
      if (fail!.quality >= MAX_QUALITY) break;
      const jump = Math.max(3, Math.ceil((target - fail!.score) / SCORE_PER_QUALITY));
      next = Math.min(MAX_QUALITY, fail!.quality + jump);
    }
    evaluate(next);
  }
  return pass?.quality ?? MAX_QUALITY;
}
