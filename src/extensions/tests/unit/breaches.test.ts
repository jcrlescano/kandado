// E-15: which breaches of the tab's site affect which of its logins.

import { describe, expect, it } from "vitest";
import { type BreachSite, dateToUnix, siteBreach } from "../../src/background/breaches";
import type { Summary } from "../../src/background/keyring";
import type { Suggestion, UrlMatcher } from "../../src/background/matcher";

/** Registrable-domain matching, enough for these tests (the real one is the core's). */
const matcher: UrlMatcher = {
  matchUrl(saved, _mode, page) {
    const site = (url: string) => new URL(url).hostname.split(".").slice(-2).join(".");
    if (site(saved) !== site(page)) return "no";
    return new URL(page).protocol === "http:" ? "insecure" : "yes";
  },
};

function summary(id: string, extra: Partial<Summary> = {}): Summary {
  return {
    id,
    vault_id: "v",
    kind: "login",
    title: id,
    subtitle: "juan",
    websites: [{ url: "https://lakbay.ph", match_mode: "domain" }],
    favourite: false,
    has_totp: false,
    trashed: false,
    created_at: dateToUnix("2026-01-01") ?? 0,
    updated_at: 0,
    ...extra,
  };
}

const hits = (...items: Summary[]): Suggestion[] =>
  items.map((item) => ({ item, insecure: false }));

const LAKBAY: BreachSite = {
  name: "Lakbay",
  title: "Lakbay",
  domain: "lakbay.ph",
  breach_date: "2026-11-02",
  added_date: "2026-11-14T00:00:00Z",
  data_classes: ["Passwords"],
};

describe("siteBreach", () => {
  it("flags logins whose password predates the breach, and not the others", () => {
    const old = summary("old");
    const changed = summary("changed", { password_changed_at: dateToUnix("2026-11-10") ?? 0 });
    const found = siteBreach(matcher, [LAKBAY], "https://www.lakbay.ph/login", hits(old, changed));
    expect(found).toEqual({
      title: "Lakbay",
      domain: "lakbay.ph",
      breach_date: "2026-11-02",
      item_ids: ["old"],
    });
  });

  it("is nothing for another site, a resolved breach, or a changed password", () => {
    expect(siteBreach(matcher, [LAKBAY], "https://other.example/", hits(summary("a")))).toBeNull();
    const resolved = summary("r", { resolved_breaches: ["Lakbay"] });
    expect(siteBreach(matcher, [LAKBAY], "https://lakbay.ph/", hits(resolved))).toBeNull();
    const fresh = summary("f", { created_at: dateToUnix("2026-11-20") ?? 0 });
    expect(siteBreach(matcher, [LAKBAY], "https://lakbay.ph/", hits(fresh))).toBeNull();
  });

  it("counts a plain HTTP page of the same site, and picks the newest breach", () => {
    const older = { ...LAKBAY, name: "Older", title: "Older", breach_date: "2026-03-01" };
    const found = siteBreach(matcher, [older, LAKBAY], "http://lakbay.ph/", hits(summary("a")));
    expect(found?.title).toBe("Lakbay");
  });

  it("reads breach dates as UTC days", () => {
    expect(dateToUnix("1970-01-02")).toBe(86_400);
    expect(dateToUnix("2026-11-02T10:00:00Z")).toBe(dateToUnix("2026-11-02"));
    expect(dateToUnix("not a date")).toBeNull();
  });
});
