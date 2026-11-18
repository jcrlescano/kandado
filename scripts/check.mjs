// Runs every check CI runs, in the same order. Usage: pnpm check [--no-rust] [--no-js]
import { spawnSync } from "node:child_process";

const args = new Set(process.argv.slice(2));

const js = [
  // Biome is run through Node directly so this works whether or not pnpm is on PATH.
  ["node", ["node_modules/@biomejs/biome/bin/biome", "check", "."]],
  ["node", ["scripts/sync-tokens.mjs", "--check"]],
  ["node", ["scripts/sync-icons.mjs", "--check"]],
  ["node", ["scripts/sync-ui.mjs", "--check"]],
  // The extension: types, then unit tests (which build the WebAssembly core first;
  // that needs the wasm-bindgen CLI, see scripts/build-wasm.mjs).
  ["node", ["src/extensions/node_modules/typescript/bin/tsc", "-p", "src/extensions", "--noEmit"]],
  ["node", ["src/extensions/node_modules/vitest/vitest.mjs", "run", "--root", "src/extensions"]],
];

const rust = [
  ["cargo", ["fmt", "--all", "--check"]],
  ["cargo", ["clippy", "--workspace", "--all-targets", "--", "-D", "warnings"]],
  ["cargo", ["test", "--workspace"]],
];

const steps = [...(args.has("--no-js") ? [] : js), ...(args.has("--no-rust") ? [] : rust)];

for (const [command, commandArgs] of steps) {
  const label = `${command} ${commandArgs.join(" ")}`;
  console.log(`\n> ${label}`);
  const result = spawnSync(command, commandArgs, { stdio: "inherit" });
  if (result.status !== 0) {
    console.error(`\nFailed: ${label}`);
    process.exit(result.status ?? 1);
  }
}

console.log("\nAll checks passed.");
