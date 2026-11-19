// Prints the Chromium extension ID that a manifest key file gives, the way
// Chrome and Edge work it out: the first 16 bytes of the SHA-256 of the public
// key, one letter a to p per half-byte.
//
//   node scripts/extension-id.mjs            the release zips (manifests/release.json)
//   node scripts/extension-id.mjs dev.json   the development builds
//   node scripts/extension-id.mjs --host     as a sign-in redirect host, {id}.chromiumapp.org
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const args = process.argv.slice(2);
const name = args.find((arg) => !arg.startsWith("--")) ?? "release.json";
const { key } = JSON.parse(readFileSync(resolve(root, "src/extensions/manifests", name), "utf8"));
const digest = createHash("sha256").update(Buffer.from(key, "base64")).digest();
const id = [...digest.subarray(0, 16)]
  .map((byte) => String.fromCharCode(97 + (byte >> 4), 97 + (byte & 15)))
  .join("");
console.log(args.includes("--host") ? `${id}.chromiumapp.org` : id);
