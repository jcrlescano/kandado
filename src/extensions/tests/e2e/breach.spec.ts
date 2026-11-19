// E-15: the breached-site banner in the popup, in a real browser with the
// built extension. Its own browser and server: the worker keeps the breach
// list for hours, so the list is set before anything asks for it.

import { resolve } from "node:path";
import { type BrowserContext, chromium, expect, type Page, test } from "@playwright/test";
import { PASSWORD } from "../unit/helpers";
import { type Running, SAMPLE_ITEMS, startFakeServer } from "./fake-server";

const PORT = 8787;
const dist = resolve(import.meta.dirname, "../../dist/edge");
const shots = resolve(import.meta.dirname, "../../test-results/popup");

let server: Running;
let context: BrowserContext;
let extensionId: string;

test.beforeAll(async () => {
  server = await startFakeServer(PORT, SAMPLE_ITEMS);
  // The sample login for localhost was saved today; a breach dated tomorrow comes after it.
  const tomorrow = new Date(Date.now() + 86_400_000).toISOString().slice(0, 10);
  server.breaches = [
    {
      name: "LocalBreach",
      title: "Local",
      domain: "localhost",
      breach_date: tomorrow,
      added_date: new Date().toISOString(),
      data_classes: ["Email addresses", "Passwords"],
    },
  ];
  context = await chromium.launchPersistentContext("", {
    channel: process.env.KANDADO_BROWSER_CHANNEL ?? "msedge",
    headless: false,
    viewport: { width: 380, height: 600 },
    args: [`--disable-extensions-except=${dist}`, `--load-extension=${dist}`],
  });
  const worker = context.serviceWorkers()[0] ?? (await context.waitForEvent("serviceworker"));
  extensionId = new URL(worker.url()).host;

  const page = await context.newPage();
  await page.goto(`chrome-extension://${extensionId}/src/popup/index.html`);
  await page.getByRole("button", { name: "Continue with Google" }).click();
  await page.getByLabel("Master password").fill(PASSWORD);
  await page.getByRole("button", { name: "Unlock", exact: true }).click();
  await expect(page.getByRole("option", { name: /GitHub/ })).toBeVisible();
  await page.close();
});

test.afterAll(async () => {
  await context?.close();
  await server?.close();
});

/** The popup page about a tab showing `url` (the real toolbar popup cannot be scripted). */
async function popupOver(url: string): Promise<Page> {
  const site = await context.newPage();
  await site.goto(url);
  const worker = context.serviceWorkers()[0];
  if (!worker) throw new Error("no service worker");
  const tabId = await worker.evaluate(
    async (address) => (await chrome.tabs.query({ url: address }))[0]?.id,
    url,
  );
  const page = await context.newPage();
  await page.goto(`chrome-extension://${extensionId}/src/popup/index.html?tab=${tabId}`);
  return page;
}

async function both(page: Page, name: string) {
  for (const scheme of ["dark", "light"] as const) {
    await page.emulateMedia({ colorScheme: scheme });
    await page.waitForTimeout(300);
    await page.screenshot({ path: `${shots}/${name}-${scheme}.png` });
  }
  await page.emulateMedia({ colorScheme: "dark" });
}

test("E-15: a breach of the tab's site after the password was set shows a banner", async () => {
  const page = await popupOver("http://localhost:8787/site");
  await expect(page.getByText(/localhost had a breach on/)).toBeVisible();
  await expect(page.getByText("Sign in, then change this password.")).toBeVisible();
  await expect(page.getByText("Source: Have I Been Pwned")).toBeVisible();
  await expect(page.getByText("Breached", { exact: true })).toBeVisible();
  const suggestions = page.getByRole("listbox", { name: "Suggestions for localhost" });
  await expect(suggestions.getByText("Change", { exact: true })).toBeVisible();
  await both(page, "E-15");
  await page.close();

  // Another site has no logins and no banner.
  const other = await popupOver("http://127.0.0.1:8787/other");
  await expect(other.getByText("No logins for 127.0.0.1")).toBeVisible();
  await expect(other.getByText(/had a breach/)).toHaveCount(0);
  await other.close();
});
