import { describe, expect, it } from "vitest";
import type { OptimizationReport } from "../types";
import { isJpeg, selectJpegliQuality } from "./jpegli";

function analysis(overrides: Partial<OptimizationReport["analysis"]>): OptimizationReport["analysis"] {
  return {
    kind: "photo", entropy: 7, estimatedColors: 20_000, edgeDensity: 0.03,
    noise: 0.2, flatAreaRatio: 0.5, hasAlpha: false, ...overrides,
  };
}

describe("Jpegli selection", () => {
  it("identifies only JPEG input", () => {
    expect(isJpeg(new Uint8Array([0xff, 0xd8, 0xff, 0xe0]))).toBe(true);
    expect(isJpeg(new Uint8Array([0x89, 0x50, 0x4e, 0x47]))).toBe(false);
  });

  it("uses the measured quality ladder without a user-facing mode", () => {
    expect(selectJpegliQuality(analysis({ flatAreaRatio: 0.964, noise: 0.0013, edgeDensity: 0.000004 }))).toBe(85);
    expect(selectJpegliQuality(analysis({ flatAreaRatio: 0.353, noise: 0.286, edgeDensity: 0.050 }))).toBe(74);
    expect(selectJpegliQuality(analysis({ flatAreaRatio: 0.682, noise: 0.095, edgeDensity: 0.034 }))).toBe(73);
    expect(selectJpegliQuality(analysis({ flatAreaRatio: 0.616, noise: 0.149, edgeDensity: 0.008 }))).toBe(72);
    expect(selectJpegliQuality(analysis({ flatAreaRatio: 0.271, noise: 0.438, edgeDensity: 0.084 }))).toBe(70);
  });
});
