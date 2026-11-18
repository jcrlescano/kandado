import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    environment: "node",
    include: ["src/**/*.test.ts", "tests/unit/**/*.test.ts"],
    // The WebAssembly core is built into wasm/pkg-test first (see globalSetup).
    globalSetup: ["tests/unit/global-setup.ts"],
    testTimeout: 30_000,
  },
});
