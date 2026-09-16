import { describe, expect, it } from "vitest";
import { isJpeg, selectJpegliQuality } from "./jpegli";

describe("Jpegli selection", () => {
  it("identifies only JPEG input", () => {
    expect(isJpeg(new Uint8Array([0xff, 0xd8, 0xff, 0xe0]))).toBe(true);
    expect(isJpeg(new Uint8Array([0x89, 0x50, 0x4e, 0x47]))).toBe(false);
  });

  it("uses the conservative quality for low-detail images", () => {
    expect(selectJpegliQuality(0.964, 0.0013)).toBe(85);
    expect(selectJpegliQuality(0.82, 0.036)).toBe(72);
  });
});
