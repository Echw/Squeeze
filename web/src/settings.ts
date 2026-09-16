const RETIRED_STORAGE_KEYS = ["squeeze.settings", "squeeze.settings.v2"] as const;

/** Old preferences could enable WebP or expert controls, so remove them on upgrade. */
export function clearRetiredSettings(): void {
  try {
    for (const key of RETIRED_STORAGE_KEYS) localStorage.removeItem(key);
  } catch {
    // Storage can be unavailable in private browsing; the fixed defaults still apply.
  }
}
