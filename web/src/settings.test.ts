import { afterEach, describe, expect, it } from "vitest";
import {
  DEFAULT_SETTINGS,
  LEGACY_STORAGE_KEY,
  SETTINGS_SCHEMA_VERSION,
  STORAGE_KEY,
  loadSettings,
  saveSettings,
} from "./settings";

class StorageMock {
  #values = new Map<string, string>();
  getItem(key: string): string | null { return this.#values.get(key) ?? null; }
  setItem(key: string, value: string): void { this.#values.set(key, value); }
  clear(): void { this.#values.clear(); }
}

const storage = new StorageMock();
Object.defineProperty(globalThis, "localStorage", { value: storage, configurable: true });
afterEach(() => storage.clear());

describe("settings storage v3", () => {
  it("gives a new user strong, preserving, automatic defaults", () => {
    expect(loadSettings()).toEqual(DEFAULT_SETTINGS);
  });

  it("keeps every valid v2 preference", () => {
    storage.setItem(LEGACY_STORAGE_KEY, JSON.stringify({ profile: "balanced", outputFormat: "webp", autoStart: false, method: "search", searchEffort: "detailed", expertMode: true }));
    expect(loadSettings()).toEqual({ profile: "balanced", outputFormat: "webp", autoStart: false, method: "search", paletteColors: 256, paletteDithering: "none", searchEffort: "detailed", expertMode: true });
  });

  it("fills only missing or invalid fields", () => {
    storage.setItem(LEGACY_STORAGE_KEY, JSON.stringify({ profile: "balanced", autoStart: false, method: "invalid" }));
    expect(loadSettings()).toMatchObject({ profile: "balanced", autoStart: false, method: "auto" });
  });

  it("recovers from corrupt JSON", () => {
    storage.setItem(STORAGE_KEY, "{");
    expect(loadSettings()).toEqual(DEFAULT_SETTINGS);
  });

  it("round-trips the versioned envelope", () => {
    saveSettings({ ...DEFAULT_SETTINGS, profile: "balanced", expertMode: true });
    expect(JSON.parse(storage.getItem(STORAGE_KEY)!)).toMatchObject({ schemaVersion: SETTINGS_SCHEMA_VERSION, settings: { profile: "balanced", expertMode: true } });
    expect(loadSettings()).toMatchObject({ profile: "balanced", expertMode: true });
  });
});
