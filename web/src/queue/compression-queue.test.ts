import { describe, expect, it } from "vitest";
import { optionsFor } from "./compression-queue";

describe("optionsFor", () => {
  it("passes the default Auto effort, explicit output format and stable engine limits", () => {
    expect(optionsFor("balanced")).toEqual({
      profile: "balanced",
      searchEffort: "auto",
      method: "auto",
      outputFormat: "preserve",
      metadata: "stripPrivate",
      limits: {
        maxInputBytes: 104_857_600,
        maxPixels: 24_000_000,
        maxWorkingBytes: 805_306_368,
      },
    });
  });

  it("keeps the profile threshold separate from Detailed search effort", () => {
    expect(optionsFor("maximumQuality", "detailed", "palette")).toMatchObject({
      profile: "maximumQuality",
      searchEffort: "detailed",
      method: "palette",
      outputFormat: "preserve",
    });
  });

  it("keeps the explicit multi-method search strategy in the worker options", () => {
    expect(optionsFor("balanced", "auto", "search")).toMatchObject({
      method: "search",
      searchEffort: "auto",
    });
  });
});
