import { describe, expect, it } from "vitest";
import { DEFAULT_SETTINGS, normaliseSite, readSettings } from "../../src/shared/settings";

describe("excluded sites", () => {
  it("keeps the hostname of whatever was typed or pasted", () => {
    expect(normaliseSite("Example.COM")).toBe("example.com");
    expect(normaliseSite("https://www.bank.example/login?x=1")).toBe("bank.example");
    expect(normaliseSite("  intranet.acme.studio:8443/path ")).toBe("intranet.acme.studio");
    expect(normaliseSite("localhost")).toBe("localhost");
  });

  it("refuses what is not a hostname", () => {
    for (const bad of ["", "   ", "not a site", "a..b", "javascript:alert(1)", "*.example.com"]) {
      expect(normaliseSite(bad), bad).toBeNull();
    }
  });
});

describe("stored settings", () => {
  it("fills in defaults and drops what is not valid", () => {
    expect(readSettings(undefined)).toEqual(DEFAULT_SETTINGS);
    expect(
      readSettings({
        theme: "purple",
        autoLockMinutes: 7,
        toolbarCount: "yes",
        excludedSites: ["WWW.A.example", "a.example", 42, "not a site"],
        offerToSave: false,
      }),
    ).toEqual({ ...DEFAULT_SETTINGS, excludedSites: ["a.example"], offerToSave: false });
    expect(readSettings({ theme: "light", autoLockMinutes: 60 })).toMatchObject({
      theme: "light",
      autoLockMinutes: 60,
    });
  });
});
