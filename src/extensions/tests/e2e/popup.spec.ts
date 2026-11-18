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
  context = await chromium.launchPersistentContext("", {
    channel: process.env.KANDADO_BROWSER_CHANNEL ?? "msedge",
    headless: false,
    viewport: { width: 380, height: 600 },
    args: [`--disable-extensions-except=${dist}`, `--load-extension=${dist}`],
  });
  const worker = context.serviceWorkers()[0] ?? (await context.waitForEvent("serviceworker"));
  extensionId = new URL(worker.url()).host;
});

test.afterAll(async () => {
  await context?.close();
  await server?.close();
});

async function popup(scheme: "dark" | "light"): Promise<Page> {
  const page = await context.newPage();
  await page.emulateMedia({ colorScheme: scheme });
  await page.goto(`chrome-extension://${extensionId}/src/popup/index.html`);
  return page;
}

/** Screenshots in both themes, after the colour transitions settle. */
async function both(page: Page, name: string) {
  for (const scheme of ["dark", "light"] as const) {
    await page.emulateMedia({ colorScheme: scheme });
    await page.waitForTimeout(300);
    await page.screenshot({ path: `${shots}/${name}-${scheme}.png` });
  }
  await page.emulateMedia({ colorScheme: "dark" });
}

test("signed out, sign in, unlock, browse, create and generate, in both themes", async () => {
  const page = await popup("dark");
  await expect(page.getByRole("button", { name: "Continue with Google" })).toBeVisible();
  await both(page, "E-01");

  // E-01 → E-02: Google (the fake server sends the browser straight back).
  await page.getByRole("button", { name: "Continue with Google" }).click();
  await expect(page.getByLabel("Master password")).toBeVisible();
  await page.getByLabel("Master password").fill("not it");
  await page.getByRole("button", { name: "Unlock", exact: true }).click();
  await expect(page.getByText("That is not your master password.")).toBeVisible();
  await both(page, "E-02-password");
  await page.getByLabel("Master password").fill(PASSWORD);
  await page.getByRole("button", { name: "Unlock", exact: true }).click();

  // E-03: the items made "on the desktop" arrive through sync.
  await expect(page.getByRole("option", { name: /GitHub/ })).toBeVisible();
  await expect(page.getByRole("option", { name: /BPI Visa/ })).toBeVisible();
  await both(page, "E-03");

  // E-04
  await page.getByRole("searchbox").fill("bpi");
  await expect(page.getByText("Results · 1")).toBeVisible();
  await both(page, "E-04");

  // E-07
  await page.getByRole("option", { name: /BPI Visa/ }).click();
  await expect(page.getByText("Card number")).toBeVisible();
  await expect(page.getByText("08 / 29")).toBeVisible();
  await expect(page.getByText("4821009433104821")).toHaveCount(0); // masked
  await both(page, "E-07");
  await page.getByRole("button", { name: "Back" }).click();
  await page.getByRole("searchbox").fill("");

  // E-06
  await page.getByRole("option", { name: /GitHub/ }).click();
  await expect(page.getByText("juan", { exact: true })).toBeVisible();
  await expect(page.getByText("Tq7#vL2m!Ox0-r9P")).toHaveCount(0);
  await page.getByRole("button", { name: "Reveal" }).click();
  await expect(page.getByText("Tq7#vL2m!Ox0-r9P")).toBeVisible();
  await both(page, "E-06");
  await page.getByRole("button", { name: "Back" }).click();

  // E-14
  await page.getByRole("button", { name: /All vaults/ }).click();
  await expect(page.getByRole("menuitemradio", { name: "Personal" })).toBeVisible();
  await both(page, "E-14");
  await page.keyboard.press("Escape");

  // E-13
  await page.getByRole("button", { name: "Menu" }).click();
  await expect(page.getByRole("menuitem", { name: "Sign out" })).toBeVisible();
  await both(page, "E-13");
  await page.keyboard.press("Escape");

  // E-09 → E-10: a new login is saved and pushed.
  await page.getByRole("button", { name: "Create" }).click();
  await expect(page.getByRole("menuitem", { name: "Login" })).toBeVisible();
  await both(page, "E-09");
  await page.getByRole("menuitem", { name: "Login" }).click();
  await expect(page.getByText("New login")).toBeVisible();
  await page.getByLabel("Title").fill("Linear");
  await page.getByLabel("Username or email").fill("juan@example.com");
  await page.getByLabel("Website").fill("https://linear.app");
  await expect(page.getByText(/bits/)).toBeVisible();
  await both(page, "E-10");
  await page.getByRole("button", { name: "Save" }).click();
  await expect(page.getByText("Login created")).toBeVisible();
  await expect(page.getByRole("option", { name: /Linear/ })).toBeVisible();
  await expect.poll(() => server.sync.items.size).toBe(SAMPLE_ITEMS.length + 1);

  // E-11
  await page.getByRole("button", { name: "Generate", exact: true }).click();
  await expect(page.getByText("Generate password")).toBeVisible();
  await expect(page.getByText(/bits/)).toBeVisible();
  await both(page, "E-11");
  await page.getByRole("button", { name: "Back" }).click();

  // Lock → E-02 with a PIN (set through the worker here; E-31 sets it from settings).
  await page.evaluate(() => chrome.runtime.sendMessage({ type: "set-pin", pin: "482913" }));
  await page.getByRole("button", { name: "Lock now" }).click();
  await expect(page.getByText("Enter your PIN")).toBeVisible();
  await page.keyboard.type("4829");
  await both(page, "E-02-pin");
  await page.keyboard.type("13");
  await expect(page.getByRole("option", { name: /GitHub/ })).toBeVisible();

  // Sign out → E-01.
  await page.getByRole("button", { name: "Menu" }).click();
  await page.getByRole("menuitem", { name: "Sign out" }).click();
  await expect(page.getByRole("button", { name: "Continue with Google" })).toBeVisible();
});

/** Opens the popup page about a tab showing `url` (the real toolbar popup cannot be scripted). */
async function openPopupOver(url: string): Promise<Page> {
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

test("suggestions for the site in the tab, and the empty state for a site with none", async () => {
  // Signed in and unlocked again (the first test signed out).
  const page = await popup("dark");
  await page.getByRole("button", { name: "Continue with Google" }).click();
  await page.getByLabel("Master password").fill(PASSWORD);
  await page.getByRole("button", { name: "Unlock", exact: true }).click();
  await expect(page.getByRole("option", { name: /GitHub/ })).toBeVisible();
  await page.close();

  // E-03: a login saved for this site is suggested on top.
  const suggested = await openPopupOver("http://localhost:8787/site");
  await expect(suggested.getByText("Suggestions for localhost")).toBeVisible();
  await expect(
    suggested.getByRole("listbox", { name: "Suggestions for localhost" }).getByRole("option"),
  ).toHaveCount(1);
  await both(suggested, "E-03-suggestions");
  await suggested.close();

  // E-05: nothing for 127.0.0.1, with a shortcut that prefills the new login.
  const empty = await openPopupOver("http://127.0.0.1:8787/other");
  await expect(empty.getByText("No logins for 127.0.0.1")).toBeVisible();
  await both(empty, "E-05");
  await empty.getByRole("button", { name: "Create login for 127.0.0.1" }).click();
  await expect(empty.getByLabel("Website")).toHaveValue("http://127.0.0.1:8787");
});
