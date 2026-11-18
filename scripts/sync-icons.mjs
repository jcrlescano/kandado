// Writes Kandado Hardware's icons and logo marks as SVG files for the desktop app.
// The icons come from docs/design-system/project/assets/Icons/icons.json (one
// entry per icon: the inner markup of a 24px, 1.9px-stroke SVG); the marks are
// the Logos group's files, copied as they are.
//
//   node scripts/sync-icons.mjs           write
//   node scripts/sync-icons.mjs --check   fail if a file is missing, extra or out of date
//
// GPUI draws an icon SVG as a one-colour mask in the current text colour, so
// "currentColor" becomes plain black here; any single ink would do.
import { mkdirSync, readdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const ds = "docs/design-system/project";
const check = process.argv.includes("--check");

const set = JSON.parse(readFileSync(resolve(root, ds, "assets/Icons/icons.json"), "utf8"));
if (!Array.isArray(set.names) || set.names.length === 0) {
  throw new Error("icons.json: no icon names");
}

function iconSvg(name) {
  const inner = set.icons[name];
  if (typeof inner !== "string") throw new Error(`icons.json: ${name} has no markup`);
  if (/<script|on\w+=|href=/i.test(inner))
    throw new Error(`icons.json: ${name} is not plain shapes`);
  const shapes = inner.replaceAll("currentColor", "#000");
  return `<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="${set.viewBox}" fill="none" stroke="#000" stroke-width="${set.strokeWidth}" stroke-linecap="round" stroke-linejoin="round">${shapes}</svg>\n`;
}

const mark = (theme) =>
  readFileSync(resolve(root, ds, `assets/Logos/kandado-mark-${theme}.svg`), "utf8");

const icons = new Map(set.names.map((name) => [`${name}.svg`, iconSvg(name)]));
// `logo.svg` is the mark as a one-colour silhouette, for places that draw it as
// a mask (build.rs requires it). The app draws the full-colour marks below.
icons.set("logo.svg", mark("light"));

const logos = new Map(["light", "dark"].map((theme) => [`logo-${theme}.svg`, mark(theme)]));

let stale = 0;
let total = 0;
// Makes outDir hold exactly these files, or with --check reports where it does not.
function sync(outDir, files) {
  total += files.size;
  const dir = resolve(root, outDir);
  let existing = [];
  try {
    existing = readdirSync(dir);
  } catch {
    // Missing directory: everything is missing.
  }
  for (const [name, content] of files) {
    let current = null;
    try {
      current = readFileSync(resolve(dir, name), "utf8");
    } catch {
      // Missing: handled below.
    }
    if (current === content) {
      continue;
    }
    if (check) {
      console.error(`${outDir}/${name} is ${current === null ? "missing" : "out of date"}`);
      stale += 1;
    } else {
      mkdirSync(dir, { recursive: true });
      writeFileSync(resolve(dir, name), content);
      console.log(`updated ${outDir}/${name}`);
    }
  }
  for (const name of existing.filter((file) => !files.has(file))) {
    if (check) {
      console.error(`${outDir}/${name} is not in the design system`);
      stale += 1;
    } else {
      rmSync(resolve(dir, name));
      console.log(`removed ${outDir}/${name}`);
    }
  }
}

sync("src/desktop/resources/icons", icons);
sync("src/desktop/resources/hardware/logos", logos);

if (stale > 0) {
  console.error("\nThe design system's icons changed. Run: pnpm sync:icons");
  process.exit(1);
}
console.log(check ? `${total} icon files are up to date.` : `${total} icon files synced.`);
