// In-page autofill on fixture pages (E-16 to E-18, E-20 to E-22), in a real
// browser with the built extension. The dropdown lives in a closed shadow root,
// so the tests reach it as a frame of the page, as the browser sees it.

import { resolve } from "node:path";
import {
  type BrowserContext,
  chromium,
  expect,
  type Frame,
  type Page,
  test,
} from "@playwright/test";
import { PASSWORD } from "../unit/helpers";
import { type Running, SAMPLE_ITEMS, startFakeServer } from "./fake-server";

const PORT = 8787;
const SITE = `http://localhost:${PORT}`;
const dist = resolve(import.meta.dirname, "../../dist/edge");
const shots = resolve(import.meta.dirname, "../../test-results/autofill");

let server: Running;
let context: BrowserContext;
let extensionId: string;

test.beforeAll(async () => {
  server = await startFakeServer(PORT, SAMPLE_ITEMS);
  context = await chromium.launchPersistentContext("", {
    channel: process.env.KANDADO_BROWSER_CHANNEL ?? "msedge",
    headless: false,
    viewport: { width: 900, height: 640 },
    args: [`--disable-extensions-except=${dist}`, `--load-extension=${dist}`],
  });
  const worker = context.serviceWorkers()[0] ?? (await context.waitForEvent("serviceworker"));
  extensionId = new URL(worker.url()).host;

  // Sign in and unlock once, through the popup page.
  const popup = await context.newPage();
  await popup.goto(`chrome-extension://${extensionId}/src/popup/index.html`);
  await popup.getByRole("button", { name: "Continue with Google" }).click();
  await popup.getByLabel("Master password").fill(PASSWORD);
  await popup.getByRole("button", { name: "Unlock", exact: true }).click();
  await expect(popup.getByRole("option", { name: /GitHub/ })).toBeVisible();
  await popup.close();
});

test.afterAll(async () => {
  await context?.close();
  await server?.close();
});

/** The dropdown frame inside the page, once it has drawn. */
async function dropdown(page: Page): Promise<Frame> {
  await expect
    .poll(() => page.frames().some((frame) => frame.url().includes("/src/content/dropdown/")))
    .toBe(true);
  const frame = page.frames().find((f) => f.url().includes("/src/content/dropdown/"));
  if (!frame) throw new Error("no dropdown");
  await expect(frame.locator(".kt-drop-in")).toBeVisible();
  return frame;
}

async function noDropdown(page: Page) {
  await page.waitForTimeout(800);
  expect(page.frames().some((frame) => frame.url().includes("/src/content/dropdown/"))).toBe(false);
}

async function both(page: Page, name: string) {
  for (const scheme of ["dark", "light"] as const) {
    await page.emulateMedia({ colorScheme: scheme });
    await page.waitForTimeout(300);
    await page.screenshot({ path: `${shots}/${name}-${scheme}.png` });
  }
  await page.emulateMedia({ colorScheme: "dark" });
}

async function visit(path: string): Promise<Page> {
  const page = await context.newPage();
  await page.goto(`${SITE}/${path}`);
  return page;
}

test("E-16: a login field lists this site's login; a click fills both fields", async () => {
  const page = await visit("login");
  await page.locator("#login").click();
  const frame = await dropdown(page);
  await expect(frame.getByText("Kandado · localhost")).toBeVisible();
  await expect(frame.getByRole("option", { name: /Local test site/ })).toBeVisible();
  await expect(frame.getByRole("option", { name: /GitHub/ })).toHaveCount(0);
  await both(page, "E-16");

  await frame.getByRole("option", { name: /Local test site/ }).click();
  await expect(page.locator("#login")).toHaveValue("juan");
  await expect(page.locator("#password")).toHaveValue("local-pw");
  // The page's own script saw input events, as frameworks need.
  await expect(page.locator("#tracked")).toHaveText("juan|8");
  await noDropdown(page);
  await page.close();
});

test("E-16 by keyboard: arrow down and Enter fill; Enter alone does not", async () => {
  const page = await visit("login");
  await page.locator("#password").click();
  await dropdown(page);
  await page.keyboard.press("Enter"); // no highlight chosen yet: the form's own Enter
  await expect(page.locator("#password")).toHaveValue("");
  await page.locator("#password").click();
  await dropdown(page);
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("ArrowUp");
  await page.keyboard.press("Enter");
  await expect(page.locator("#password")).toHaveValue("local-pw");
  await page.close();
});

test("a page cannot pick for the person with synthetic key events", async () => {
  const page = await visit("login");
  await page.locator("#login").click();
  await dropdown(page);
  await page.evaluate(() => {
    const field = document.getElementById("login") as HTMLInputElement;
    for (const key of ["ArrowDown", "Enter"]) {
      field.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true }));
    }
  });
  await page.waitForTimeout(500);
  await expect(page.locator("#password")).toHaveValue("");
  await page.close();
});

