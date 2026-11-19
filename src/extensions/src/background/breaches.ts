// The breached-site banner (E-15): does the site in the tab have a breach
// that came after the password of one of its logins was set?
//
// The list is Have I Been Pwned's public breach list, through the Kandado
// server (data CC BY 4.0, Have I Been Pwned). It is fetched whole and matched
// here, so neither the server nor Have I Been Pwned learns which sites the
// person visits. The rules are the desktop app's (`kandado_model::health`).

import type { Summary } from "./keyring";
import type { Suggestion, UrlMatcher } from "./matcher";

/** One breached site, as `GET /v1/monitor/breaches` lists it. */
export interface BreachSite {
  name: string;
  title: string;
  domain: string;
  /** `YYYY-MM-DD`. */
  breach_date: string;
  added_date: string;
  data_classes: string[];
}

export interface BreachList {
  breaches: BreachSite[];
  fetched_at: string;
}

/** What the popup shows for a breached site. */
export interface SiteBreach {
  title: string;
  domain: string;
  /** `YYYY-MM-DD`. */
  breach_date: string;
  /** The logins for this site whose password predates the breach. */
  item_ids: string[];
}

/** How long the list is kept before asking the server again. */
export const BREACH_LIST_FRESH_MS = 12 * 60 * 60 * 1000;

/** Midnight UTC of a `YYYY-MM-DD` date, in Unix seconds. */
export function dateToUnix(date: string): number | null {
  const match = /^(\d{4})-(\d{2})-(\d{2})/.exec(date.trim());
  if (!match) return null;
  const ms = Date.UTC(Number(match[1]), Number(match[2]) - 1, Number(match[3]));
  return Number.isNaN(ms) ? null : ms / 1000;
}

/** When a login's password was set: its stamp, or when the item was made. */
function passwordSet(item: Summary): number {
  return item.password_changed_at ?? item.created_at;
}

/**
 * The newest breach of the tab's site that still affects one of its logins
 * (password set before the breach, not marked resolved), or null.
 */
export function siteBreach(
  matcher: UrlMatcher,
  list: BreachSite[],
  pageUrl: string,
  suggestions: Suggestion[],
): SiteBreach | null {
  let best: SiteBreach | null = null;
  let bestDate = -Infinity;
  for (const breach of list) {
    const when = dateToUnix(breach.breach_date);
    if (when === null || !breach.domain) continue;
    // The same site, whatever the scheme: "insecure" is a match on plain HTTP.
    if (matcher.matchUrl(`https://${breach.domain}`, "domain", pageUrl) === "no") continue;
    const affected = suggestions
      .map((hit) => hit.item)
      .filter(
        (item) =>
          item.kind === "login" &&
          passwordSet(item) <= when &&
          !(item.resolved_breaches ?? []).includes(breach.name),
      );
    if (affected.length > 0 && when > bestDate) {
      bestDate = when;
      best = {
        title: breach.title || breach.name,
        domain: breach.domain,
        breach_date: breach.breach_date.slice(0, 10),
        item_ids: affected.map((item) => item.id),
      };
    }
  }
  return best;
}
