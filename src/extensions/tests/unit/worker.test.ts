import { beforeAll, describe, expect, it } from "vitest";
import { Api, type TokenStore, type Tokens } from "../../src/background/api";
import type { FrameRef } from "../../src/background/autofill";
import { provideCore } from "../../src/background/core";
import { route } from "../../src/background/router";
import { Store } from "../../src/background/store";
import { type KeyValue, type Platform, Worker } from "../../src/background/worker";
import { uuidv7 } from "../../src/shared/bytes";
import type { WorkerMessage } from "../../src/shared/messages";
import { FakeServer, login, newWorld, testCore, type World } from "./helpers";

beforeAll(async () => {
  provideCore(await testCore());
});

function memory(): KeyValue & { data: Map<string, unknown> } {
  const data = new Map<string, unknown>();
  return {
    data,
    async get<T>(key: string) {
      return data.get(key) as T | undefined;
    },
    async set(key, value) {
      data.set(key, value);
    },
    async remove(key) {
      data.delete(key);
    },
  };
}

const json = (body: unknown, status = 200) =>
  new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } });

/** A browser and a server: everything a worker touches, in memory. */
async function setup(options: { hasKeys?: boolean } = {}) {
  const world: World = await newWorld();
  const server = new FakeServer(world.vault);
  const tokenData: { current: Tokens | undefined } = { current: undefined };
  const tokens: TokenStore = {
    load: async () => tokenData.current,
    save: async (value) => {
      tokenData.current = value;
    },
  };
  let clock = 1_000_000;
  const alarms = new Map<string, number>();
  const session = memory();
  const sent: { frame: FrameRef; message: WorkerMessage }[] = [];
  const broadcasts: WorkerMessage[] = [];
  const tabUrls = new Map<number, string>();
  const tabFrames = new Map<number, { frameId: number; url: string }[]>();
  const popups = { count: 0 };
  const platform: Platform = {
    session,
    local: memory(),
    identity: {
      redirectUrl: "https://abc.chromiumapp.org/",
      launch: async () => "https://abc.chromiumapp.org/?code=one-time",
    },
    device: { name: "Chrome", platform: "Chrome" },
    alarms: {
      set: (name, minutes) => void alarms.set(name, minutes),
      clear: (name) => void alarms.delete(name),
    },
    now: () => clock,
    sendToFrame: async (frame, message) => {
      sent.push({ frame, message });
      // A content script answers a page fill with how many inputs it wrote.
      return message.type === "kandado-fill-page"
        ? frame.url.includes("/no-form")
          ? 0
          : 2
        : undefined;
    },
    frames: async (tabId) => tabFrames.get(tabId) ?? [],
    broadcast: async (message) => void broadcasts.push(message),
    tabUrl: async (tabId) => tabUrls.get(tabId),
    openPopup: async () => {
      popups.count += 1;
    },
  };
  const fetchImpl: typeof fetch = async (input, init) => {
    const url = new URL(String(input));
    const path = url.pathname;
    if (path === "/v1/auth/token") {
      return json({
        access_token: "a",
        expires_in: 900,
        refresh_token: "r",
        user_id: world.account.user_id,
        device_id: world.account.device_id,
      });
    }
    if (path === "/v1/me") {
      return json({
        id: world.account.user_id,
        email: world.account.email,
        display_name: world.account.display_name,
        plan: "free",
        has_keys: options.hasKeys ?? true,
        device_id: world.account.device_id,
      });
    }
    if (path === "/v1/account/keys") return json(JSON.parse(world.account.keys_json));
    if (path === "/v1/auth/logout") return new Response(null, { status: 204 });
    if (path === "/v1/sync" && (init?.method ?? "GET") === "GET") {
      return json(
        await server.pull(
          Number(url.searchParams.get("cursor")),
          Number(url.searchParams.get("limit")),
        ),
      );
    }
    if (path === "/v1/sync") return json(await server.push(JSON.parse(String(init?.body))));
    return json({ code: "not_found", title: "Not found", status: 404 }, 404);
  };
  const store = await Store.open(`worker-${uuidv7()}`);
  const make = () =>
    new Worker(
      store,
      new Api({ baseUrl: "http://server", tokens, fetch: fetchImpl, now: () => clock }),
      "http://server",
      platform,
    );
  return {
    world,
    server,
    store,
    platform,
    session,
    alarms,
    tokenData,
    sent,
    broadcasts,
    tabUrls,
    tabFrames,
    popups,
    make,
    advance: (ms: number) => {
      clock += ms;
    },
  };
}

