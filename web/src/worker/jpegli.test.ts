import { describe, expect, it } from "vitest";
import { insertSegments, orientationSegment, readJpegHeader, rgbProfileKind } from "./jpeg-header";
import { MAX_QUALITY, MIN_QUALITY, SMOOTH_TARGET_TILE_SCORE, TARGET_TILE_SCORE, sampleTiles, searchQuality } from "./jpeg-quality";
import { isJpeg } from "./jpegli";

function segment(marker: number, payload: number[]): number[] {
  const length = payload.length + 2;
  return [0xff, marker, length >> 8, length & 0xff, ...payload];
}
function iccSegment(colorSpace: string): number[] {
  const profile = new Array<number>(128).fill(0);
  [...colorSpace].forEach((char, index) => { profile[16 + index] = char.charCodeAt(0); });
  return segment(0xe2, [...new TextEncoder().encode("ICC_PROFILE\0"), 1, 1, ...profile]);
}
const sof = (components: number) => segment(0xc0, [8, 0, 16, 0, 16, components, ...new Array<number>(components * 3).fill(0)]);
const jpeg = (...segments: number[][]) => new Uint8Array([0xff, 0xd8, ...segments.flat(), 0xff, 0xda, 0x00, 0x02, 0x00]);

describe("JPEG header", () => {
  it("identifies only JPEG input", () => {
    expect(isJpeg(new Uint8Array([0xff, 0xd8, 0xff, 0xe0]))).toBe(true);
    expect(isJpeg(new Uint8Array([0x89, 0x50, 0x4e, 0x47]))).toBe(false);
  });

  it("reports the ICC colour space and component count", () => {
    const header = readJpegHeader(jpeg(iccSegment("CMYK"), sof(4)));
    expect(header?.iccColorSpace).toBe("CMYK");
    expect(header?.components).toBe(4);
    expect(header?.iccSegments).toHaveLength(1);
    expect(readJpegHeader(jpeg(sof(3)))?.iccColorSpace).toBeUndefined();
  });

  it("round-trips the EXIF orientation it writes", () => {
    const withOrientation = insertSegments(jpeg(sof(3)), [orientationSegment(6)]);
    expect(readJpegHeader(withOrientation)?.orientation).toBe(6);
    expect(readJpegHeader(jpeg(sof(3)))?.orientation).toBe(1);
  });

  it("rejects a truncated marker", () => {
    expect(readJpegHeader(new Uint8Array([0xff, 0xd8, 0xff, 0xe2, 0x10, 0x00]))).toBeUndefined();
  });
});

describe("JPEG quality search", () => {
  const linear = (threshold: number) => {
    const calls: number[] = [];
    const score = (quality: number) => { calls.push(quality); return TARGET_TILE_SCORE + (quality - threshold) * 0.5; };
    return { calls, score };
  };

  it("finds the lowest passing quality within two steps and few scores", () => {
    for (const threshold of [70, 75, 83, 90]) {
      const { calls, score } = linear(threshold);
      const quality = searchQuality(score, TARGET_TILE_SCORE);
      expect(quality).toBeGreaterThanOrEqual(threshold);
      expect(quality - threshold).toBeLessThanOrEqual(2);
      expect(calls.length).toBeLessThanOrEqual(6);
    }
  });

  it("stays within the quality range", () => {
    expect(searchQuality(linear(61).score, TARGET_TILE_SCORE)).toBe(MIN_QUALITY);
    expect(searchQuality(() => 100, TARGET_TILE_SCORE)).toBe(MIN_QUALITY);
    expect(searchQuality(() => 0, TARGET_TILE_SCORE)).toBe(MAX_QUALITY);
  });

  it("spreads tiles across smooth and detailed areas on the MCU grid", () => {
    const width = 1024, height = 512;
    const rgba = new Uint8ClampedArray(width * height * 4);
    for (let y = 0; y < height; y++) for (let x = 0; x < width; x++) {
      // Detail grows from left to right.
      rgba[(y * width + x) * 4 + 1] = (x * y) % Math.max(1, Math.floor(x / 64)) * 16;
    }
    const { tiles, target } = sampleTiles(rgba, width, height);
    expect(tiles).toHaveLength(6);
    expect(tiles.every((tile) => tile.x % 16 === 0 && tile.y % 16 === 0 && tile.size === 256)).toBe(true);
    expect(new Set(tiles.map((tile) => tile.x)).size).toBeGreaterThan(2);
    expect(target).toBe(TARGET_TILE_SCORE);
    expect(sampleTiles(rgba.subarray(0, 48 * 48 * 4), 48, 48).tiles).toEqual([]);
  });

  it("asks more of smooth images", () => {
    const flat = new Uint8ClampedArray(512 * 512 * 4).fill(128);
    expect(sampleTiles(flat, 512, 512).target).toBe(SMOOTH_TARGET_TILE_SCORE);
  });
});

describe("RGB profile kind", () => {
  function profile(primaries: number[][], gamma: number): Uint8Array {
    const tags: [string, Uint8Array][] = primaries.map((xyz, index) => {
      const tag = new Uint8Array(20); const view = new DataView(tag.buffer);
      tag.set([..."XYZ "].map((char) => char.charCodeAt(0)));
      xyz.forEach((value, axis) => view.setInt32(8 + axis * 4, Math.round(value * 65536)));
      return [["rXYZ", "gXYZ", "bXYZ"][index]!, tag];
    });
    const curve = new Uint8Array(14); new DataView(curve.buffer).setUint32(8, 1); new DataView(curve.buffer).setUint16(12, Math.round(gamma * 256));
    curve.set([..."curv"].map((char) => char.charCodeAt(0)));
    tags.push(["rTRC", curve]);
    const header = 132 + tags.length * 12;
    const bytes = new Uint8Array(header + tags.reduce((size, [, data]) => size + data.byteLength, 0));
    const view = new DataView(bytes.buffer);
    view.setUint32(128, tags.length);
    let offset = header;
    tags.forEach(([name, data], index) => {
      bytes.set([...name].map((char) => char.charCodeAt(0)), 132 + index * 12);
      view.setUint32(136 + index * 12, offset); view.setUint32(140 + index * 12, data.byteLength);
      bytes.set(data, offset); offset += data.byteLength;
    });
    return new Uint8Array([0xff, 0xe2, 0, 0, ...new TextEncoder().encode("ICC_PROFILE\0"), 1, 1, ...bytes]);
  }
  const srgb = [[0.4361, 0.2225, 0.0139], [0.3851, 0.7169, 0.0971], [0.1431, 0.0606, 0.7141]];
  const adobe = [[0.6097, 0.3111, 0.0195], [0.2053, 0.6257, 0.0609], [0.1492, 0.0632, 0.7446]];

  it("recognises sRGB by primaries and tone curve", () => {
    // A 2.2 gamma is close enough to the sRGB curve at mid-grey.
    expect(rgbProfileKind([profile(srgb, 2.2)])).toBe("srgb");
    expect(rgbProfileKind([profile(srgb, 1.8)])).toBe("other");
    expect(rgbProfileKind([profile(adobe, 2.2)])).toBe("other");
  });
});
