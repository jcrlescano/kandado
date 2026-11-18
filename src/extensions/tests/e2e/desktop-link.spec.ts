// The desktop link end to end: the real native host, the real named pipe, the
// app's browser link (played by kandado-app's `desktop_link` example with the
// same account the fake server serves), and the extension in Edge.
//
// It registers the native host under HKEY_CURRENT_USER, as D-45 does, and
// removes the registration when it ends. So it runs only when asked:
//   cargo build -p kandado-bridge -p kandado-app --example desktop_link
//   KANDADO_E2E_DESKTOP=1 pnpm e2e

import { type ChildProcessWithoutNullStreams, spawn } from "node:child_process";
import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { type BrowserContext, chromium, expect, type Page, test } from "@playwright/test";
import { PASSWORD } from "../unit/helpers";
import { type Running, SAMPLE_ITEMS, startFakeServer } from "./fake-server";

test.skip(!process.env.KANDADO_E2E_DESKTOP, "set KANDADO_E2E_DESKTOP=1 to run the desktop link");

const PORT = 8787;
const root = resolve(import.meta.dirname, "../../../..");
const dist = resolve(import.meta.dirname, "../../dist/edge");
const shots = resolve(import.meta.dirname, "../../test-results/desktop-link");

let server: Running;
let context: BrowserContext;
let extensionId: string;
let desktop: ChildProcessWithoutNullStreams;
const lines: string[] = [];

async function line(pattern: RegExp, timeout = 20_000): Promise<string> {
  await expect.poll(() => lines.some((l) => pattern.test(l)), { timeout }).toBe(true);
  return lines.find((l) => pattern.test(l)) ?? "";
}

function say(command: string) {
  desktop.stdin.write(`${command}\n`);
}

test.beforeAll(async () => {
  server = await startFakeServer(PORT, SAMPLE_ITEMS);
  const dir = mkdtempSync(join(tmpdir(), "kandado-desktop-link-"));
  const keys = join(dir, "keys.json");
  writeFileSync(keys, server.world.account.keys_json);
  desktop = spawn(
    resolve(root, "target/debug/examples/desktop_link.exe"),
    [keys, server.world.account.user_id, PASSWORD, join(dir, "app")],
    { cwd: root },
  );
  desktop.stdout.on("data", (chunk: Buffer) => lines.push(...chunk.toString().split(/\r?\n/)));
  await line(/^ready$/, 60_000);

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
  if (desktop && desktop.exitCode === null) {
    say("quit");
    await line(/^bye$/).catch(() => desktop.kill());
  }
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

test("pair with the code, unlock with the desktop app, and lock and unlock together", async () => {
  // The development build has the fixed ID the desktop app trusts.
  expect(extensionId).toBe("knfanepenpkmplikmbdpmgnmpjfaljco");

  const popup = await context.newPage();
  await popup.goto(`chrome-extension://${extensionId}/src/popup/index.html`);
  await popup.getByRole("button", { name: "Continue with Google" }).click();
  await popup.getByLabel("Master password").fill(PASSWORD);
  await popup.getByRole("button", { name: "Unlock", exact: true }).click();
  await expect(popup.getByRole("option", { name: /GitHub/ })).toBeVisible();

  // E-31: turn on the desktop link; the code here is the code the app shows.
  const settings = await context.newPage();
  await settings.goto(`chrome-extension://${extensionId}/src/pages/settings/index.html#security`);
  const toggle = settings.getByRole("switch", { name: "Lock and unlock with the desktop app" });
  await expect(toggle).toBeEnabled({ timeout: 10_000 });
  await toggle.click();
  const shown = await line(/^code \d{6}$/);
  const code = shown.slice("code ".length);
  await expect(
    settings.getByText("This browser unlocks and locks with the desktop app."),
  ).toBeVisible({
    timeout: 10_000,
  });
  await both(settings, "E-31-paired");
  expect(code).toMatch(/^\d{6}$/);

  // E-02: locked, then unlocked by the desktop app without the password.
  await popup.bringToFront();
  await popup.getByRole("button", { name: "Lock now" }).click();
  await expect(
    popup.getByRole("button", { name: "Unlock with desktop app" }).first(),
  ).toBeVisible();
  await both(popup, "E-02-desktop");
  await popup.getByRole("button", { name: "Unlock with desktop app" }).first().click();
  await expect(popup.getByRole("option", { name: /GitHub/ })).toBeVisible();

  // The desktop app locks: so does the extension. It unlocks: so does the extension.
  say("lock");
  await line(/^locked$/);
  // The app tells connected extensions within half a second.
  await expect
    .poll(
      async () => {
        await popup.reload();
        return popup.getByRole("button", { name: "Unlock with desktop app" }).first().isVisible();
      },
      { timeout: 15_000 },
    )
    .toBe(true);
  say("unlock");
  await line(/^unlocked$/);
  await expect
    .poll(
      async () => {
        await popup.reload();
        return popup.getByRole("option", { name: /GitHub/ }).isVisible();
      },
      { timeout: 15_000 },
    )
    .toBe(true);
});
