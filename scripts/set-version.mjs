// Writes one version into every file that carries it, so a release needs only a tag.
//
//   node scripts/set-version.mjs 0.2.0
//
// The release workflow runs this with the version from the tag (v0.2.0) before it
// builds, so nobody bumps the version by hand or in a pull request. The files are:
//   Cargo.toml                    [workspace.package] version, read by every crate
//   Cargo.lock                    the workspace's own crates, so `--locked` still passes
//   package.json                  the pnpm workspace
//   src/extensions/package.json   the extension manifests (scripts/build-extension.mjs)
//
// The version is MAJOR.MINOR.PATCH, digits only: the desktop app's update check
// (src/desktop/crates/kandado-app/src/update.rs) reads no other form.
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const version = process.argv[2]?.replace(/^v/, "");

if (!version || !/^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.test(version)) {
  console.error(`Give a version like 1.2.3 (or v1.2.3), not "${process.argv[2] ?? ""}".`);
  process.exit(1);
}

function edit(path, change) {
  const file = resolve(root, path);
  const before = readFileSync(file, "utf8");
  const after = change(before);
  if (after === null) {
    console.error(`Found no version to set in ${path}.`);
    process.exit(1);
  }
  writeFileSync(file, after);
  console.log(`${path}: ${version}`);
}

// The first `version = "…"` after [workspace.package].
edit("Cargo.toml", (text) => {
  const pattern = /(\[workspace\.package\][^[]*?\nversion = ")[^"]*(")/;
  return pattern.test(text) ? text.replace(pattern, `$1${version}$2`) : null;
});

// Lockfile entries without a `source` are this workspace's crates.
edit("Cargo.lock", (text) => {
  let count = 0;
  const blocks = text.split(/(?=^\[\[package\]\]$)/m).map((block) => {
    if (!block.startsWith("[[package]]") || /^source = /m.test(block)) return block;
    count += 1;
    return block.replace(/^version = "[^"]*"$/m, `version = "${version}"`);
  });
  return count > 0 ? blocks.join("") : null;
});

for (const path of ["package.json", "src/extensions/package.json"]) {
  edit(path, (text) => {
    const pattern = /("version":\s*")[^"]*(")/;
    return pattern.test(text) ? text.replace(pattern, `$1${version}$2`) : null;
  });
}