test("E-18: a sign-up form offers a strong password and fills both fields with it", async () => {
  const page = await visit("signup");
  await page.locator("#new").click();
  const frame = await dropdown(page);
  await expect(frame.getByText("Use a strong password")).toBeVisible();
  await both(page, "E-18");
  await frame.getByText("Use a strong password").click();
  const value = await page.locator("#new").inputValue();
  expect(value).toHaveLength(20);
  await expect(page.locator("#confirm")).toHaveValue(value);
  await expect(page.locator("#email")).toHaveValue("");
  await page.close();
});

test("E-20: a checkout form fills the card", async () => {
  const page = await visit("checkout");
  await page.locator("#number").click();
  const frame = await dropdown(page);
  await expect(frame.getByText("Kandado · cards")).toBeVisible();
  await both(page, "E-20");
  await frame.getByRole("option", { name: /BPI Visa/ }).click();
  await expect(page.locator("#number")).toHaveValue("4821009433104821");
  await expect(page.locator("#holder")).toHaveValue("JUAN DELA CRUZ");
  await expect(page.locator("#exp")).toHaveValue("08/29");
  await expect(page.locator("#cvc")).toHaveValue("214");
  await page.close();
});

test("E-21: an address form fills the identity", async () => {
  const page = await visit("address");
  await page.locator("#name").click();
  const frame = await dropdown(page);
  await expect(frame.getByText("Kandado · identities")).toBeVisible();
  await both(page, "E-21");
  await frame.getByRole("option", { name: /Juan Dela Cruz/ }).click();
  await expect(page.locator("#name")).toHaveValue("Juan Dela Cruz");
  await expect(page.locator("#tel")).toHaveValue("+63 917 000 0000");
  await expect(page.locator("#street")).toHaveValue("12 Mabini St.");
  await expect(page.locator("#city")).toHaveValue("Quezon City");
  await page.close();
});

test("E-22: a code field fills the current two-factor code", async () => {
  const page = await visit("two-factor");
  await page.locator("#code").click();
  const frame = await dropdown(page);
  await expect(frame.getByText(/^Fill \d{3} \d{3}$/)).toBeVisible();
  await both(page, "E-22");
  await frame.getByText(/^Fill \d{3} \d{3}$/).click();
  await expect(page.locator("#code")).toHaveValue(/^\d{6}$/);
  await page.close();
});

test("a sign-in form embedded from another site gets no dropdown for this site's login", async () => {
  const page = await visit("framed");
  const inner = page.frameLocator("#other");
  await inner.locator("#login").click();
  await noDropdown(page);
  await expect(inner.locator("#login")).toHaveValue("");
  await page.close();
});

test("E-17: while locked, the dropdown says so and fills nothing", async () => {
  const popup = await context.newPage();
  await popup.goto(`chrome-extension://${extensionId}/src/popup/index.html`);
  await popup.getByRole("button", { name: "Lock now" }).click();
  await popup.close();

  const page = await visit("login");
  await page.locator("#login").click();
  const frame = await dropdown(page);
  await expect(frame.getByText("Kandado is locked")).toBeVisible();
  await expect(frame.getByText("Unlock Kandado to fill")).toBeVisible();
  await both(page, "E-17");
  await expect(page.locator("#password")).toHaveValue("");
  await page.close();
});

/** The popup page about a tab (the real toolbar popup cannot be scripted). */
async function popupOver(site: Page): Promise<Page> {
  const worker = context.serviceWorkers()[0];
  if (!worker) throw new Error("no service worker");
  const tabId = await worker.evaluate(
    async (address) => (await chrome.tabs.query({ url: address }))[0]?.id,
    site.url(),
  );
  const popup = await context.newPage();
  await popup.goto(`chrome-extension://${extensionId}/src/popup/index.html?tab=${tabId}`);
  return popup;
}

test("E-03 and E-07: the popup fills a suggestion, and a card from its detail", async () => {
  // The locked test may have run first: unlock again if needed.
  const unlock = await context.newPage();
  await unlock.goto(`chrome-extension://${extensionId}/src/popup/index.html`);
  const password = unlock.getByLabel("Master password");
  if (await password.isVisible().catch(() => false)) {
    await password.fill(PASSWORD);
    await unlock.getByRole("button", { name: "Unlock", exact: true }).click();
  }
  await expect(unlock.getByRole("option", { name: /GitHub/ })).toBeVisible();
  await unlock.close();

  const site = await visit("login");
  const popup = await popupOver(site);
  await popup
    .getByRole("listbox", { name: "Suggestions for localhost" })
    .getByRole("option", { name: /Local test site/ })
    .click();
  await expect(site.locator("#login")).toHaveValue("juan");
  await expect(site.locator("#password")).toHaveValue("local-pw");
  await popup.close();
  await site.close();

  const checkout = await visit("checkout");
  const cardPopup = await popupOver(checkout);
  await cardPopup.getByRole("option", { name: /BPI Visa/ }).click();
  await expect(cardPopup.getByRole("button", { name: "Fill card" })).toBeVisible();
  await both(cardPopup, "E-07-fill");
  await cardPopup.getByRole("button", { name: "Fill card" }).click();
  await expect(checkout.locator("#number")).toHaveValue("4821009433104821");
  await expect(checkout.locator("#exp")).toHaveValue("08/29");
  await cardPopup.close();
  await checkout.close();
});