describe("worker lifecycle", () => {
  it("signs in, stays locked until the master password, and unlocks", async () => {
    const t = await setup();
    const worker = t.make();
    expect(await worker.start()).toEqual({ state: "signed_out" });

    expect(await worker.signIn()).toEqual({
      state: "locked",
      email: "juan@example.com",
      pin: false,
      desktop: false,
    });
    expect(t.tokenData.current?.refresh_token).toBe("r");

    await expect(worker.unlock("wrong")).rejects.toMatchObject({ code: "wrong_password" });
    expect(worker.status().state).toBe("locked");

    expect((await worker.unlock("correct horse battery staple")).state).toBe("unlocked");
    expect(t.alarms.has("auto-lock")).toBe(true);
    expect(t.alarms.has("sync")).toBe(true);
  });

  it("an account with no keys is told to finish setting up in the desktop app", async () => {
    const t = await setup({ hasKeys: false });
    const worker = t.make();
    await expect(worker.signIn()).rejects.toMatchObject({ code: "no_keys" });
    expect(worker.status().state).toBe("signed_out");
    expect(t.tokenData.current).toBeUndefined();
  });

  it("a restarted worker stays unlocked until the auto-lock time", async () => {
    const t = await setup();
    const first = t.make();
    await first.start();
    await first.signIn();
    await first.unlock("correct horse battery staple");

    const restarted = t.make(); // the worker was stopped for idling and started again
    expect((await restarted.start()).state).toBe("unlocked");

    t.advance(16 * 60_000);
    const late = t.make();
    expect((await late.start()).state).toBe("locked");
    expect(t.session.data.has("session")).toBe(false);
  });

  it("locking wipes the session key and the alarms; the key is only in session storage", async () => {
    const t = await setup();
    const worker = t.make();
    await worker.start();
    await worker.signIn();
    await worker.unlock("correct horse battery staple");
    expect(t.session.data.has("session")).toBe(true);
    // Nothing of the key reaches the local store.
    expect(
      JSON.stringify(await t.store.read(async (tx) => [await tx.items(), await tx.vaults()])),
    ).not.toContain(String((t.session.data.get("session") as { key: string }).key));

    await worker.onAlarm("auto-lock");
    expect(worker.status().state).toBe("locked");
    expect(t.session.data.has("session")).toBe(false);
    expect(t.alarms.size).toBe(0);
    await expect(worker.handle({ type: "items" })).rejects.toMatchObject({ code: "locked" });
  });

  it("signing out removes every local row and the account", async () => {
    const t = await setup();
    const worker = t.make();
    await worker.start();
    await worker.signIn();
    await worker.unlock("correct horse battery staple");
    await worker.sync();
    expect(await t.store.read((tx) => tx.vaults())).toHaveLength(1);

    expect(await worker.signOut()).toEqual({ state: "signed_out" });
    expect(await t.store.read((tx) => tx.vaults())).toHaveLength(0);
    expect(await t.store.account()).toBeUndefined();
    expect(t.tokenData.current).toBeUndefined();
  });
});

describe("PIN, generator and new logins", () => {
  async function unlockedWorker() {
    const t = await setup();
    const worker = t.make();
    await worker.start();
    await worker.signIn();
    await worker.unlock("correct horse battery staple");
    await worker.sync();
    return { t, worker };
  }

  it("a PIN unlocks after a lock, for the rest of the browser session", async () => {
    const { t, worker } = await unlockedWorker();
    await expect(worker.setPin("12a456")).rejects.toMatchObject({ code: "bad_pin" });
    expect(await worker.setPin("482913")).toMatchObject({ pin: true });
    await worker.lock();
    expect(worker.status()).toEqual({
      state: "locked",
      email: "juan@example.com",
      pin: true,
      desktop: false,
    });
    expect((await worker.unlockWithPin("482913")).state).toBe("unlocked");

    // A restarted worker still knows a PIN is set.
    await worker.lock();
    const restarted = t.make();
    expect(await restarted.start()).toMatchObject({ state: "locked", pin: true });
  });

  it("five wrong PINs discard it, and the master password is needed", async () => {
    const { worker } = await unlockedWorker();
    await worker.setPin("482913");
    await worker.lock();
    for (let attempt = 1; attempt < 5; attempt += 1) {
      await expect(worker.unlockWithPin("000000")).rejects.toMatchObject({ code: "wrong_pin" });
      expect(await worker.pinAttemptsLeft()).toBe(5 - attempt);
    }
    await expect(worker.unlockWithPin("000000")).rejects.toMatchObject({ code: "pin_discarded" });
    expect(worker.status()).toMatchObject({ state: "locked", pin: false });
    await expect(worker.unlockWithPin("482913")).rejects.toMatchObject({ code: "pin_discarded" });
  });

  it("generates through the shared core", async () => {
    const { worker } = await unlockedWorker();
    const random = await worker.handle({
      type: "generate",
      options: { mode: "random", length: 24, uppercase: true, numbers: true, symbols: false },
    });
    expect(random.password).toHaveLength(24);
    expect(random.password).toMatch(/^[A-Za-z0-9]+$/);
    expect(random.score).toBe(4);
    const memorable = await worker.handle({
      type: "generate",
      options: { mode: "memorable", words: 4, uppercase: false, numbers: false, symbols: true },
    });
    expect(memorable.password.split("-")).toHaveLength(4);
    expect(await worker.handle({ type: "strength", password: "password" })).toBeLessThan(2);
  });

  it("a new login is saved, suggested for its site, and pushed", async () => {
    const { t, worker } = await unlockedWorker();
    const id = await worker.handle({
      type: "create-login",
      login: { title: "", username: "juan", password: "pw-1", url: "https://linear.app/login" },
    });
    await worker.sync();
    expect(t.server.items.has(id)).toBe(true);
    const hits = await worker.handle({ type: "suggestions", url: "https://linear.app/" });
    expect(hits.map((hit) => hit.item.title)).toEqual(["linear.app"]);
    expect((await worker.handle({ type: "item", id })).data.password).toBe("pw-1");
  });
});

