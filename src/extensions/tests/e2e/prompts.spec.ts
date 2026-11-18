// Save and update prompts (E-23, E-24) on fixture pages, in a real browser with
// the built extension. Each test signs in to a fresh fake server, so saved and
// updated logins do not leak between files.

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
const shots = resolve(import.meta.dirname, "../../test-results/prompts");

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

async function promptFrame(page: Page): Promise<Frame> {
  await expect
    .poll(() => page.frames().some((frame) => frame.url().includes("/src/content/prompts/")), {
      timeout: 8000,
    })
    .toBe(true);
  const frame = page.frames().find((f) => f.url().includes("/src/content/prompts/"));
  if (!frame) throw new Error("no prompt");
  await expect(frame.locator(".kx-prompt")).toBeVisible();
  return frame;
}

async function noPrompt(page: Page) {
  await page.waitForTimeout(2500);
  expect(page.frames().some((frame) => frame.url().includes("/src/content/prompts/"))).toBe(false);
}

async function both(page: Page, name: string) {
  for (const scheme of ["dark", "light"] as const) {
    await page.emulateMedia({ colorScheme: scheme });
    await page.waitForTimeout(300);
    await page.screenshot({ path: `${shots}/${name}-${scheme}.png` });
  }
  await page.emulateMedia({ colorScheme: "dark" });
}

/** Types into the sign-in fixture and submits it (the page stays: a script handles the form). */
async function signIn(username: string, password: string): Promise<Page> {
  const page = await context.newPage();
  await page.goto(`${SITE}/login`);
  await page.locator("#login").click();
  await page.keyboard.press("Escape"); // close the dropdown and type by hand
  await page.locator("#login").fill(username);
  await page.locator("#password").fill(password);
  await page.getByRole("button", { name: "Sign in" }).click();
  return page;
}

test("E-23: a new login is offered and saved", async () => {
  const before = server.sync.items.size;
  const page = await signIn("maria", "maria-pw-1");
  const frame = await promptFrame(page);
  await expect(frame.getByText("Save login for localhost?")).toBeVisible();
  await expect(frame.getByText("maria", { exact: true })).toBeVisible();
  await both(page, "E-23");
  await frame.getByRole("button", { name: "Save", exact: true }).click();
  await expect
    .poll(() => page.frames().some((f) => f.url().includes("/src/content/prompts/")))
    .toBe(false);
  await expect.poll(() => server.sync.items.size).toBe(before + 1);

  // Signing in again with the same values asks nothing.
  await page.close();
  await noPrompt(await signIn("maria", "maria-pw-1"));
});

test("E-24: a known login with a new password is offered for updating", async () => {
  const page = await signIn("juan", "local-pw-2");
  const frame = await promptFrame(page);
  await expect(frame.getByText("Update password for Local test site?")).toBeVisible();
  await expect(frame.getByText(/The old one stays in history/)).toBeVisible();
  await both(page, "E-24");
  await frame.getByRole("button", { name: "Update" }).click();
  await page.close();

  // The dropdown now fills the new password.
  const again = await context.newPage();
  await again.goto(`${SITE}/login`);
  await again.locator("#password").click();
  await expect
    .poll(() => again.frames().some((f) => f.url().includes("/src/content/dropdown/")))
    .toBe(true);
  const dropdown = again.frames().find((f) => f.url().includes("/src/content/dropdown/"));
  await dropdown?.getByRole("option", { name: /Local test site/ }).click();
  await expect(again.locator("#password")).toHaveValue("local-pw-2");
  await again.close();
});

test("the prompt follows a sign-up to the next page, and Not now drops it", async () => {
  const page = await context.newPage();
  await page.goto(`${SITE}/signup`);
  await page.locator("#email").fill("new@example.com");
  await page.keyboard.press("Escape"); // the email field opens the login dropdown over the form
  await page.locator("#new").click();
  await page.keyboard.press("Escape");
  await page.locator("#new").fill("signup-pw-1");
  await page.locator("#confirm").fill("signup-pw-1");
  await page.getByRole("button", { name: "Continue" }).click();
  await page.waitForURL(`${SITE}/site`);
  const frame = await promptFrame(page);
  await expect(frame.getByText("Save login for localhost?")).toBeVisible();
  await expect(frame.getByText("new@example.com")).toBeVisible();
  await frame.getByRole("button", { name: "Not now" }).click();
  await expect
    .poll(() => page.frames().some((f) => f.url().includes("/src/content/prompts/")))
    .toBe(false);
  await page.reload();
  await noPrompt(page);
  await page.close();
});

test("Never for this site stops prompts there", async () => {
  const page = await signIn("someone", "pw-never-1");
  const frame = await promptFrame(page);
  await frame.getByRole("button", { name: "Never for this site" }).click();
  await page.close();
  await noPrompt(await signIn("someone-else", "pw-never-2"));
});
