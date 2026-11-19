// Builds the secure link viewer that kandado-server serves at /l/{id}.
//
//   node scripts/build-viewer.mjs              release WebAssembly
//   node scripts/build-viewer.mjs --dev        debug WebAssembly
//   node scripts/build-viewer.mjs --skip-wasm  reuse src/extensions/wasm/pkg
//
// Output: src/backend/viewer/dist. The server reads it from KANDADO_VIEWER_DIR
// (default src/backend/viewer/dist). It is the page's own files, the Kandado
// Hardware styles the extension vendors, the two font files and the same
// WebAssembly build as the extension (no raw-key functions in it).
import { spawnSync } from "node:child_process";
import { copyFileSync, mkdirSync, rmSync } from "node:fs";
import { basename, dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const args = process.argv.slice(2);
const out = resolve(root, "src/backend/viewer/dist");

if (!args.includes("--skip-wasm")) {
  const result = spawnSync(
    "node",
    ["scripts/build-wasm.mjs", ...(args.includes("--dev") ? ["--dev"] : [])],
    { cwd: root, stdio: "inherit" },
  );
  if (result.status !== 0) process.exit(result.status ?? 1);
}

const files = [
  "src/backend/viewer/index.html",
  "src/backend/viewer/viewer.js",
  "src/backend/viewer/viewer.css",
  "src/extensions/src/ui/vendor/hardware/tokens.css",
  "src/extensions/src/ui/vendor/hardware/hardware.css",
  "src/extensions/src/ui/vendor/hardware/logo-light.svg",
  "src/extensions/src/ui/vendor/hardware/logo-dark.svg",
  "src/desktop/resources/fonts/SchibstedGrotesk.ttf",
  "src/desktop/resources/fonts/MartianMono.ttf",
  "src/extensions/wasm/pkg/kandado_wasm.js",
  "src/extensions/wasm/pkg/kandado_wasm_bg.wasm",
];

rmSync(out, { recursive: true, force: true });
mkdirSync(out, { recursive: true });
for (const file of files) {
  copyFileSync(resolve(root, file), resolve(out, basename(file)));
}
console.log(`\nViewer written to ${out}`);
