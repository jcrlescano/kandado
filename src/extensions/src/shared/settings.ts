// The extension's settings (E-28 to E-31), kept in `chrome.storage.local`. They
// hold no secret. The worker is the only writer; pages ask it to change them.

export type ThemeChoice = "system" | "dark" | "light";

export interface ExtensionSettings {
  /** E-28 */
  theme: ThemeChoice;
  /** E-28: logins for the current site, on the toolbar icon. */
  toolbarCount: boolean;
  /** E-29: the Kandado mark and dropdown in page fields. */
  showInFields: boolean;
  /** E-29: the new-password dropdown (E-18). */
  suggestPasswords: boolean;
  /** E-29: the save and update prompts (E-23, E-24). */
  offerToSave: boolean;
  /** E-30: hostnames that get no dropdown and no prompt. */
  excludedSites: string[];
  /** E-31 */
  autoLockMinutes: number;
  /** E-31: after a master-password unlock, ask for a PIN for the rest of the browser session. */
  pinWanted: boolean;
}

export const AUTO_LOCK_CHOICES = [1, 5, 15, 30, 60, 240] as const;

export const DEFAULT_SETTINGS: ExtensionSettings = {
  theme: "system",
  toolbarCount: true,
  showInFields: true,
  suggestPasswords: true,
  offerToSave: true,
  excludedSites: [],
  autoLockMinutes: 15,
  pinWanted: false,
};

/**
 * The hostname a person typed or pasted, as excluded sites keep it: lower case,
 * no scheme, path, port or leading "www.". Null when it is not a hostname.
 */
export function normaliseSite(input: string): string | null {
  let text = input.trim().toLowerCase();
  if (!text) return null;
  if (!/^[a-z][a-z0-9+.-]*:\/\//.test(text)) text = `https://${text}`;
  let host: string;
  try {
    host = new URL(text).hostname;
  } catch {
    return null;
  }
  host = host.replace(/^www\./, "").replace(/\.$/, "");
  if (!host || host.length > 253) return null;
  if (!/^[a-z0-9.-]+$/.test(host) && !/^\[[0-9a-f:]+\]$/.test(host)) return null;
  if (host.startsWith(".") || host.includes("..")) return null;
  return host;
}

/** Stored settings, with defaults for anything missing and anything not valid. */
export function readSettings(stored: unknown): ExtensionSettings {
  const raw = (typeof stored === "object" && stored !== null ? stored : {}) as Record<
    string,
    unknown
  >;
  const bool = (key: keyof ExtensionSettings) =>
    typeof raw[key] === "boolean" ? (raw[key] as boolean) : (DEFAULT_SETTINGS[key] as boolean);
  const theme = raw.theme === "dark" || raw.theme === "light" ? raw.theme : "system";
  const minutes = Number(raw.autoLockMinutes);
  const sites = Array.isArray(raw.excludedSites)
    ? raw.excludedSites.flatMap((site) => {
        const host = typeof site === "string" ? normaliseSite(site) : null;
        return host ? [host] : [];
      })
    : [];
  return {
    theme,
    toolbarCount: bool("toolbarCount"),
    showInFields: bool("showInFields"),
    suggestPasswords: bool("suggestPasswords"),
    offerToSave: bool("offerToSave"),
    excludedSites: [...new Set(sites)],
    autoLockMinutes: (AUTO_LOCK_CHOICES as readonly number[]).includes(minutes)
      ? minutes
      : DEFAULT_SETTINGS.autoLockMinutes,
    pinWanted: bool("pinWanted"),
  };
}
