import { afterEach, describe, expect, it } from "vitest";
import { clearRetiredSettings } from "./settings";

class StorageMock {
  #values = new Map<string, string>();
  getItem(key: string): string | null { return this.#values.get(key) ?? null; }
  setItem(key: string, value: string): void { this.#values.set(key, value); }
  removeItem(key: string): void { this.#values.delete(key); }
  clear(): void { this.#values.clear(); }
}

const storage = new StorageMock();
Object.defineProperty(globalThis, "localStorage", { value: storage, configurable: true });
afterEach(() => storage.clear());

describe("legacy settings cleanup", () => {
  it("removes every setting that could restore WebP or expert mode", () => {
    storage.setItem("squeeze.settings", JSON.stringify({ outputFormat: "webp", expertMode: true }));
    storage.setItem("squeeze.settings.v2", JSON.stringify({ method: "search" }));
    clearRetiredSettings();
    expect(storage.getItem("squeeze.settings")).toBeNull();
    expect(storage.getItem("squeeze.settings.v2")).toBeNull();
  });
});
