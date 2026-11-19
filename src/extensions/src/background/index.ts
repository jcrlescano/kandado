// The service worker: wires the worker logic to `chrome.*`.

import designTokens from "../ui/vendor/hardware/tokens.json";
import { Api, type TokenStore, type Tokens } from "./api";
import { route } from "./router";
import { Store } from "./store";
import { type KeyValue, type Platform, Worker } from "./worker";

/** The API address. Release builds use the production server; `KANDADO_SERVER_URL` overrides at build time. */
const SERVER_URL: string = import.meta.env.VITE_KANDADO_SERVER_URL ?? "https://kandado.duckdns.org";

function area(storage: chrome.storage.StorageArea): KeyValue {
  return {
    async get<T>(key: string) {
      return (await storage.get(key))[key] as T | undefined;
    },
    async set(key, value) {
      await storage.set({ [key]: value });
    },
    async remove(key) {
      await storage.remove(key);
    },
  };
}

const tokens: TokenStore = {
  async load() {
    return (await chrome.storage.local.get("tokens")).tokens as Tokens | undefined;
  },
  async save(value) {
    if (value) await chrome.storage.local.set({ tokens: value });
    else await chrome.storage.local.remove("tokens");
  },
};

const platform: Platform = {
  session: area(chrome.storage.session),
  local: area(chrome.storage.local),
  identity: {
    redirectUrl: chrome.identity.getRedirectURL(),
    launch: (url) => chrome.identity.launchWebAuthFlow({ url, interactive: true }),
  },
  device: {
    name: "Browser extension",
    platform: navigator.userAgent.includes("Firefox") ? "Firefox" : "Chrome",
  },
  alarms: {
    set(name, delayMinutes, periodic) {
      void chrome.alarms.create(name, {
        delayInMinutes: delayMinutes,
        periodInMinutes: periodic ? delayMinutes : undefined,
      });
    },
    clear(name) {
      void chrome.alarms.clear(name);
    },
  },
  now: () => Date.now(),
  async sendToFrame(frame, message) {
    return chrome.tabs.sendMessage(frame.tabId, message, { frameId: frame.frameId });
  },
  async frames(tabId) {
    const frames = (await chrome.webNavigation.getAllFrames({ tabId })) ?? [];
    return frames.map((frame) => ({ frameId: frame.frameId, url: frame.url }));
  },
  async broadcast(message) {
    // No extension page listening (no dropdown open) is not an error.
    await chrome.runtime.sendMessage(message).catch(() => {});
  },
  async tabUrl(tabId) {
    try {
      return (await chrome.tabs.get(tabId)).url;
    } catch {
      return undefined;
    }
  },
  connectNative: () => {
    const port = chrome.runtime.connectNative("app.kandado.native");
    return {
      postMessage: (message) => port.postMessage(message),
      onMessage: (listener) => port.onMessage.addListener(listener),
      onDisconnect: (listener) => port.onDisconnect.addListener(() => listener()),
      disconnect: () => port.disconnect(),
    };
  },
  browserName: navigator.userAgent.includes("Edg/")
    ? "Edge"
    : navigator.userAgent.includes("Firefox")
      ? "Firefox"
      : "Chrome",
  async openPopup() {
    await chrome.action.openPopup().catch(() => {});
  },
};

/**
 * The autofill content script, registered for every site the extension may read.
 * Host access to all sites is asked for on the welcome tab (E-27), not at install,
 * so the script is registered at runtime rather than in the manifest: the browser
 * injects it only where access was given.
 */
const CONTENT_SCRIPT_ID = "kandado-autofill";
/** The WebAuthn shim (E-25, E-26): main world, top frame, before the page's own scripts. */
const WEBAUTHN_SCRIPT_ID = "kandado-webauthn";

async function registerContentScript(): Promise<void> {
  const granted = await chrome.permissions.getAll();
  const matches = (granted.origins ?? []).filter(
    (origin) =>
      origin.startsWith("http://") || origin.startsWith("https://") || origin === "<all_urls>",
  );
  const ids = [CONTENT_SCRIPT_ID, WEBAUTHN_SCRIPT_ID];
  const existing = await chrome.scripting.getRegisteredContentScripts({ ids });
  if (existing.length > 0)
    await chrome.scripting.unregisterContentScripts({ ids: existing.map((script) => script.id) });
  if (matches.length === 0) return;
  await chrome.scripting.registerContentScripts([
    {
      id: CONTENT_SCRIPT_ID,
      js: ["content.js"],
      matches,
      allFrames: true,
      runAt: "document_idle",
      persistAcrossSessions: true,
    },
    {
      id: WEBAUTHN_SCRIPT_ID,
      js: ["webauthn.js"],
      matches,
      allFrames: false,
      runAt: "document_start",
      world: "MAIN",
      persistAcrossSessions: true,
    },
  ]);
}

void registerContentScript();
chrome.permissions.onAdded.addListener(() => void registerContentScript());
chrome.permissions.onRemoved.addListener(() => void registerContentScript());

const extension = { id: chrome.runtime.id, origin: chrome.runtime.getURL("") };

const ready = (async () => {
  const store = await Store.open();
  const api = new Api({ baseUrl: SERVER_URL, tokens });
  const worker = new Worker(store, api, SERVER_URL, platform);
  await worker.start();
  return worker;
})();

chrome.runtime.onMessage.addListener((message, sender, respond) => {
  ready
    .then((worker) => route(worker, message, sender, extension))
    .then(respond, () => respond({ ok: false, error: "failed" }));
  return true; // The reply is sent asynchronously.
});

// E-28: logins for the current site, on the toolbar icon.
async function updateBadge(tabId: number, url?: string): Promise<void> {
  const worker = await ready;
  const text = await worker.badgeFor(url ?? (await platform.tabUrl(tabId))).catch(() => "");
  await chrome.action.setBadgeText({ tabId, text }).catch(() => {});
}

async function updateActiveBadge(): Promise<void> {
  const [tab] = await chrome.tabs.query({ active: true, lastFocusedWindow: true });
  if (tab?.id !== undefined) await updateBadge(tab.id, tab.url);
}

chrome.tabs.onActivated.addListener(({ tabId }) => void updateBadge(tabId));
chrome.tabs.onUpdated.addListener((tabId, change, tab) => {
  if (change.status === "complete" || change.url) void updateBadge(tabId, tab.url);
});
// Locking, unlocking, syncing and settings change what the badge says.
chrome.storage.onChanged.addListener(() => void updateActiveBadge());
// The item count sits on Kandado green (the light theme value, which keeps white text readable).
const green = designTokens.color.tokens.find((token) => token.name === "green")?.value;
void chrome.action
  .setBadgeBackgroundColor({ color: typeof green === "object" ? green.light : (green ?? "#000") })
  .catch(() => {});

// E-27: the welcome tab, once, after install.
chrome.runtime.onInstalled.addListener((details) => {
  if (details.reason === "install") {
    void chrome.tabs.create({ url: chrome.runtime.getURL("src/pages/welcome/index.html") });
  }
});

chrome.alarms.onAlarm.addListener((alarm) => {
  void ready.then((worker) => worker.onAlarm(alarm.name));
});

// Locking when the browser goes idle is a setting (E-31); the alarm covers the timer.
