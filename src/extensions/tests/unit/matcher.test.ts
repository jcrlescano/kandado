import { beforeAll, describe, expect, it } from "vitest";
import { isExcluded, isWebPage, suggestionsFor } from "../../src/background/matcher";
import { sync } from "../../src/background/sync";
import { uuidv7 } from "../../src/shared/bytes";
import { FakeServer, login, newDevice, newWorld, testCore } from "./helpers";

beforeAll(async () => {
  await testCore();
});

const now = () => Math.floor(Date.now() / 1000);

async function vaultWith(items: ReturnType<typeof login>[]) {
  const world = await newWorld();
  const server = new FakeServer(world.vault);
  const device = await newDevice(world);
  await sync(device.store, device.keyring, server);
  for (const item of items) {
    await device.keyring.createItem(world.vaultId, uuidv7(), item, now(), uuidv7());
  }
  return device.keyring;
}

describe("suggestions for a page", () => {
  it("matches by registrable domain, through the shared core", async () => {
    const keyring = await vaultWith([
      login("GitHub", "pw", "https://github.com/login"),
      login("Other", "pw", "https://example.org"),
    ]);
    const hits = suggestionsFor(keyring, await keyring.items(), "https://gist.github.com/");
    expect(hits.map((hit) => hit.item.title)).toEqual(["GitHub"]);
    expect(hits[0]?.insecure).toBe(false);
  });

  it("warns instead of filling a login saved for HTTPS on a plain HTTP page", async () => {
    const keyring = await vaultWith([login("Bank", "pw", "https://bank.example")]);
    const hits = suggestionsFor(keyring, await keyring.items(), "http://bank.example/login");
    expect(hits).toHaveLength(1);
    expect(hits[0]?.insecure).toBe(true);
  });

  it("puts favourites first, and never suggests trashed items, cards or notes", async () => {
    const starred = { ...login("Zed", "pw", "https://a.example"), favourite: true };
    const keyring = await vaultWith([login("Alpha", "pw", "https://a.example"), starred]);
    const items = await keyring.items();
    expect(
      suggestionsFor(keyring, items, "https://a.example/").map((hit) => hit.item.title),
    ).toEqual(["Zed", "Alpha"]);
    const trashed = items.map((item) => ({ ...item, trashed: true }));
    expect(suggestionsFor(keyring, trashed, "https://a.example/")).toEqual([]);
  });

  it("never matches a page that is not the web, or an excluded site", async () => {
    const keyring = await vaultWith([login("Site", "pw", "https://a.example")]);
    const items = await keyring.items();
    expect(suggestionsFor(keyring, items, "chrome://settings")).toEqual([]);
    expect(suggestionsFor(keyring, items, "https://a.example/", ["a.example"])).toEqual([]);
    expect(suggestionsFor(keyring, items, "https://sub.a.example/", ["a.example"])).toEqual([]);
  });

  it("a login set to never fill does not match", async () => {
    const never = login("Never", "pw", "https://a.example");
    never.data.websites = [{ url: "https://a.example", match_mode: "never" }];
    const keyring = await vaultWith([never]);
    expect(suggestionsFor(keyring, await keyring.items(), "https://a.example/")).toEqual([]);
  });
});

describe("helpers", () => {
  it("knows web pages", () => {
    expect(isWebPage("https://a.example")).toBe(true);
    expect(isWebPage("http://a.example")).toBe(true);
    expect(isWebPage("file:///c:/x")).toBe(false);
    expect(isWebPage("nonsense")).toBe(false);
  });

  it("excludes a host and its subdomains, not lookalikes", () => {
    expect(isExcluded("https://a.example/x", ["a.example"])).toBe(true);
    expect(isExcluded("https://notA.example/x", ["a.example"])).toBe(false);
    expect(isExcluded("https://a.example/x", [""])).toBe(false);
  });
});