/**
 * The desktop app as the extension sees it through native messaging: the
 * protocol of kandado-bridge, with the user key sealed by the same HPKE
 * (purpose 12) the desktop app uses.
 */
function fakeDesktop(world: World, userKey: Uint8Array) {
  const uuidBytes = (id: string) =>
    Uint8Array.from(id.replace(/-/g, "").match(/../g) ?? [], (h) => Number.parseInt(h, 16));
  const state = {
    unlocked: true,
    unlockEnabled: true,
    userId: world.account.user_id,
    accept: true,
    pairings: new Map<string, Uint8Array>(),
    connections: 0,
    listeners: [] as ((message: unknown) => void)[],
  };
  const connect = () => {
    state.connections += 1;
    const listeners: ((message: unknown) => void)[] = [];
    state.listeners = listeners;
    const reply = (message: unknown) =>
      queueMicrotask(() => {
        for (const listener of listeners) listener(message);
      });
    return {
      postMessage: async (message: unknown) => {
        const request = message as {
          id: number;
          type: string;
          pairing_id?: string;
          public_key?: string;
        };
        const core = (await testCore()) as unknown as {
          sealTo(
            publicKey: Uint8Array,
            purpose: number,
            owner: Uint8Array,
            object: Uint8Array,
            format: number,
            plaintext: Uint8Array,
          ): Uint8Array;
        };
        switch (request.type) {
          case "status":
            return reply({
              id: request.id,
              type: "status",
              version: "0.1.0",
              state: state.unlocked ? "unlocked" : "locked",
              user_id: state.userId,
              unlock_enabled: state.unlockEnabled,
            });
          case "pair":
            if (!state.accept)
              return reply({ id: request.id, type: "refused", reason: "declined" });
            state.pairings.set(
              request.pairing_id as string,
              Uint8Array.from(atob(request.public_key as string), (c) => c.charCodeAt(0)),
            );
            return reply({ id: request.id, type: "paired" });
          case "unlock": {
            if (!state.unlocked)
              return reply({ id: request.id, type: "refused", reason: "locked" });
            const publicKey = state.pairings.get(request.pairing_id as string);
            if (!publicKey) return reply({ id: request.id, type: "refused", reason: "not_paired" });
            const sealed = core.sealTo(
              publicKey,
              12,
              uuidBytes(state.userId),
              uuidBytes(request.pairing_id as string),
              1,
              userKey,
            );
            return reply({
              id: request.id,
              type: "unlocked",
              sealed: btoa(String.fromCharCode(...sealed)),
            });
          }
          case "unpair":
            state.pairings.delete(request.pairing_id as string);
            return reply({ id: request.id, type: "done" });
        }
      },
      onMessage: (listener: (message: unknown) => void) => listeners.push(listener),
      onDisconnect: () => {},
      disconnect: () => {},
    };
  };
  const emit = (event: "locked" | "unlocked") => {
    for (const listener of state.listeners) listener({ type: "event", event });
  };
  return { state, connect, emit };
}

