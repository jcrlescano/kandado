// Builds the browser extension, one folder per browser.
//
//   node scripts/build-extension.mjs                 all browsers, release WebAssembly
//   node scripts/build-extension.mjs --dev           debug WebAssembly, development server address
//   node scripts/build-extension.mjs --browser=chrome
//   node scripts/build-extension.mjs --skip-wasm     reuse src/extensions/wasm/pkg
//   node scripts/build-extension.mjs --server=URL    the API address (or KANDADO_SERVER_URL)
//
// Output: src/extensions/dist/{chrome,edge,firefox}. Load that folder as an
// unpacked extension. The manifest is the base plus the browser's own file.
import { spawnSync } from "node:child_process";
import { cpSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { createRequire } from "node:module";
import { dirname, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const extension = resolve(root, "src/extensions");
const args = process.argv.slice(2);
const dev = args.includes("--dev");
const only = args.find((arg) => arg.startsWith("--browser="))?.slice("--browser=".length);
const browsers = only ? [only] : ["chrome", "edge", "firefox"];

if (!args.includes("--skip-wasm")) {
  const result = spawnSync("node", ["scripts/build-wasm.mjs", ...(dev ? ["--dev"] : [])], {
    cwd: root,
    stdio: "inherit",
  });
  if (result.status !== 0) process.exit(result.status ?? 1);
}

// Vite is a dependency of the extension package, not of the root.
const require = createRequire(resolve(extension, "package.json"));
const { build } = await import(pathToFileURL(require.resolve("vite")).href);
const { default: react } = await import(
  pathToFileURL(require.resolve("@vitejs/plugin-react")).href
);

// The API the extension talks to. The release address is a placeholder until A9 (P1-9).
const server =
  args.find((arg) => arg.startsWith("--server="))?.slice("--server=".length) ??
  process.env.KANDADO_SERVER_URL ??
  (dev ? "http://127.0.0.1:8080" : "https://api.kandado.app");
const version = JSON.parse(readFileSync(resolve(extension, "package.json"), "utf8")).version;

/** The font files the extension pages load (src/extensions/src/ui/fonts.css). */
const FONTS = ["SchibstedGrotesk", "MartianMono"];

function merge(base, override) {
  const out = { ...base };
  for (const [key, value] of Object.entries(override)) {
    out[key] =
      value && typeof value === "object" && !Array.isArray(value) && typeof base[key] === "object"
        ? merge(base[key], value)
        : value;
  }
  return out;
}

function manifest(browser) {
  const read = (name) => JSON.parse(readFileSync(resolve(extension, "manifests", name), "utf8"));
  const merged = merge(read("base.json"), read(`${browser}.json`));
  merged.version = version;
  // Development Chromium builds get a fixed ID (see manifests/dev.json), so the
  // desktop app can let them use its native messaging host.
  if (dev && browser !== "firefox") merged.key = read("dev.json").key;
  const hosts = new Set(merged.host_permissions ?? []);
  hosts.add(`${new URL(server).origin}/*`);
  // Development builds may read and fill local fixture pages without the
  // all-sites permission the welcome tab asks for.
  if (dev) {
    hosts.add("http://localhost/*");
    hosts.add("http://127.0.0.1/*");
  }
  merged.host_permissions = [...hosts];
  return merged;
}

for (const browser of browsers) {
  const outDir = resolve(extension, "dist", browser);
  rmSync(outDir, { recursive: true, force: true });
  mkdirSync(outDir, { recursive: true });

  // The service worker (one ES module, with the WebAssembly file beside it) and
  // every extension page, in one build so they share chunks.
  await build({
    root: extension,
    configFile: false,
    logLevel: "warn",
    plugins: [react()],
    // Fonts are copied below from the desktop resources.
    publicDir: false,
    define: { "import.meta.env.VITE_KANDADO_SERVER_URL": JSON.stringify(server) },
    build: {
      outDir,
      emptyOutDir: false,
      target: "es2022",
      minify: !dev,
      sourcemap: dev,
      assetsInlineLimit: 0,
      // Not `lib` mode: that inlines every asset, and the WebAssembly file is 4 MB.
      rollupOptions: {
        input: {
          background: resolve(extension, "src/background/index.ts"),
          popup: resolve(extension, "src/popup/index.html"),
          dropdown: resolve(extension, "src/content/dropdown/index.html"),
          prompts: resolve(extension, "src/content/prompts/index.html"),
          welcome: resolve(extension, "src/pages/welcome/index.html"),
          settings: resolve(extension, "src/pages/settings/index.html"),
        },
        preserveEntrySignatures: "strict",
        output: {
          format: "es",
          entryFileNames: "[name].js",
          chunkFileNames: "chunks/[name]-[hash].js",
          assetFileNames: "assets/[name]-[hash][extname]",
        },
      },
    },
  });

  // The content script: a classic script (content scripts cannot be modules),
  // everything inlined, no React. It is the only code a page can reach.
  await build({
    root: extension,
    configFile: false,
    logLevel: "warn",
    publicDir: false,
    build: {
      outDir,
      emptyOutDir: false,
      target: "es2022",
      minify: !dev,
      sourcemap: dev ? "inline" : false,
      lib: {
        entry: resolve(extension, "src/content/index.ts"),
        formats: ["iife"],
        name: "kandadoContent",
        fileName: () => "content.js",
      },
    },
  });

  // The WebAuthn shim: runs in the page's main world, so it gets no extension
  // APIs and imports nothing.
  await build({
    root: extension,
    configFile: false,
    logLevel: "warn",
    publicDir: false,
    build: {
      outDir,
      emptyOutDir: false,
      target: "es2022",
      minify: !dev,
      sourcemap: dev ? "inline" : false,
      lib: {
        entry: resolve(extension, "src/content/webauthn-main.ts"),
        formats: ["iife"],
        name: "kandadoWebAuthn",
        fileName: () => "webauthn.js",
      },
    },
  });

  writeFileSync(
    resolve(outDir, "manifest.json"),
    `${JSON.stringify(manifest(browser), null, 2)}\n`,
  );
  // Kandado Hardware's two families, the same files as the desktop app (SIL OFL,
  // licences included).
  mkdirSync(resolve(outDir, "fonts"), { recursive: true });
  for (const family of FONTS) {
    for (const file of [`${family}.ttf`, `${family}-OFL.txt`]) {
      cpSync(resolve(root, "src/desktop/resources/fonts", file), resolve(outDir, "fonts", file));
    }
  }
  console.log(`built ${browser} -> ${outDir}`);
}
