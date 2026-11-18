// The autofill content script. It runs in every frame of the sites the extension
// may read, in the isolated world, and is the only Kandado code a page can reach.
// So it holds as little as possible: it finds fields, shows the Kandado mark in
// the focused one, places the dropdown (an extension page in an iframe, inside a
// closed shadow root), and writes values when the worker sends them after a pick.
// It never asks for a list or a secret itself.

import type { DropdownKey, Reply, WorkerMessage } from "../shared/messages";
import tokens from "../ui/tokens.json";
import logo from "../ui/vendor/logo.svg?raw";
import { captureFrom, formAround } from "./capture";
import { type DetectedForm, detect } from "./detect";
import { fill } from "./fill";

const DROPDOWN_WIDTH = 340;
const GAP = 4;
/** The save and update prompt: 340 px, with room for its shadow. */
const PROMPT_WIDTH = 364;

interface Open {
  token: string;
  input: HTMLInputElement;
  form: DetectedForm;
  /** The person moved the highlight with the arrow keys, so Enter picks. */
  navigated: boolean;
}

let open: Open | null = null;
/** The field the mark is shown in, even when no dropdown is open. */
let marked: { input: HTMLInputElement; form: DetectedForm } | null = null;
/** Set while a fill moves focus between fields, so it does not open a new dropdown. */
let filling = false;

// ---- the overlay: one host element, closed shadow root ---------------------------

const host = document.createElement("kandado-overlay");
const shadow = host.attachShadow({ mode: "closed" });
host.style.cssText =
  "all: initial; position: fixed; inset: 0 auto auto 0; width: 0; height: 0; z-index: 2147483647;";

function brass(scheme: "dark" | "light"): string {
  const value = tokens.color.tokens.find((token) => token.name === "brass")?.value;
  return typeof value === "object" ? value[scheme] : (value ?? "currentColor");
}

const style = document.createElement("style");
style.textContent = `
  .mark { position: fixed; width: 22px; height: 22px; padding: 2px; border: 0; margin: 0;
          background: transparent; cursor: pointer; color: ${brass("light")}; line-height: 0; }
  @media (prefers-color-scheme: dark) { .mark { color: ${brass("dark")}; } }
  .mark svg { width: 18px; height: 18px; }
  .mark [fill="#000"] { fill: currentColor; }
  .mark [stroke="#000"] { stroke: currentColor; }
  iframe { position: fixed; border: 0; width: ${DROPDOWN_WIDTH}px; height: 0;
           color-scheme: normal; background: transparent; }
  iframe.prompt { top: 4px; right: 4px; left: auto; width: ${PROMPT_WIDTH}px; }
`;
const mark = document.createElement("button");
mark.className = "mark";
mark.type = "button";
mark.tabIndex = -1;
mark.setAttribute("aria-label", "Kandado");
mark.innerHTML = logo.replace(/id="keyhole"/, 'id="k"').replace("url(#keyhole)", "url(#k)");
let frame: HTMLIFrameElement | null = null;
shadow.append(style);

function attach() {
  if (!host.isConnected) document.documentElement.append(host);
}

function place() {
  const target = open?.input ?? marked?.input;
  if (!target?.isConnected) {
    hideAll();
    return;
  }
  const rect = target.getBoundingClientRect();
  mark.style.left = `${rect.right - 28}px`;
  mark.style.top = `${rect.top + rect.height / 2 - 11}px`;
  if (frame) {
    const height = Number.parseFloat(frame.style.height) || 0;
    const below = rect.bottom + GAP;
    const fitsBelow = below + height <= window.innerHeight || rect.top - GAP - height < 0;
    // The dropdown page keeps 8 px either side for its shadow.
    const left = Math.max(0, Math.min(rect.left - 8, window.innerWidth - DROPDOWN_WIDTH));
    frame.style.left = `${left}px`;
    frame.style.top = `${fitsBelow ? below : rect.top - GAP - height}px`;
  }
}