describe("the desktop app link (E-02, E-31)", () => {
  async function paired() {
    const t = await setup();
    const worker = t.make();
    await worker.start();
    await worker.signIn();
    await worker.unlock("correct horse battery staple");
    // The user key, as the desktop app holds it.
    const { Keyring } = await import("../../src/background/keyring");
    const keyring = await Keyring.unlock(
      await testCore(),
      t.store,
      t.world.account,
      "correct horse battery staple",
    );
    const userKey = keyring.exportUserKey();
    keyring.lock();
    const desktop = fakeDesktop(t.world, userKey);
    t.platform.connectNative = desktop.connect;
    t.platform.browserName = "Edge";
    return { t, worker, desktop };
  }

  it("pairs after the person accepts, then unlocks with the desktop app without the password", async () => {
    const { worker, desktop } = await paired();
    const { code } = await worker.handle({ type: "desktop-pair" });
    expect(code).toMatch(/^\d{6}$/);
    await expect.poll(async () => (await worker.desktopStatus()).paired).toBe(true);
    // The code the desktop app shows is made from the public key it received.
    const [publicKey] = [...desktop.state.pairings.values()];
    const core = await testCore();
    expect(core.pairingCode(publicKey as Uint8Array)).toBe(code);

    await worker.lock();
    expect(worker.status()).toMatchObject({ state: "locked", desktop: true });
    expect((await worker.handle({ type: "unlock-desktop" })).state).toBe("unlocked");
    expect((await worker.handle({ type: "items" })).length).toBeGreaterThanOrEqual(0);
  });

  it("locks and unlocks when the desktop app does", async () => {
    const { worker, desktop } = await paired();
    await worker.handle({ type: "desktop-pair" });
    await expect.poll(async () => (await worker.desktopStatus()).paired).toBe(true);

    desktop.emit("locked");
    await expect.poll(() => worker.status().state).toBe("locked");
    desktop.emit("unlocked");
    await expect.poll(() => worker.status().state).toBe("unlocked");
  });

  it("refuses to pair a locked, turned-off or other-account desktop app, and a declined pairing is not kept", async () => {
    const { worker, desktop } = await paired();
    desktop.state.unlocked = false;
    await expect(worker.handle({ type: "desktop-pair" })).rejects.toMatchObject({
      code: "desktop_locked",
    });
    desktop.state.unlocked = true;
    desktop.state.unlockEnabled = false;
    await expect(worker.handle({ type: "desktop-pair" })).rejects.toMatchObject({
      code: "desktop_off",
    });
    desktop.state.unlockEnabled = true;
    desktop.state.userId = "00000000-0000-0000-0000-000000000009";
    await expect(worker.handle({ type: "desktop-pair" })).rejects.toMatchObject({
      code: "desktop_other_account",
    });

    const { worker: other, desktop: declining } = await paired();
    declining.state.accept = false;
    await other.handle({ type: "desktop-pair" });
    await expect.poll(async () => (await other.desktopStatus()).pairingCode).toBeNull();
    expect((await other.desktopStatus()).paired).toBe(false);
    await other.lock();
    expect(other.status()).toMatchObject({ desktop: false });
    void desktop;
  });

  it("a key that does not open is refused, and an unpaired extension forgets its pairing", async () => {
    const { worker, desktop } = await paired();
    await worker.handle({ type: "desktop-pair" });
    await expect.poll(async () => (await worker.desktopStatus()).paired).toBe(true);
    await worker.lock();

    desktop.state.unlocked = false;
    await expect(worker.handle({ type: "unlock-desktop" })).rejects.toMatchObject({
      code: "desktop_locked",
    });
    desktop.state.unlocked = true;
    desktop.state.pairings.clear(); // removed in D-45
    await expect(worker.handle({ type: "unlock-desktop" })).rejects.toMatchObject({
      code: "desktop_declined",
    });
    expect(worker.status()).toMatchObject({ state: "locked", desktop: false });
  });

  it("a content script cannot unlock through the desktop app", async () => {
    const { worker } = await paired();
    const page = { id: "ext", url: "https://github.com/", tab: { id: 7 }, frameId: 0 };
    for (const type of ["unlock-desktop", "desktop-pair", "desktop-unpair", "desktop-status"]) {
      expect(
        await route(worker, { type }, page, { id: "ext", origin: "chrome-extension://ext/" }),
      ).toEqual({
        ok: false,
        error: "not_allowed",
      });
    }
  });
});

