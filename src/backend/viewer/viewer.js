// The secure link viewer, served at /l/{id}. The key is in the fragment,
// which the browser never sends anywhere. This page reads it, takes it out of
// the address bar, asks the server for the encrypted payload (which counts one
// view), and decrypts it here with the Kandado core compiled to WebAssembly.
// Everything is written with textContent; nothing from the item is ever HTML.
import init, { openSecureLink, totpCode } from "/l/assets/kandado_wasm.js";

const MASK = "••••••••••••";
const KINDS = {
  login: "Login",
  card: "Card",
  note: "Note",
  identity: "Identity",
  password: "Password",
  alias: "Alias",
};

const id = location.pathname.split("/").filter(Boolean)[1] ?? "";
// A link key is 32 bytes as base64url: 43 characters.
let fragment = location.hash.replace(/^#/, "");
if (location.hash) history.replaceState(null, "", location.pathname);

const states = ["ready", "loading", "item", "gone", "broken", "failed"];
function show(state) {
  for (const name of states) {
    document.getElementById(`state-${name}`).hidden = name !== state;
  }
}

function base64ToBytes(text) {
  const binary = atob(text);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
  return bytes;
}

function el(tag, className, text) {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}

function key(label, onClick) {
  const button = el("button", "hw-key hw-key--sm", label);
  button.type = "button";
  button.addEventListener("click", onClick);
  return button;
}

async function copy(button, value) {
  try {
    await navigator.clipboard.writeText(value);
    const label = button.textContent;
    button.textContent = "Copied";
    setTimeout(() => {
      button.textContent = label;
    }, 1500);
  } catch {
    button.textContent = "Copy failed";
  }
}

function safeUrl(text) {
  try {
    const url = new URL(text.includes("://") ? text : `https://${text}`);
    return url.protocol === "https:" || url.protocol === "http:" ? url.href : null;
  } catch {
    return null;
  }
}

const timers = [];

function fieldRow(field) {
  const row = el("div", "viewer-field");
  const text = el("div", "viewer-field-text");
  text.append(el("span", "hw-label", field.label));
  const keys = el("div", "viewer-field-keys");
  const value = field.value ?? "";

  if (field.style === "secret") {
    const shown = el("span", "viewer-value viewer-value--mono", MASK);
    let revealed = false;
    const toggle = key("Reveal", () => {
      revealed = !revealed;
      shown.textContent = revealed ? value : MASK;
      toggle.textContent = revealed ? "Hide" : "Reveal";
    });
    text.append(shown);
    keys.append(toggle);
  } else if (field.style === "totp") {
    const code = el("span", "viewer-code", "------");
    const left = el("span", "hw-sub");
    const tick = () => {
      const now = Date.now() / 1000;
      try {
        const digits = totpCode(value, now);
        code.textContent = digits.replace(/^(\d{3})(\d+)$/, "$1 $2");
        left.textContent = `${30 - (Math.floor(now) % 30)} s`;
      } catch {
        code.textContent = "Not a valid setup key";
      }
    };
    tick();
    timers.push(setInterval(tick, 1000));
    text.append(code, left);
    keys.append(
      key("Copy", (event) => copy(event.currentTarget, totpCode(value, Date.now() / 1000))),
    );
    row.append(text, keys);
    return row;
  } else if (field.style === "url" && safeUrl(value)) {
    const link = el("a", "viewer-value", value);
    link.href = safeUrl(value);
    link.target = "_blank";
    link.rel = "noopener noreferrer";
    text.append(link);
  } else {
    text.append(el("span", "viewer-value", value));
  }
  keys.append(key("Copy", (event) => copy(event.currentTarget, value)));
  row.append(text, keys);
  return row;
}

function render(snapshot, opened) {
  document.getElementById("item-kind").textContent = KINDS[snapshot.kind] ?? "Item";
  document.getElementById("item-title").textContent = (snapshot.title || "Untitled").toUpperCase();
  document.getElementById("item-left").textContent =
    opened.views_left === null
      ? "NO VIEW LIMIT"
      : `${opened.views_left} ${opened.views_left === 1 ? "VIEW" : "VIEWS"} LEFT`;
  document.title = `${snapshot.title || "Item"} · Kandado`;

  const fields = document.getElementById("item-fields");
  fields.replaceChildren(...snapshot.fields.map(fieldRow));
  fields.hidden = snapshot.fields.length === 0;

  const notes = snapshot.notes ?? "";
  document.getElementById("item-notes-box").hidden = notes.trim() === "";
  document.getElementById("item-notes").textContent = notes;
  show("item");
}

async function open() {
  show("loading");
  let response;
  try {
    response = await fetch(`/v1/public/links/${encodeURIComponent(id)}/open`, {
      method: "POST",
      cache: "no-store",
      credentials: "omit",
      referrerPolicy: "no-referrer",
    });
  } catch {
    show("failed");
    return;
  }
  if (response.status === 404) {
    show("gone");
    return;
  }
  if (!response.ok) {
    show("failed");
    return;
  }
  try {
    const opened = await response.json();
    await init();
    const snapshot = JSON.parse(openSecureLink(id, fragment, base64ToBytes(opened.payload_enc)));
    fragment = "";
    render(snapshot, opened);
  } catch {
    show("broken");
  }
}

if (!/^[0-9a-fA-F-]{36}$/.test(id) || !/^[A-Za-z0-9_-]{43}$/.test(fragment)) {
  show("broken");
} else {
  show("ready");
}
document.getElementById("show").addEventListener("click", open);
document.getElementById("retry").addEventListener("click", open);
