import type { CompressionMethod, CompressionProfile, OutputFormat, SearchEffort } from "./types";

export interface AppSettings {
  profile: CompressionProfile;
  outputFormat: OutputFormat;
  autoStart: boolean;
  method: CompressionMethod;
  searchEffort: SearchEffort;
  expertMode: boolean;
}

const STORAGE_KEY = "squeeze.settings.v2";
export const DEFAULT_SETTINGS: AppSettings = {
  profile: "balanced",
  outputFormat: "preserve",
  autoStart: true,
  method: "auto",
  searchEffort: "auto",
  expertMode: false,
};

export function loadSettings(): AppSettings {
  try {
    const stored = JSON.parse(localStorage.getItem(STORAGE_KEY) ?? "{}") as Partial<AppSettings>;
    return {
      profile: oneOf(stored.profile, ["maximumQuality", "balanced", "maximumCompression"], DEFAULT_SETTINGS.profile),
      outputFormat: oneOf(stored.outputFormat, ["preserve", "webp"], DEFAULT_SETTINGS.outputFormat),
      autoStart: typeof stored.autoStart === "boolean" ? stored.autoStart : DEFAULT_SETTINGS.autoStart,
      method: oneOf(stored.method, ["auto", "search", "lossless", "palette"], DEFAULT_SETTINGS.method),
      searchEffort: oneOf(stored.searchEffort, ["auto", "detailed"], DEFAULT_SETTINGS.searchEffort),
      expertMode: typeof stored.expertMode === "boolean" ? stored.expertMode : DEFAULT_SETTINGS.expertMode,
    };
  } catch { return { ...DEFAULT_SETTINGS }; }
}

export function saveSettings(settings: AppSettings): void {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(settings));
}

function oneOf<T extends string>(value: unknown, values: readonly T[], fallback: T): T {
  return typeof value === "string" && values.includes(value as T) ? value as T : fallback;
}
