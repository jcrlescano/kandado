// Writes the design system's icons and logo mark as SVG files for the desktop app.
// They are drawn by code in docs/design-system (the Icon and Logo components), so
// this runs those components with a stand-in for React and saves what they return.
//
//   node scripts/sync-icons.mjs           write
//   node scripts/sync-icons.mjs --check   fail if a file is missing, extra or out of date
import { mkdirSync, readdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import vm from "node:vm";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const source = "docs/design-system/project/components/src/components.js";
const outDir = "src/desktop/resources/icons";
const check = process.argv.includes("--check");

// Just enough React for Icon and Logo: elements become plain objects.
const React = {
  createElement: (type, props, ...children) => ({
    type,
    props: props ?? {},
    children: children.flat(),
  }),
  useState: (initial) => [typeof initial === "function" ? initial() : initial, () => {}],
  useEffect: () => {},
  useRef: () => ({ current: null }),
};
const window = { React };
vm.runInNewContext(readFileSync(resolve(root, source), "utf8"), { window, Object, Math, String });
const { Icon, Logo, ICON_NAMES } = window.Kandado;

const ATTRIBUTE_NAMES = {
  strokeWidth: "stroke-width",
  strokeLinecap: "stroke-linecap",
  strokeLinejoin: "stroke-linejoin",
};
// Not meaningful in a file: React and accessibility props, and ids that differ per render.
const DROPPED = new Set(["className", "key", "role", "focusable", "aria-hidden", "aria-label"]);

function attributes(props) {
  return Object.entries(props)
    .filter(([name, value]) => !DROPPED.has(name) && value !== undefined && value !== null)
    .map(([name, value]) => {
      // GPUI draws an SVG as a mask in the current text colour, so any paint will do.
      const text = String(value).replace(/var\(--[a-z-]+\)|currentColor/g, "#000");
      return ` ${ATTRIBUTE_NAMES[name] ?? name}="${text}"`;
    })
    .join("");
}

function render(element, depth = 0) {
  if (typeof element !== "object" || element === null) {
    return "";
  }
  const indent = "  ".repeat(depth);
  const open = `${indent}<${element.type}${depth === 0 ? ' xmlns="http://www.w3.org/2000/svg"' : ""}${attributes(element.props)}`;
  const children = element.children.map((child) => render(child, depth + 1)).filter(Boolean);
  if (children.length === 0) {
    return `${open}/>\n`;
  }
  return `${open}>\n${children.join("")}${indent}</${element.type}>\n`;
}

const files = new Map();
for (const name of ICON_NAMES) {
  files.set(`${name}.svg`, render(Icon({ name, size: 24 })));
}
// The mark alone; the wordmark is text set in the display font.
files.set("logo.svg", render(Logo({ size: 32 })).replaceAll("kd-keyhole-1", "keyhole"));

const dir = resolve(root, outDir);
let existing = [];
try {
  existing = readdirSync(dir);
} catch {
  // Missing directory: everything is missing.
}

let stale = 0;
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

if (stale > 0) {
  console.error(`\nThe design system's icons changed. Run: pnpm sync:icons`);
  process.exit(1);
}
console.log(
  check ? `${files.size} icon files are up to date.` : `${files.size} icon files synced.`,
);
