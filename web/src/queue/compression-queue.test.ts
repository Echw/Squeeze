import { describe, expect, it } from "vitest";
import { optionsFor } from "./compression-queue";

describe("optionsFor", () => {
  it("passes only a named profile and stable engine limits", () => {
    expect(optionsFor("balanced")).toEqual({
      profile: "balanced",
      outputFormat: "preserve",
      metadata: "stripPrivate",
      limits: {
        maxInputBytes: 104_857_600,
        maxPixels: 24_000_000,
        maxWorkingBytes: 805_306_368,
      },
    });
  });
});

