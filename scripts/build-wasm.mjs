// Builds kandado-wasm for the browser and writes the JavaScript wrapper next to it.
//
//   node scripts/build-wasm.mjs            release build into src/extensions/wasm/pkg
//   node scripts/build-wasm.mjs --dev      debug build (faster, larger)
//   node scripts/build-wasm.mjs --testing  with the test helpers, into src/extensions/wasm/pkg-test
//
// The extension build leaves out the `vectors` feature, so no exported function
// takes or returns a raw key (see src/extensions/wasm/src/raw.rs). Needs the
// wasm-bindgen CLI at the version pinned in Cargo.lock:
//   cargo install wasm-bindgen-cli --version 0.2.130 --locked
import { spawnSync } from "node:child_process";
import { mkdirSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const dev = process.argv.includes("--dev");
// Test build: adds the helpers that make accounts and vaults, in its own folder.
const testing = process.argv.includes("--testing");
const profile = dev ? "debug" : "release";
const outDir = resolve(root, testing ? "src/extensions/wasm/pkg-test" : "src/extensions/wasm/pkg");

function run(command, args) {
  const result = spawnSync(command, args, { cwd: root, stdio: "inherit" });
  if (result.status !== 0) {
    console.error(`\nFailed: ${command} ${args.join(" ")}`);
    process.exit(result.status ?? 1);
  }
}

run("cargo", [
  "build",
  "-p",
  "kandado-wasm",
  "--target",
  "wasm32-unknown-unknown",
  "--no-default-features",
  // Tests also get the stateless functions, to play the desktop app in the TypeScript tests.
  ...(testing ? ["--features", "testing,vectors"] : []),
  ...(dev ? [] : ["--release"]),
]);
mkdirSync(outDir, { recursive: true });
run("wasm-bindgen", [
  `target/wasm32-unknown-unknown/${profile}/kandado_wasm.wasm`,
  "--target",
  "web",
  "--out-dir",
  outDir,
  "--out-name",
  "kandado_wasm",
]);
console.log(`\nkandado-wasm written to ${outDir}`);
