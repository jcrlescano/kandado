// Theme: System (default), Light or Dark, chosen in E-28. Enamel (light) is the
// fallback; Hardware's tokens.css follows the system unless data-theme is set.
import { readSettings, type ThemeChoice } from "../shared/settings";

export type { ThemeChoice };

export function applyTheme(choice: ThemeChoice): void {
  const root = document.documentElement;
  if (choice === "system") delete root.dataset.theme;
  else root.dataset.theme = choice;
}

/** Applies the chosen theme, and again whenever it changes. */
export async function loadTheme(): Promise<void> {
  try {
    const { settings } = await chrome.storage.local.get("settings");
    applyTheme(readSettings(settings).theme);
    chrome.storage.onChanged.addListener((changes, area) => {
      if (area === "local" && changes.settings)
        applyTheme(readSettings(changes.settings.newValue).theme);
    });
  } catch {
    applyTheme("system");
  }
}