describe("what the popup and the page may ask", () => {
  const extension = { id: "ext", origin: "chrome-extension://ext/" };

  async function unlocked() {
    const t = await setup();
    const worker = t.make();
    await worker.start();
    await worker.signIn();
    await worker.unlock("correct horse battery staple");
    await worker.sync();
    // The same account adds a login from another device.
    const other = t.make();
    void other;
    return { t, worker };
  }

  async function withItems(t: Awaited<ReturnType<typeof setup>>, worker: Worker) {
    const { Keyring } = await import("../../src/background/keyring");
    const keyring = await Keyring.unlock(
      await testCore(),
      t.store,
      t.world.account,
      "correct horse battery staple",
    );
    const github = uuidv7();
    await keyring.createItem(
      t.world.vaultId,
      github,
      {
        ...login("GitHub", "s3", "https://github.com"),
        data: {
          username: "juan",
          password: "s3",
          websites: [{ url: "https://github.com", match_mode: "domain" }],
          totp: "JBSWY3DPEHPK3PXP",
        },
      },
      1,
      uuidv7(),
    );
    const card = uuidv7();
    await keyring.createItem(
      t.world.vaultId,
      card,
      {
        title: "BPI Visa",
        notes: "",
        favourite: false,
        custom_fields: [],
        kind: "card",
        data: {
          cardholder: "JUAN",
          number: "4111 1111 1111 1234",
          expiry: { month: 8, year: 2029 },
          cvv: "214",
        },
      },
      1,
      uuidv7(),
    );
    keyring.lock();
    await worker.sync();
    return { github, card };
  }

  const frameSender = (url: string, frameId = 0) => ({ id: "ext", url, tab: { id: 7 }, frameId });
  const pageSender = { id: "ext", url: "chrome-extension://ext/src/content/dropdown/index.html" };

  it("a focused login field opens a dropdown that lists only this frame's logins; a pick fills that frame", async () => {
    const { t, worker } = await unlocked();
    const { github } = await withItems(t, worker);

    const open = await route(
      worker,
      { type: "dropdown-open", kind: "login" },
      frameSender("https://github.com/login"),
      extension,
    );
    expect(open.ok).toBe(true);
    const token = (open as { value: string }).value;
    expect(token).toMatch(/^[0-9a-f]{32}$/);

    const view = await route(worker, { type: "dropdown-view", token }, pageSender, extension);
    expect(view).toMatchObject({ ok: true, value: { mode: "logins", host: "github.com" } });
    const rows = (view as { value: { rows: { id: string; title: string }[] } }).value.rows;
    expect(rows.map((r) => r.title)).toEqual(["GitHub"]);
    expect(JSON.stringify(view)).not.toContain("s3"); // no password in the list

    const pick = await route(
      worker,
      { type: "dropdown-pick", token, choice: { item: github } },
      pageSender,
      extension,
    );
    expect(pick).toEqual({ ok: true, value: "filled" });
    expect(t.sent).toEqual([
      {
        frame: { tabId: 7, frameId: 0, url: "https://github.com/login" },
        message: { type: "kandado-fill", values: { username: "juan", password: "s3" } },
      },
    ]);
    // A token is good for one fill.
    expect(
      await route(
        worker,
        { type: "dropdown-pick", token, choice: { item: github } },
        pageSender,
        extension,
      ),
    ).toEqual({ ok: true, value: "refused" });
  });

  it("a frame from another site gets nothing for a login it does not match, even by naming it", async () => {
    const { t, worker } = await unlocked();
    const { github } = await withItems(t, worker);
    // An embedded frame from another site: no matches, so no dropdown at all.
    const open = await route(
      worker,
      { type: "dropdown-open", kind: "login" },
      frameSender("https://evil.example/frame", 3),
      extension,
    );
    expect(open).toEqual({ ok: true, value: null });

    // A new-password dropdown there, then a pick that names the GitHub login: refused.
    const other = await route(
      worker,
      { type: "dropdown-open", kind: "new-password" },
      frameSender("https://evil.example/frame", 3),
      extension,
    );
    const token = (other as { value: string }).value;
    const pick = await route(
      worker,
      { type: "dropdown-pick", token, choice: { item: github } },
      pageSender,
      extension,
    );
    expect(pick).toEqual({ ok: true, value: "refused" });
    expect(t.sent).toEqual([]);
  });

  it("an HTTP page asks once more before a login saved for HTTPS fills", async () => {
    const { t, worker } = await unlocked();
    const { github } = await withItems(t, worker);
    const open = await route(
      worker,
      { type: "dropdown-open", kind: "login" },
      frameSender("http://github.com/login"),
      extension,
    );
    const token = (open as { value: string }).value;
    const view = await route(worker, { type: "dropdown-view", token }, pageSender, extension);
    expect((view as { value: { rows: { insecure?: boolean }[] } }).value.rows[0]?.insecure).toBe(
      true,
    );
    expect(
      await route(
        worker,
        { type: "dropdown-pick", token, choice: { item: github } },
        pageSender,
        extension,
      ),
    ).toEqual({ ok: true, value: "confirm-insecure" });
    expect(t.sent).toEqual([]);
    expect(
      await route(
        worker,
        { type: "dropdown-pick", token, choice: { item: github, confirmInsecure: true } },
        pageSender,
        extension,
      ),
    ).toEqual({ ok: true, value: "filled" });
  });

  it("cards fill the top page and same-origin frames only", async () => {
    const { t, worker } = await unlocked();
    const { card } = await withItems(t, worker);
    const top = await route(
      worker,
      { type: "dropdown-open", kind: "card" },
      frameSender("https://shop.example/checkout"),
      extension,
    );
    const token = (top as { value: string }).value;
    expect(
      await route(
        worker,
        { type: "dropdown-pick", token, choice: { item: card } },
        pageSender,
        extension,
      ),
    ).toEqual({ ok: true, value: "filled" });
    expect(t.sent[0]?.message).toEqual({
      type: "kandado-fill",
      values: {
        card: { number: "4111111111111234", name: "JUAN", month: 8, year: 2029, cvc: "214" },
      },
    });

    t.tabUrls.set(7, "https://shop.example/checkout");
    const sameOrigin = await route(
      worker,
      { type: "dropdown-open", kind: "card" },
      frameSender("https://shop.example/pay", 2),
      extension,
    );
    expect(sameOrigin.ok && sameOrigin.value).toBeTruthy();
    const crossOrigin = await route(
      worker,
      { type: "dropdown-open", kind: "card" },
      frameSender("https://pay.other.example/card", 4),
      extension,
    );
    expect(crossOrigin).toEqual({ ok: true, value: null });
  });

  it("a code field gets the current two-factor code of this site's login", async () => {
    const { t, worker } = await unlocked();
    await withItems(t, worker);
    const open = await route(
      worker,
      { type: "dropdown-open", kind: "totp" },
      frameSender("https://github.com/sessions/two-factor"),
      extension,
    );
    const token = (open as { value: string }).value;
    const view = await route(worker, { type: "dropdown-view", token }, pageSender, extension);
    const codes = (view as { value: { codes: { code: string }[] } }).value.codes;
    expect(codes).toHaveLength(1);
    expect(codes[0]?.code).toMatch(/^\d{6}$/);
  });

  it("the new-password dropdown offers a generated password and fills every new field with it", async () => {
    const { t, worker } = await unlocked();
    const open = await route(
      worker,
      { type: "dropdown-open", kind: "new-password" },
      frameSender("https://linear.app/signup"),
      extension,
    );
    const token = (open as { value: string }).value;
    const view = (await route(worker, { type: "dropdown-view", token }, pageSender, extension)) as {
      value: { mode: string; password: string };
    };
    expect(view.value.mode).toBe("new-password");
    expect(view.value.password).toHaveLength(20);
    await route(
      worker,
      { type: "dropdown-pick", token, choice: { generated: true } },
      pageSender,
      extension,
    );
    expect(t.sent[0]?.message).toEqual({
      type: "kandado-fill",
      values: { newPassword: view.value.password },
    });
  });

  it("locked shows E-17 and a pick opens the popup; signed out and excluded sites get nothing", async () => {
    const { t, worker } = await unlocked();
    await worker.lock();
    const open = await route(
      worker,
      { type: "dropdown-open", kind: "login" },
      frameSender("https://github.com/login"),
      extension,
    );
    const token = (open as { value: string }).value;
    expect(await route(worker, { type: "dropdown-view", token }, pageSender, extension)).toEqual({
      ok: true,
      value: { mode: "locked", host: "github.com" },
    });
    expect(
      await route(
        worker,
        { type: "dropdown-pick", token, choice: { item: "x" } },
        pageSender,
        extension,
      ),
    ).toEqual({ ok: true, value: "opened-popup" });
    expect(t.popups.count).toBe(1);

    await t.platform.local.set("settings", { excludedSites: ["github.com"] });
    expect(
      await route(
        worker,
        { type: "dropdown-open", kind: "login" },
        frameSender("https://github.com/login"),
        extension,
      ),
    ).toEqual({ ok: true, value: null });

    await worker.signOut();
    expect(
      await route(
        worker,
        { type: "dropdown-open", kind: "login" },
        frameSender("https://example.com/"),
        extension,
      ),
    ).toEqual({ ok: true, value: null });
  });

  it("the popup fills a login only into frames on its site, and a card only on the top page's origin", async () => {
    const { t, worker } = await unlocked();
    const { github, card } = await withItems(t, worker);
    const popup = { id: "ext", url: "chrome-extension://ext/src/popup/index.html" };
    t.tabFrames.set(9, [
      { frameId: 0, url: "https://shop.example/checkout" },
      { frameId: 4, url: "https://pay.other.example/card" },
      { frameId: 6, url: "https://github.com/embedded-login" },
    ]);
    expect(
      await route(worker, { type: "fill-tab", tabId: 9, itemId: github }, popup, extension),
    ).toEqual({
      ok: true,
      value: "filled",
    });
    expect(t.sent.map((entry) => entry.frame.frameId)).toEqual([6]);

    t.sent.length = 0;
    expect(
      await route(worker, { type: "fill-tab", tabId: 9, itemId: card }, popup, extension),
    ).toEqual({
      ok: true,
      value: "filled",
    });
    expect(t.sent.map((entry) => entry.frame.frameId)).toEqual([0]);

    // Nothing to fill: the page has no form for it.
    t.sent.length = 0;
    t.tabFrames.set(10, [{ frameId: 0, url: "https://github.com/no-form" }]);
    expect(
      await route(worker, { type: "fill-tab", tabId: 10, itemId: github }, popup, extension),
    ).toEqual({
      ok: true,
      value: "no-form",
    });
    // A content script cannot ask for it.
    expect(
      await route(
        worker,
        { type: "fill-tab", tabId: 9, itemId: github },
        frameSender("https://github.com/"),
        extension,
      ),
    ).toEqual({ ok: false, error: "not_allowed" });
  });

  async function submitAndCheck(
    worker: Worker,
    url: string,
    username: string,
    password: string,
  ): Promise<string | null> {
    await route(
      worker,
      { type: "login-captured", username, password },
      frameSender(url),
      extension,
    );
    const reply = await route(worker, { type: "prompt-check" }, frameSender(url), extension);
    return reply.ok ? (reply.value as string | null) : null;
  }

  it("E-23: a new login is offered for saving, and Save stores it", async () => {
    const { t, worker } = await unlocked();
    await withItems(t, worker);
    const token = await submitAndCheck(
      worker,
      "https://linear.app/login",
      "juan@example.com",
      "new-pw",
    );
    expect(token).toMatch(/^[0-9a-f]{32}$/);
    const view = await route(
      worker,
      { type: "prompt-view", token: token as string },
      pageSender,
      extension,
    );
    expect(view).toMatchObject({
      ok: true,
      value: { mode: "save", host: "linear.app", username: "juan@example.com" },
    });
    expect(JSON.stringify(view)).not.toContain("new-pw"); // the prompt page never sees the password

    await route(
      worker,
      { type: "prompt-answer", token: token as string, answer: { answer: "save" } },
      pageSender,
      extension,
    );
    expect(t.sent.at(-1)?.message).toEqual({ type: "kandado-prompt-close", token });
    const hits = await worker.handle({ type: "suggestions", url: "https://linear.app/" });
    expect(hits.map((hit) => hit.item.subtitle)).toEqual(["juan@example.com"]);
    expect(
      (await worker.handle({ type: "item", id: hits[0]?.item.id as string })).data.password,
    ).toBe("new-pw");
    // Asked once only.
    expect(
      await route(worker, { type: "prompt-check" }, frameSender("https://linear.app/"), extension),
    ).toEqual({ ok: true, value: null });
  });

  it("E-24: a known username with another password is offered for updating", async () => {
    const { t, worker } = await unlocked();
    const { github } = await withItems(t, worker);
    const token = await submitAndCheck(worker, "https://github.com/session", "JUAN", "changed");
    const view = await route(
      worker,
      { type: "prompt-view", token: token as string },
      pageSender,
      extension,
    );
    expect(view).toMatchObject({
      ok: true,
      value: { mode: "update", title: "GitHub", vault: "Personal" },
    });
    await route(
      worker,
      { type: "prompt-answer", token: token as string, answer: { answer: "update" } },
      pageSender,
      extension,
    );
    expect((await worker.handle({ type: "item", id: github })).data.password).toBe("changed");
  });

  it("no prompt when the login is saved as it is, when locked, or on an excluded site", async () => {
    const { t, worker } = await unlocked();
    await withItems(t, worker);
    expect(await submitAndCheck(worker, "https://github.com/session", "juan", "s3")).toBeNull();

    await route(
      worker,
      { type: "login-captured", username: "a", password: "b" },
      frameSender("https://other.example/"),
      extension,
    );
    await worker.lock();
    expect(
      await route(
        worker,
        { type: "prompt-check" },
        frameSender("https://other.example/"),
        extension,
      ),
    ).toEqual({ ok: true, value: null });

    await worker.unlock("correct horse battery staple");
    await t.platform.local.set("settings", { excludedSites: ["other.example"] });
    expect(await submitAndCheck(worker, "https://other.example/", "a", "b")).toBeNull();
  });

  it("Never for this site excludes it; Not now and a minute's wait drop the password", async () => {
    const { t, worker } = await unlocked();
    const token = await submitAndCheck(worker, "https://news.example/login", "juan", "pw");
    await route(
      worker,
      { type: "prompt-answer", token: token as string, answer: { answer: "never" } },
      pageSender,
      extension,
    );
    expect((await worker.settings()).excludedSites).toEqual(["news.example"]);

    const again = await submitAndCheck(worker, "https://shop.example/login", "juan", "pw");
    await route(
      worker,
      { type: "prompt-answer", token: again as string, answer: { answer: "not-now" } },
      pageSender,
      extension,
    );
    expect(
      await route(worker, { type: "prompt-view", token: again as string }, pageSender, extension),
    ).toEqual({ ok: true, value: { mode: "closed" } });

    await route(
      worker,
      { type: "login-captured", username: "juan", password: "pw" },
      frameSender("https://late.example/"),
      extension,
    );
    t.advance(61_000);
    expect(
      await route(
        worker,
        { type: "prompt-check" },
        frameSender("https://late.example/"),
        extension,
      ),
    ).toEqual({ ok: true, value: null });
  });

  it("only the top frame shows a prompt, and a content script cannot answer one", async () => {
    const { worker } = await unlocked();
    await route(
      worker,
      { type: "login-captured", username: "juan", password: "pw" },
      frameSender("https://news.example/", 3),
      extension,
    );
    expect(
      await route(
        worker,
        { type: "prompt-check" },
        frameSender("https://news.example/", 3),
        extension,
      ),
    ).toEqual({ ok: true, value: null });
    const token = (await route(
      worker,
      { type: "prompt-check" },
      frameSender("https://news.example/"),
      extension,
    )) as { value: string };
    expect(
      await route(
        worker,
        { type: "prompt-answer", token: token.value, answer: { answer: "save" } },
        frameSender("https://news.example/"),
        extension,
      ),
    ).toEqual({ ok: false, error: "not_allowed" });
    expect(
      await route(
        worker,
        { type: "prompt-view", token: token.value },
        frameSender("https://news.example/"),
        extension,
      ),
    ).toEqual({
      ok: false,
      error: "not_allowed",
    });
  });

  it("E-29 settings turn the dropdowns and the prompts off", async () => {
    const { t, worker } = await unlocked();
    await withItems(t, worker);
    const open = (kind: "login" | "new-password", url: string) =>
      route(worker, { type: "dropdown-open", kind }, frameSender(url), extension);

    await route(
      worker,
      { type: "settings-set", patch: { suggestPasswords: false } },
      pageSender,
      extension,
    );
    expect(await open("new-password", "https://linear.app/signup")).toEqual({
      ok: true,
      value: null,
    });
    expect(
      (await open("login", "https://github.com/login")) as { value: unknown },
    ).not.toMatchObject({ value: null });

    await route(
      worker,
      { type: "settings-set", patch: { showInFields: false } },
      pageSender,
      extension,
    );
    expect(await open("login", "https://github.com/login")).toEqual({ ok: true, value: null });

    await route(
      worker,
      { type: "settings-set", patch: { offerToSave: false } },
      pageSender,
      extension,
    );
    expect(await submitAndCheck(worker, "https://new.example/", "juan", "pw")).toBeNull();
  });

  it("settings are cleaned when changed, and Never stores the bare hostname", async () => {
    const { worker } = await unlocked();
    const set = await route(
      worker,
      {
        type: "settings-set",
        patch: { autoLockMinutes: 3, theme: "dark", excludedSites: ["HTTPS://WWW.X.example/a"] },
      },
      pageSender,
      extension,
    );
    expect(set).toMatchObject({
      ok: true,
      value: { autoLockMinutes: 15, theme: "dark", excludedSites: ["x.example"] },
    });
    const token = await submitAndCheck(worker, "https://www.news.example/login", "juan", "pw");
    await route(
      worker,
      { type: "prompt-answer", token: token as string, answer: { answer: "never" } },
      pageSender,
      extension,
    );
    expect((await worker.settings()).excludedSites).toEqual(["x.example", "news.example"]);
    // A content script cannot change settings.
    expect(
      await route(
        worker,
        { type: "settings-set", patch: { showInFields: false } },
        frameSender("https://a.example/"),
        extension,
      ),
    ).toEqual({ ok: false, error: "not_allowed" });
  });

  it("E-28: the toolbar badge counts this site's logins, and is empty when locked or off", async () => {
    const { t, worker } = await unlocked();
    await withItems(t, worker);
    expect(await worker.badgeFor("https://github.com/login")).toBe("1");
    expect(await worker.badgeFor("https://other.example/")).toBe("");
    await worker.changeSettings({ toolbarCount: false });
    expect(await worker.badgeFor("https://github.com/login")).toBe("");
    await worker.changeSettings({ toolbarCount: true });
    await worker.lock();
    expect(await worker.badgeFor("https://github.com/login")).toBe("");
  });

  it("keys from the field reach the dropdown only from the frame that owns it", async () => {
    const { t, worker } = await unlocked();
    await withItems(t, worker);
    const open = await route(
      worker,
      { type: "dropdown-open", kind: "login" },
      frameSender("https://github.com/login"),
      extension,
    );
    const token = (open as { value: string }).value;
    await route(
      worker,
      { type: "dropdown-key", token, key: "down" },
      frameSender("https://github.com/login", 5),
      extension,
    );
    expect(t.broadcasts).toEqual([]);
    await route(
      worker,
      { type: "dropdown-key", token, key: "down" },
      frameSender("https://github.com/login"),
      extension,
    );
    expect(t.broadcasts).toEqual([{ type: "kandado-dropdown-key", token, key: "down" }]);
  });

  it("a content script gets no item content, no unlock and no sign-out", async () => {
    const { worker } = await unlocked();
    const page = { id: "ext", url: "https://github.com/", tab: { id: 7 }, frameId: 0 };
    for (const message of [
      { type: "suggestions", url: "https://github.com/" },
      { type: "dropdown-view", token: "x" },
      { type: "dropdown-pick", token: "x", choice: { item: "x" } },
      { type: "item", id: "x" },
      { type: "totp", id: "x" },
      { type: "unlock", password: "x" },
      { type: "sign-out" },
      { type: "lock" },
      { type: "items" },
    ]) {
      expect(await route(worker, message, page, extension)).toEqual({
        ok: false,
        error: "not_allowed",
      });
    }
  });

  it("refuses messages from other extensions and nonsense", async () => {
    const { worker } = await unlocked();
    expect(
      await route(
        worker,
        { type: "status" },
        { id: "other", url: "chrome-extension://ext/popup.html" },
        extension,
      ),
    ).toEqual({
      ok: false,
      error: "not_allowed",
    });
    expect(
      await route(
        worker,
        { type: "boom" },
        { id: "ext", url: "chrome-extension://ext/popup.html" },
        extension,
      ),
    ).toEqual({
      ok: false,
      error: "not_allowed",
    });
  });
});
