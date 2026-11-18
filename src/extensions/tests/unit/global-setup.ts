import { spawnSync } from "node:child_process";
import { existsSync, statSync } from "node:fs";
import { resolve } from "node:path";

// Builds the WebAssembly core with the test helpers if it is missing or older than its sources.
export default function setup() {
  const root = resolve(import.meta.dirname, "../../../..");
  const wasm = resolve(root, "src/extensions/wasm/pkg-test/kandado_wasm_bg.wasm");
  if (existsSync(wasm) && statSync(wasm).mtimeMs > newestSource(root)) return;
  const result = spawnSync("node", ["scripts/build-wasm.mjs", "--testing", "--dev"], {
    cwd: root,
    stdio: "inherit",
  });
  if (result.status !== 0) throw new Error("building kandado-wasm failed");
}

function newestSource(root: string): number {
  const files = [
    "src/extensions/wasm/src/lib.rs",
    "src/extensions/wasm/src/session.rs",
    "src/extensions/wasm/src/raw.rs",
  ];
  return Math.max(...files.map((file) => statSync(resolve(root, file)).mtimeMs));
}
