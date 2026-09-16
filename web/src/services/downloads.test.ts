import { unzipSync } from "fflate";
import { describe, expect, it } from "vitest";

import { archiveEntries } from "./downloads";

describe("archiveEntries", () => {
  it("creates a readable archive without a second worker", () => {
    const archive = archiveEntries({
      "road.squeezed.jpg": new Uint8Array([1, 2, 3]),
      "screenshot.squeezed.png": new Uint8Array([4, 5]),
    });

    const files = unzipSync(archive);
    expect([...files["road.squeezed.jpg"]!]).toEqual([1, 2, 3]);
    expect([...files["screenshot.squeezed.png"]!]).toEqual([4, 5]);
  });
});
