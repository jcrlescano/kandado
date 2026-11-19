// Passkeys (E-25, E-26) in a real browser with the built extension, on a
// fixture page that is its own relying party: it checks every registration and
// sign-in with Web Crypto, as a server would. "Use another device" is checked
// against the browser's virtual authenticator, which stands in for Windows
// Hello or a phone.

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
const shots = resolve(import.meta.dirname, "../../test-results/passkeys");

let server: Running;
let context: BrowserContext;

test.beforeAll(async () => {
  server = await startFakeServer(PORT, SAMPLE_ITEMS);
  context = await chromium.launchPersistentContext("", {
    channel: process.env.KANDADO_BROWSER_CHANNEL ?? "msedge",
    headless: false,
    viewport: { width: 960, height: 640 },
    args: [`--disable-extensions-except=${dist}`, `--load-extension=${dist}`],
  });
  const worker = context.serviceWorkers()[0] ?? (await context.waitForEvent("serviceworker"));
  const extensionId = new URL(worker.url()).host;

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
    .poll(() => page.frames().some((frame) => frame.url().includes("kind=passkey")), {
      timeout: 8000,
    })
    .toBe(true);
  const frame = page.frames().find((f) => f.url().includes("kind=passkey"));
  if (!frame) throw new Error("no passkey prompt");
  await expect(frame.locator(".kx-prompt")).toBeVisible();
  return frame;
}

async function gone(page: Page) {
  await expect
    .poll(() => page.frames().some((frame) => frame.url().includes("kind=passkey")))
    .toBe(false);
}

async function both(page: Page, name: string) {
  for (const scheme of ["dark", "light"] as const) {
    await page.emulateMedia({ colorScheme: scheme });
    await page.waitForTimeout(300);
    await page.screenshot({ path: `${shots}/${name}-${scheme}.png` });
  }
  await page.emulateMedia({ colorScheme: "dark" });
}

const result = (page: Page) => page.locator("#result");

test("E-25 and E-26: save a passkey, then sign in with it", async () => {
  const page = await context.newPage();
  await page.goto(`${SITE}/webauthn`);

  await page.getByRole("button", { name: "Create a passkey" }).click();
  let prompt = await promptFrame(page);
  await expect(prompt.getByText("Save a passkey for localhost?")).toBeVisible();
  await expect(prompt.getByText("juan@local.test")).toBeVisible();
  await both(page, "e25-create");
  await prompt.getByRole("button", { name: "Save passkey" }).click();
  await expect(result(page)).toHaveText(/^registered /);
  await gone(page);

  // Sent to the server with the next sync, as a passkey item.
  await expect
    .poll(() => [...server.sync.items.values()].some((item) => item.kind === "passkey"))
    .toBe(true);

  await page.getByRole("button", { name: "Sign in with a passkey" }).click();
  prompt = await promptFrame(page);
  await expect(prompt.getByText("Sign in with a passkey")).toBeVisible();
  await expect(prompt.getByRole("option", { name: /juan@local\.test/ })).toBeVisible();
  await both(page, "e26-sign-in");
  await prompt.getByRole("button", { name: "Sign in", exact: true }).click();
  await expect(result(page)).toHaveText("signed in as user-7");
  await gone(page);
  await page.close();
});

test("cancel, a passkey the site already has, and a page that gives up", async () => {
  const page = await context.newPage();
  await page.goto(`${SITE}/webauthn`);

  // The site asks again for the same account: Kandado says it has one.
  await page.getByRole("button", { name: "Create another for the same account" }).click();
  let prompt = await promptFrame(page);
  await expect(prompt.getByText(/already has a passkey/)).toBeVisible();
  await expect(prompt.getByRole("button", { name: "Save passkey" })).toHaveCount(0);
  await both(page, "e25-already");
  await prompt.getByRole("button", { name: "Cancel" }).click();
  await expect(result(page)).toHaveText("register error: InvalidStateError");

  await page.getByRole("button", { name: "Sign in with a passkey" }).click();
  prompt = await promptFrame(page);
  await prompt.getByRole("button", { name: "Cancel" }).click();
  await expect(result(page)).toHaveText("sign-in error: NotAllowedError");

  await page.getByRole("button", { name: "Sign in, then give up" }).click();
  await promptFrame(page);
  await expect(result(page)).toHaveText("sign-in error: AbortError");
  await gone(page);
  await page.close();
});

test("Use another device hands the request to the browser", async () => {
  const page = await context.newPage();
  await page.goto(`${SITE}/webauthn`);
  const cdp = await context.newCDPSession(page);
  await cdp.send("WebAuthn.enable");
  await cdp.send("WebAuthn.addVirtualAuthenticator", {
    options: {
      protocol: "ctap2",
      transport: "internal",
      hasResidentKey: true,
      hasUserVerification: true,
      isUserVerified: true,
    },
  });
  const before = [...server.sync.items.values()].filter((item) => item.kind === "passkey").length;

  await page.getByRole("button", { name: "Create a passkey" }).click();
  const prompt = await promptFrame(page);
  await prompt.getByRole("button", { name: "Use another device" }).click();
  // The virtual authenticator made it: Kandado stored nothing.
  await expect(result(page)).toHaveText(/^registered /);
  await gone(page);
  await page.waitForTimeout(1500);
  expect([...server.sync.items.values()].filter((item) => item.kind === "passkey").length).toBe(
    before,
  );
  // And the browser signs in with its own passkey; Kandado has none that fits.
  await page.getByRole("button", { name: "Sign in with a passkey" }).click();
  await expect(result(page)).toHaveText("signed in as user-7");
  expect(page.frames().some((frame) => frame.url().includes("kind=passkey"))).toBe(false);
  await page.close();
});