function showMark(input: HTMLInputElement, form: DetectedForm) {
  marked = { input, form };
  attach();
  if (!mark.isConnected) shadow.append(mark);
  place();
}

function closeDropdown(tellWorker = true) {
  if (open && tellWorker) void send({ type: "dropdown-close", token: open.token });
  open = null;
  frame?.remove();
  frame = null;
}

function hideAll() {
  closeDropdown();
  marked = null;
  mark.remove();
}

// ---- talking to the worker --------------------------------------------------------

async function send<T>(message: unknown): Promise<T | null> {
  try {
    const reply = (await chrome.runtime.sendMessage(message)) as Reply<T> | undefined;
    return reply?.ok ? reply.value : null;
  } catch {
    return null; // The extension was reloaded or removed: this page's copy is stale.
  }
}

async function openDropdown(input: HTMLInputElement, form: DetectedForm) {
  const before = input.value;
  const token = await send<string | null>({ type: "dropdown-open", kind: form.kind });
  // The person may have moved on, or started typing, while the worker answered.
  if (!token) return;
  if (document.activeElement !== input || input.value !== before) {
    void send({ type: "dropdown-close", token });
    return;
  }
  closeDropdown();
  open = { token, input, form, navigated: false };
  frame = document.createElement("iframe");
  frame.src = `${chrome.runtime.getURL("src/content/dropdown/index.html")}?token=${token}`;
  frame.setAttribute("title", "Kandado");
  shadow.append(frame);
  place();
}

// ---- events -----------------------------------------------------------------------

function inputOf(target: EventTarget | null): HTMLInputElement | null {
  return target instanceof HTMLInputElement ? target : null;
}

document.addEventListener(
  "focusin",
  (event) => {
    if (!event.isTrusted || filling) return;
    const input = inputOf(event.composedPath()[0] ?? event.target);
    if (!input) return;
    if (open?.input === input) return;
    const form = detect(input, location.href);
    if (!form) {
      if (marked?.input !== input) hideAll();
      return;
    }
    showMark(input, form);
    void openDropdown(input, form);
  },
  true,
);

document.addEventListener(
  "focusout",
  (event) => {
    if (!event.isTrusted) return;
    // Focus went into the dropdown (the overlay host): keep it.
    setTimeout(() => {
      if (document.activeElement === host) return;
      const input = inputOf(document.activeElement);
      if (input && input === marked?.input) return;
      hideAll();
    }, 150);
  },
  true,
);

// Typing means the person is not picking: the dropdown gets out of the way (as the
// browser's own autofill does). The mark stays; clicking it opens the dropdown again.
document.addEventListener(
  "input",
  (event) => {
    if (event.isTrusted && !filling && open && event.target === open.input) closeDropdown();
  },
  true,
);

const KEYS: Record<string, DropdownKey> = {
  ArrowDown: "down",
  ArrowUp: "up",
  Enter: "enter",
  Escape: "escape",
};

document.addEventListener(
  "keydown",
  (event) => {
    // Only real key presses steer the dropdown: a page cannot pick for the person.
    if (!event.isTrusted || !open || event.target !== open.input) return;
    const key = KEYS[event.key];
    if (!key) return;
    if (key === "enter" && !open.navigated) return; // Enter submits the form as usual.
    if (key === "up" || key === "down") open.navigated = true;
    event.preventDefault();
    event.stopPropagation();
    void send({ type: "dropdown-key", token: open.token, key });
    if (key === "escape") closeDropdown(false);
  },
  true,
);

mark.addEventListener("mousedown", (event) => {
  event.preventDefault(); // keep focus in the field
});
mark.addEventListener("click", (event) => {
  if (!event.isTrusted || !marked) return;
  if (open) {
    closeDropdown();
    return;
  }
  marked.input.focus();
  void openDropdown(marked.input, marked.form);
});

