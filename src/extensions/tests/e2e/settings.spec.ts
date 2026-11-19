// The welcome tab (E-27) and the settings tab (E-28 to E-31), in a real browser
// with the built extension.

import { resolve } from "node:path";
import { type BrowserContext, chromium, expect, type Page, test } from "@playwright/test";
import { PASSWORD } from "../unit/helpers";
import { type Running, SAMPLE_ITEMS, startFakeServer } from "./fake-server";

const PORT = 8787;
const SITE = `http://localhost:${PORT}`;
const dist = resolve(import.meta.dirname, "../../dist/edge");
const shots = resolve(import.meta.dirname, "../../test-results/settings");

let server: Running;
let context: BrowserContext;
let extensionId: string;

test.beforeAll(async () => {
  server = await startFakeServer(PORT, SAMPLE_ITEMS);
  context = await chromium.launchPersistentContext("", {
    channel: process.env.KANDADO_BROWSER_CHANNEL ?? "msedge",
    headless: false,
    viewport: { width: 1000, height: 680 },
    args: [`--disable-extensions-except=${dist}`, `--load-extension=${dist}`],
  });
  const worker = context.serviceWorkers()[0] ?? (await context.waitForEvent("serviceworker"));
  extensionId = new URL(worker.url()).host;
});

test.afterAll(async () => {
  await context?.close();
  await server?.close();
});

async function both(page: Page, name: string) {
  for (const scheme of ["dark", "light"] as const) {
    await page.emulateMedia({ colorScheme: scheme });
    await page.waitForTimeout(300);
    await page.screenshot({ path: `${shots}/${name}-${scheme}.png` });
  }
  await page.emulateMedia({ colorScheme: "dark" });
}

const url = (path: string) => `chrome-extension://${extensionId}/${path}`;

async function dropdownShown(page: Page): Promise<boolean> {
  await page.waitForTimeout(800);
  return page.frames().some((frame) => frame.url().includes("/src/content/dropdown/"));
}

test("E-27: the welcome tab opens after install, signs in and asks for site access", async () => {
  await expect
    .poll(() => context.pages().some((page) => page.url().includes("/src/pages/welcome/")))
    .toBe(true);
  const page = await context.newPage();
  await page.goto(url("src/pages/welcome/index.html"));
  await expect(page.getByRole("heading", { name: "Kandado is in your browser." })).toBeVisible();
  await both(page, "E-27");

  await page.getByRole("button", { name: "Continue with Google" }).click();
  await expect(page.getByRole("heading", { name: "Let Kandado fill on websites" })).toBeVisible();
  await both(page, "E-27-access");
  // The browser's own permission prompt cannot be driven here; Not now goes on without it.
  await page.getByRole("button", { name: "Not now" }).click();
  await expect(page.getByRole("heading", { name: "You’re all set" })).toBeVisible();
  await expect(page.getByText("Signed in as juan@example.com")).toBeVisible();
  await page.close();

  // Unlock for the tests that follow.
  const popup = await context.newPage();
  await popup.goto(url("src/popup/index.html"));
  await popup.getByLabel("Master password").fill(PASSWORD);
  await popup.getByRole("button", { name: "Unlock", exact: true }).click();
  await expect(popup.getByRole("option", { name: /GitHub/ })).toBeVisible();
  await popup.close();
});

test("E-28: theme and toolbar count", async () => {
  const page = await context.newPage();
  await page.goto(url("src/pages/settings/index.html"));
  await expect(page.getByRole("heading", { name: "General" })).toBeVisible();
  await both(page, "E-28");

  await page.getByRole("radio", { name: "Light", exact: true }).click();
  await expect(page.locator("html")).toHaveAttribute("data-theme", "light");
  await page.emulateMedia({ colorScheme: "dark" });
  await page.screenshot({ path: `${shots}/E-28-chosen-light.png` });
  await page.getByRole("radio", { name: "System", exact: true }).click();
  await expect(page.locator("html")).not.toHaveAttribute("data-theme", /.+/);
  await expect(
    page.getByRole("switch", { name: "Show item count on the toolbar icon" }),
  ).toBeChecked();
  await page.close();
});

