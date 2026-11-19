// Copies the design tokens out of docs/ into the places source code reads them.
// Source code never reads from docs/ directly.
//
//   node scripts/sync-tokens.mjs           copy
//   node scripts/sync-tokens.mjs --check   fail if a copy is missing or out of date
//
// The extension gets its token values from scripts/sync-ui.mjs, with its CSS.
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const sources = {
  // host/build.rs turns this copy into Rust constants.
  "docs/design-system/project/tokens.json": ["src/desktop/resources/tokens.json"],
};

const check = process.argv.includes("--check");

let stale = 0;
for (const [source, copies] of Object.entries(sources)) {
  const expected = readFileSync(resolve(root, source));
  // Fail early with a clear message if the artifact ever exports something that is not JSON.
  JSON.parse(expected.toString("utf8"));

  for (const copy of copies) {
    const path = resolve(root, copy);
    let current = null;
    try {
      current = readFileSync(path);
    } catch {
      // Missing: handled below.
    }
    if (current?.equals(expected)) {
      continue;
    }
    if (check) {
      console.error(`${copy} is ${current ? "out of date" : "missing"} (source: ${source})`);
      stale += 1;
    } else {
      mkdirSync(dirname(path), { recursive: true });
      writeFileSync(path, expected);
      console.log(`updated ${copy}`);
    }
  }
}

if (stale > 0) {
  console.error("\nThe design tokens changed. Run: pnpm sync:tokens");
  process.exit(1);
}
console.log(check ? "Token copies are up to date." : "Tokens synced.");
