export type ThemeMode = "system" | "dark" | "light";

const STORAGE_KEY = "catnap.theme";

export function coerceThemeMode(value: unknown): ThemeMode {
  if (value === "system" || value === "dark" || value === "light") return value;
  return "system";
}

export function loadThemeMode(storage: Storage = window.localStorage): ThemeMode {
  try {
    const raw = storage.getItem(STORAGE_KEY);
    if (!raw) return "system";
    try {
      return coerceThemeMode(JSON.parse(raw) as unknown);
    } catch {
      return coerceThemeMode(raw);
    }
  } catch {
    return "system";
  }
}

export function saveThemeMode(mode: ThemeMode, storage: Storage = window.localStorage): void {
  try {
    storage.setItem(STORAGE_KEY, JSON.stringify(mode));
  } catch {
    // Ignore persistence errors (e.g. private mode, quota exceeded).
  }
}

export function applyThemeMode(mode: ThemeMode, doc: Document = document): void {
  const root = doc.documentElement;
  if (mode === "system") {
    root.removeAttribute("data-theme");
    root.style.colorScheme = "light dark";
    return;
  }

  root.setAttribute("data-theme", mode);
  root.style.colorScheme = mode;
}

export function initTheme(): ThemeMode {
  const mode = loadThemeMode();
  applyThemeMode(mode);
  return mode;
}