test("E-29: turning Kandado off in form fields removes the dropdown", async () => {
  const page = await context.newPage();
  await page.goto(url("src/pages/settings/index.html#autofill"));
  await expect(page.getByRole("heading", { name: "Autofill" })).toBeVisible();
  await both(page, "E-29");
  await page.getByRole("switch", { name: "Show Kandado in form fields" }).click();

  const site = await context.newPage();
  await site.goto(`${SITE}/login`);
  await site.locator("#login").click();
  expect(await dropdownShown(site)).toBe(false);

  await page.bringToFront();
  await page.getByRole("switch", { name: "Show Kandado in form fields" }).click();
  await site.reload();
  await site.locator("#login").click();
  expect(await dropdownShown(site)).toBe(true);
  await site.close();
  await page.close();
});

test("E-30: an excluded site gets no dropdown until it is removed", async () => {
  const page = await context.newPage();
  await page.goto(url("src/pages/settings/index.html#excluded"));
  await page.getByLabel("Add a site").fill("not a site");
  await page.getByRole("button", { name: "Add" }).click();
  await expect(page.getByText("Enter a site, such as example.com.")).toBeVisible();
  await page.getByLabel("Add a site").fill("http://LOCALHOST:8787/login");
  await page.getByRole("button", { name: "Add" }).click();
  await expect(page.getByText("localhost", { exact: true })).toBeVisible();
  await both(page, "E-30");

  const site = await context.newPage();
  await site.goto(`${SITE}/login`);
  await site.locator("#login").click();
  expect(await dropdownShown(site)).toBe(false);

  await page.bringToFront();
  await page.getByRole("button", { name: "Remove localhost" }).click();
  await expect(page.getByText(/No excluded sites/)).toBeVisible();
  await site.reload();
  await site.locator("#login").click();
  expect(await dropdownShown(site)).toBe(true);
  await site.close();
  await page.close();
});

test("E-31: a PIN set here unlocks the popup, and is asked for again after a password unlock", async () => {
  const page = await context.newPage();
  await page.goto(url("src/pages/settings/index.html#security"));
  await expect(page.getByRole("heading", { name: "Security" })).toBeVisible();
  await both(page, "E-31");

  await page.getByRole("button", { name: "Set PIN" }).click();
  await expect(page.getByRole("heading", { name: "Choose a PIN" })).toBeVisible();
  await page.keyboard.type("482913");
  await expect(page.getByRole("heading", { name: "Enter it again" })).toBeVisible();
  await page.keyboard.type("482913");
  await expect(page.getByText("On", { exact: true })).toBeVisible();
  await page.getByRole("radio", { name: "5 min", exact: true }).click();
  await expect(page.getByRole("radio", { name: "5 min", exact: true })).toBeChecked();
  await both(page, "E-31-pin-on");

  // The popup now unlocks with the PIN.
  const popup = await context.newPage();
  await popup.goto(url("src/popup/index.html"));
  await popup.getByRole("button", { name: "Lock now" }).click();
  await expect(popup.getByText("Enter your PIN")).toBeVisible();
  await popup.keyboard.type("482913");
  await expect(popup.getByRole("option", { name: /GitHub/ })).toBeVisible();

  // As after a browser restart: the PIN is gone, so the master password unlocks
  // and the popup asks for a new PIN.
  await popup.evaluate(() => chrome.runtime.sendMessage({ type: "clear-pin" }));
  await popup.getByRole("button", { name: "Lock now" }).click();
  await popup.getByLabel("Master password").fill(PASSWORD);
  await popup.getByRole("button", { name: "Unlock", exact: true }).click();
  await expect(popup.getByRole("heading", { name: "Choose a PIN" })).toBeVisible();
  await popup.emulateMedia({ colorScheme: "dark" });
  await popup.screenshot({ path: `${shots}/popup-choose-pin-dark.png` });
  await popup.getByRole("button", { name: "Not now" }).click();
  await expect(popup.getByRole("option", { name: /GitHub/ })).toBeVisible();
  await popup.close();
  await page.close();
});
