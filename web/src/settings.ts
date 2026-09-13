import type { CompressionMethod, CompressionProfile, OutputFormat, PaletteDithering, SearchEffort } from "./types";

export interface AppSettings {
  profile: CompressionProfile;
  outputFormat: OutputFormat;
  autoStart: boolean;
  method: CompressionMethod;
  paletteColors: number;
  paletteDithering: PaletteDithering;
  searchEffort: SearchEffort;
  expertMode: boolean;
}

export const STORAGE_KEY = "squeeze.settings";
export const LEGACY_STORAGE_KEY = "squeeze.settings.v2";
export const SETTINGS_SCHEMA_VERSION = 3;
interface StoredSettings {
  schemaVersion: number;
  settings: Partial<AppSettings>;
}
export const DEFAULT_SETTINGS: AppSettings = {
  profile: "maximumCompression",
  outputFormat: "preserve",
  autoStart: true,
  method: "auto",
  paletteColors: 256,
  paletteDithering: "none",
  searchEffort: "auto",
  expertMode: false,
};

export function loadSettings(): AppSettings {
  try {
    const current = localStorage.getItem(STORAGE_KEY);
    if (current !== null) return normalizeEnvelope(JSON.parse(current));
    const legacy = localStorage.getItem(LEGACY_STORAGE_KEY);
    // A valid v2 value is a deliberate existing choice. Do not write it back
    // eagerly: migration must be non-destructive and survive private browsing.
    if (legacy !== null) return normalize(JSON.parse(legacy) as Partial<AppSettings>);
    return { ...DEFAULT_SETTINGS };
  } catch { return { ...DEFAULT_SETTINGS }; }
}

export function saveSettings(settings: AppSettings): void {
  const envelope: StoredSettings = { schemaVersion: SETTINGS_SCHEMA_VERSION, settings: normalize(settings) };
  localStorage.setItem(STORAGE_KEY, JSON.stringify(envelope));
}

function normalizeEnvelope(value: unknown): AppSettings {
  if (!value || typeof value !== "object") return { ...DEFAULT_SETTINGS };
  const envelope = value as Partial<StoredSettings>;
  return normalize(envelope.settings);
}

function normalize(stored: Partial<AppSettings> | undefined): AppSettings {
  const value = stored ?? {};
  return {
    profile: oneOf(value.profile, ["maximumQuality", "balanced", "maximumCompression"], DEFAULT_SETTINGS.profile),
    outputFormat: oneOf(value.outputFormat, ["preserve", "webp"], DEFAULT_SETTINGS.outputFormat),
    autoStart: typeof value.autoStart === "boolean" ? value.autoStart : DEFAULT_SETTINGS.autoStart,
    method: oneOf(value.method, ["auto", "search", "lossless", "palette"], DEFAULT_SETTINGS.method),
    paletteColors: oneOfNumber(value.paletteColors, [256, 192, 128, 96, 64, 32], DEFAULT_SETTINGS.paletteColors),
    paletteDithering: oneOf(value.paletteDithering, ["none", "floydSteinberg"], DEFAULT_SETTINGS.paletteDithering),
    searchEffort: oneOf(value.searchEffort, ["auto", "detailed"], DEFAULT_SETTINGS.searchEffort),
    expertMode: typeof value.expertMode === "boolean" ? value.expertMode : DEFAULT_SETTINGS.expertMode,
  };
}

function oneOfNumber(value: unknown, values: readonly number[], fallback: number): number {
  return typeof value === "number" && values.includes(value) ? value : fallback;
}

function oneOf<T extends string>(value: unknown, values: readonly T[], fallback: T): T {
  return typeof value === "string" && values.includes(value as T) ? value as T : fallback;
}
