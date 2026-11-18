// Which items fit this tab's page. The matching rules themselves live in the
// shared core (`kandado-model::url_match`), so the extension and the desktop
// app agree on what "the same site" means.

import type { Summary } from "./keyring";

export interface Suggestion {
  item: Summary;
  /** The page is plain HTTP and the login was saved for HTTPS: ask before filling. */
  insecure: boolean;
}

export interface UrlMatcher {
  matchUrl(saved: string, mode: string, page: string): "no" | "yes" | "insecure";
}

/** Only web pages can match: not `chrome://`, `about:` or `file:`. */
export function isWebPage(url: string): boolean {
  try {
    const { protocol } = new URL(url);
    return protocol === "https:" || protocol === "http:";
  } catch {
    return false;
  }
}

/** Logins for a page, favourites first, then by title. Trashed items never match. */
export function suggestionsFor(
  matcher: UrlMatcher,
  items: Summary[],
  pageUrl: string,
  excluded: readonly string[] = [],
): Suggestion[] {
  if (!isWebPage(pageUrl) || isExcluded(pageUrl, excluded)) return [];
  const out: Suggestion[] = [];
  for (const item of items) {
    if (item.trashed || item.kind !== "login") continue;
    let best: "no" | "yes" | "insecure" = "no";
    for (const site of item.websites) {
      const result = matcher.matchUrl(site.url, site.match_mode, pageUrl);
      if (result === "yes") {
        best = "yes";
        break;
      }
      if (result === "insecure") best = "insecure";
    }
    if (best !== "no") out.push({ item, insecure: best === "insecure" });
  }
  return out.sort(
    (a, b) =>
      Number(b.item.favourite) - Number(a.item.favourite) ||
      a.item.title.localeCompare(b.item.title),
  );
}

/** Excluded sites (E-30) get no icon, no dropdown and no save prompt. Entries are hostnames. */
export function isExcluded(pageUrl: string, excluded: readonly string[]): boolean {
  let host: string;
  try {
    host = new URL(pageUrl).hostname.toLowerCase();
  } catch {
    return false;
  }
  return excluded.some((entry) => {
    const site = entry.trim().toLowerCase();
    return site !== "" && (host === site || host.endsWith(`.${site}`));
  });
}
