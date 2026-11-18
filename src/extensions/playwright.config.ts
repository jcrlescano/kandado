import { defineConfig } from "@playwright/test";

// Loads the built extension (dist/edge) into a real browser, against the fake
// server the tests start. `pnpm e2e` builds it first.
// Branded Chrome no longer loads unpacked extensions from the command line, so the
// Chromium runs use Edge (channel msedge) unless KANDADO_BROWSER_CHANNEL says otherwise.
export default defineConfig({
  testDir: "tests/e2e",
  timeout: 90_000,
  workers: 1,
  reporter: [["list"]],
  outputDir: "test-results/playwright",
  use: { trace: "retain-on-failure" },
});
