// Draws the Kandado app icon: the logo (the padlock in
// src/desktop/resources/hardware/logos/logo-dark.svg) on a dark rounded tile,
// so it reads on light and dark taskbars and toolbars alike.
//
//   node scripts/make-icons.mjs
//
// Writes, and the repository keeps:
//   src/desktop/resources/app-icon/kandado.ico     the exe, window, taskbar, Start menu, installer
//   src/desktop/resources/app-icon/kandado-512.png the same, large, for listings
//   src/extensions/icons/icon-{16,32,48,128}.png  the browser extension
//
// No dependencies: the logo is three shapes, drawn here with 8x8 samples a
// pixel, and written as PNG (node:zlib) and ICO (PNG entries).
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { deflateSync } from "node:zlib";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");

// Kandado Hardware, dark theme (src/desktop/resources/tokens.json).
const TILE = [0x12, 0x15, 0x17]; // body
const INK = [0xec, 0xe9, 0xe2]; // ink: the shackle
const GREEN = [0x46, 0xa5, 0x85]; // green: the lock body

// The logo is 30 x 32 units; the tile is 40 x 40 with the logo centred.
const UNITS = 40;
const OFFSET = [5, 4];
const TILE_RADIUS = 9;

function roundRect(x, y, left, top, width, height, radius) {
  const cx = left + width / 2;
  const cy = top + height / 2;
  const qx = Math.abs(x - cx) - width / 2 + radius;
  const qy = Math.abs(y - cy) - height / 2 + radius;
  const outside = Math.hypot(Math.max(qx, 0), Math.max(qy, 0));
  return outside + Math.min(Math.max(qx, qy), 0) - radius;
}

function segment(x, y, ax, ay, bx, by) {
  const dx = bx - ax;
  const dy = by - ay;
  const t = Math.max(0, Math.min(1, ((x - ax) * dx + (y - ay) * dy) / (dx * dx + dy * dy)));
  return Math.hypot(x - ax - t * dx, y - ay - t * dy);
}

// "M8 13 V9 a7 7 0 0 1 14 0 V13", 3.2 wide with round caps.
function shackle(x, y) {
  const arc = y <= 9 ? Math.abs(Math.hypot(x - 15, y - 9) - 7) : Infinity;
  const sides = Math.min(segment(x, y, 8, 9, 8, 13), segment(x, y, 22, 9, 22, 13));
  return Math.min(arc, sides) - 1.6;
}

/** The colour at a point of the tile, in tile units, or null outside it. */
function paint(x, y) {
  if (roundRect(x, y, 0, 0, UNITS, UNITS, TILE_RADIUS) > 0) return null;
  const lx = x - OFFSET[0];
  const ly = y - OFFSET[1];
  const keyhole = Math.hypot(lx - 15, ly - 20) <= 2.6 || roundRect(lx, ly, 14, 21, 2, 5, 1) <= 0;
  if (roundRect(lx, ly, 3, 12, 24, 18, 5) <= 0) return keyhole ? TILE : GREEN;
  if (shackle(lx, ly) <= 0) return INK;
  return TILE;
}

/** RGBA pixels, `size` square. */
function draw(size) {
  const samples = 8;
  const pixels = Buffer.alloc(size * size * 4);
  for (let py = 0; py < size; py++) {
    for (let px = 0; px < size; px++) {
      let r = 0;
      let g = 0;
      let b = 0;
      let covered = 0;
      for (let sy = 0; sy < samples; sy++) {
        for (let sx = 0; sx < samples; sx++) {
          const x = ((px + (sx + 0.5) / samples) / size) * UNITS;
          const y = ((py + (sy + 0.5) / samples) / size) * UNITS;
          const colour = paint(x, y);
          if (!colour) continue;
          r += colour[0];
          g += colour[1];
          b += colour[2];
          covered++;
        }
      }
      const at = (py * size + px) * 4;
      if (covered) {
        pixels[at] = Math.round(r / covered);
        pixels[at + 1] = Math.round(g / covered);
        pixels[at + 2] = Math.round(b / covered);
        pixels[at + 3] = Math.round((covered / (samples * samples)) * 255);
      }
    }
  }
  return pixels;
}

const CRC_TABLE = Array.from({ length: 256 }, (_, n) => {
  let c = n;
  for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
  return c >>> 0;
});

function crc32(buffer) {
  let c = 0xffffffff;
  for (const byte of buffer) c = CRC_TABLE[(c ^ byte) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}

function chunk(type, data) {
  const length = Buffer.alloc(4);
  length.writeUInt32BE(data.length);
  const body = Buffer.concat([Buffer.from(type, "ascii"), data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(body));
  return Buffer.concat([length, body, crc]);
}

function png(size) {
  const pixels = draw(size);
  const header = Buffer.alloc(13);
  header.writeUInt32BE(size, 0);
  header.writeUInt32BE(size, 4);
  header[8] = 8; // bit depth
  header[9] = 6; // RGBA
  const rows = Buffer.alloc(size * (size * 4 + 1));
  for (let y = 0; y < size; y++) {
    pixels.copy(rows, y * (size * 4 + 1) + 1, y * size * 4, (y + 1) * size * 4);
  }
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk("IHDR", header),
    chunk("IDAT", deflateSync(rows, { level: 9 })),
    chunk("IEND", Buffer.alloc(0)),
  ]);
}

/** An ICO whose entries are PNGs (Windows Vista and later). */
function ico(sizes) {
  const images = sizes.map(png);
  const head = Buffer.alloc(6 + 16 * sizes.length);
  head.writeUInt16LE(0, 0);
  head.writeUInt16LE(1, 2); // icon
  head.writeUInt16LE(sizes.length, 4);
  let offset = head.length;
  sizes.forEach((size, index) => {
    const at = 6 + index * 16;
    head[at] = size >= 256 ? 0 : size;
    head[at + 1] = size >= 256 ? 0 : size;
    head.writeUInt16LE(1, at + 4); // planes
    head.writeUInt16LE(32, at + 6); // bits per pixel
    head.writeUInt32LE(images[index].length, at + 8);
    head.writeUInt32LE(offset, at + 12);
    offset += images[index].length;
  });
  return Buffer.concat([head, ...images]);
}

function write(path, data) {
  const full = resolve(root, path);
  mkdirSync(dirname(full), { recursive: true });
  writeFileSync(full, data);
  console.log(`wrote ${path} (${data.length} bytes)`);
}

write("src/desktop/resources/app-icon/kandado.ico", ico([16, 20, 24, 32, 40, 48, 64, 256]));
write("src/desktop/resources/app-icon/kandado-512.png", png(512));
for (const size of [16, 32, 48, 128]) {
  write(`src/extensions/icons/icon-${size}.png`, png(size));
}