window.addEventListener("scroll", () => place(), { capture: true, passive: true });
window.addEventListener("resize", () => place(), { passive: true });

// ---- save and update prompts (E-23, E-24) -------------------------------------------

let prompt: { token: string; frame: HTMLIFrameElement } | null = null;
let lastCapture = { key: "", at: 0 };

function closePrompt() {
  prompt?.frame.remove();
  prompt = null;
}

/** Shows the prompt if the worker has one waiting for this tab. Top frame only. */
async function checkPrompt() {
  if (window !== window.top || prompt) return;
  const token = await send<string | null>({ type: "prompt-check" });
  if (!token || prompt) return;
  attach();
  const element = document.createElement("iframe");
  element.className = "prompt";
  element.src = `${chrome.runtime.getURL("src/content/prompts/index.html")}?token=${token}`;
  element.setAttribute("title", "Kandado");
  shadow.append(element);
  prompt = { token, frame: element };
}

/** A form with a password is being submitted: hand its values to the worker. */
function captureAround(element: Element) {
  const form = formAround(element, location.href);
  const captured = form && captureFrom(form);
  if (!captured) return;
  // A click on the button and the submit that follows it are one capture.
  const key = `${captured.username}\u0000${captured.password}`;
  const now = Date.now();
  if (key === lastCapture.key && now - lastCapture.at < 2000) return;
  lastCapture = { key, at: now };
  void send({ type: "login-captured", ...captured });
  // If the page stays (a form handled by script), ask after a moment; if it
  // leaves, the next page asks when it loads.
  setTimeout(() => void checkPrompt(), 1500);
}

document.addEventListener(
  "submit",
  (event) => {
    if (event.isTrusted && event.target instanceof HTMLFormElement) captureAround(event.target);
  },
  true,
);

document.addEventListener(
  "click",
  (event) => {
    if (!event.isTrusted || !(event.target instanceof Element)) return;
    const button = event.target.closest('button, input[type="submit"], [role="button"]');
    if (button && !(button instanceof HTMLInputElement && button.type !== "submit")) {
      captureAround(button);
    }
  },
  true,
);

document.addEventListener(
  "keydown",
  (event) => {
    if (event.isTrusted && event.key === "Enter" && event.target instanceof HTMLInputElement) {
      captureAround(event.target);
    }
  },
  true,
);

void checkPrompt();

/** The first form of a kind on this frame, for a fill started from the popup. */
function firstForm(kind: DetectedForm["kind"]): DetectedForm | null {
  for (const input of Array.from(document.querySelectorAll("input"))) {
    const form = detect(input, location.href);
    if (form?.kind === kind) return form;
  }
  return null;
}

chrome.runtime.onMessage.addListener((message: WorkerMessage, sender, respond) => {
  // Only the extension's own worker talks to this script.
  if (sender.id !== chrome.runtime.id || sender.tab) return;
  switch (message.type) {
    case "kandado-fill-page": {
      const form = firstForm(message.kind);
      let written = 0;
      if (form) {
        filling = true;
        try {
          written = fill(form, message.values);
        } finally {
          filling = false;
        }
      }
      respond(written);
      break;
    }
    case "kandado-fill": {
      const target = open ?? marked;
      closeDropdown(false);
      if (target) {
        filling = true;
        try {
          fill(target.form, message.values);
        } finally {
          filling = false;
        }
      }
      break;
    }
    case "kandado-dropdown-size":
      if (frame && open?.token === message.token) {
        frame.style.height = `${message.height}px`;
        place();
      }
      break;
    case "kandado-prompt-size":
      if (prompt?.token === message.token) prompt.frame.style.height = `${message.height}px`;
      break;
    case "kandado-prompt-close":
      if (prompt?.token === message.token) closePrompt();
      break;
    case "kandado-dropdown-close":
      if (open?.token === message.token) closeDropdown(false);
      break;
  }
});
