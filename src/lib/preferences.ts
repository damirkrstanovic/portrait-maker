import type { Role } from "./contracts";

export type CatalogPreferences = {
  previewVisible: boolean;
  gridRole: Role;
  gridZoom: number;
};

const storageKey = "pathfinder-portrait-manager.catalog-preferences";
const defaults: CatalogPreferences = { previewVisible: true, gridRole: "large", gridZoom: 100 };

function isRole(value: unknown): value is Role {
  return value === "small" || value === "medium" || value === "large";
}

export function loadCatalogPreferences(): CatalogPreferences {
  if (typeof window === "undefined") return defaults;
  try {
    const saved: unknown = JSON.parse(window.localStorage.getItem(storageKey) ?? "null");
    if (!saved || typeof saved !== "object") return defaults;
    const preferences = saved as Partial<CatalogPreferences>;
    return {
      previewVisible: typeof preferences.previewVisible === "boolean" ? preferences.previewVisible : defaults.previewVisible,
      gridRole: isRole(preferences.gridRole) ? preferences.gridRole : defaults.gridRole,
      gridZoom: typeof preferences.gridZoom === "number" && Number.isFinite(preferences.gridZoom)
        ? Math.min(200, Math.max(75, preferences.gridZoom)) : defaults.gridZoom,
    };
  } catch {
    return defaults;
  }
}

export function saveCatalogPreferences(next: Partial<CatalogPreferences>): void {
  if (typeof window === "undefined") return;
  try {
    window.localStorage.setItem(storageKey, JSON.stringify({ ...loadCatalogPreferences(), ...next }));
  } catch {
    // Preferences are a convenience and must not interrupt catalog browsing.
  }
}
