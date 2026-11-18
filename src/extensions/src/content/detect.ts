// Field detection: what a focused input is for, and which other inputs of its
// form belong with it. Runs in the page's isolated world. No secrets here: this
// only reads the page's markup.
//
// Signals, strongest first: the `autocomplete` attribute, the input's type, then
// its name, id, label and placeholder. A field that is hidden, disabled or
// read-only is never a target.

export type FieldRole =
  | "username"
  | "email"
  | "current-password"
  | "new-password"
  | "totp"
  | "cc-number"
  | "cc-name"
  | "cc-exp"
  | "cc-exp-month"
  | "cc-exp-year"
  | "cc-csc"
  | "name"
  | "given-name"
  | "family-name"
  | "tel"
  | "street-address"
  | "address-line2"
  | "city"
  | "region"
  | "postal-code"
  | "country"
  | "organization";

/** Which dropdown a focused field opens. */
export type FormKind = "login" | "new-password" | "totp" | "card" | "identity";

export interface DetectedForm {
  kind: FormKind;
  /** The inputs that take part, by role. A role may have several (password and confirm). */
  fields: Map<FieldRole, HTMLInputElement[]>;
}

const AUTOCOMPLETE: Record<string, FieldRole> = {
  username: "username",
  email: "email",
  "current-password": "current-password",
  "new-password": "new-password",
  "one-time-code": "totp",
  "cc-number": "cc-number",
  "cc-name": "cc-name",
  "cc-exp": "cc-exp",
  "cc-exp-month": "cc-exp-month",
  "cc-exp-year": "cc-exp-year",
  "cc-csc": "cc-csc",
  name: "name",
  "given-name": "given-name",
  "family-name": "family-name",
  tel: "tel",
  "street-address": "street-address",
  "address-line1": "street-address",
  "address-line2": "address-line2",
  "address-level2": "city",
  "address-level1": "region",
  "postal-code": "postal-code",
  country: "country",
  "country-name": "country",
  organization: "organization",
};

/** Name, id, label and placeholder heuristics, in order. The first match wins. */
const HINTS: [RegExp, FieldRole][] = [
  [
    /\b(one[-_ ]?time|otp|totp|2fa|mfa|two[-_ ]?factor|verification[-_ ]?code|auth(entication)?[-_ ]?code|security[-_ ]?code\b(?!.*card))/i,
    "totp",
  ],
  [/card[-_ ]?(number|no\b|num)|\bcc[-_ ]?num|cardnumber|\bpan\b/i, "cc-number"],
  [/(name[-_ ]?on[-_ ]?card|card[-_ ]?holder|cc[-_ ]?name|holder[-_ ]?name)/i, "cc-name"],
  [/\b(cvv|cvc|csc|cvn|card[-_ ]?code|security[-_ ]?code)\b/i, "cc-csc"],
  [/exp(iry|iration)?[-_ ]?(month|mm)\b|\bcc[-_ ]?month/i, "cc-exp-month"],
  [/exp(iry|iration)?[-_ ]?(year|yy)\b|\bcc[-_ ]?year/i, "cc-exp-year"],
  [/\bexp(iry|iration|ires)?\b|mm\s*\/\s*yy/i, "cc-exp"],
  [/\b(first[-_ ]?name|given[-_ ]?name|fname)\b/i, "given-name"],
  [/\b(last[-_ ]?name|family[-_ ]?name|surname|lname)\b/i, "family-name"],
  [/\b(full[-_ ]?name|your[-_ ]?name)\b|^name$/i, "name"],
  [/\b(phone|mobile|tel(ephone)?)\b/i, "tel"],
  [/\b(address[-_ ]?(line)?[-_ ]?2|apt|apartment|suite|unit)\b/i, "address-line2"],
  [/\b(street|address([-_ ]?(line)?[-_ ]?1)?)\b/i, "street-address"],
  [/\b(city|town|locality)\b/i, "city"],
  [/\b(state|province|region|county)\b/i, "region"],
  [/\b(zip|postal|post[-_ ]?code|postcode)\b/i, "postal-code"],
  [/\bcountry\b/i, "country"],
  [/\b(company|organi[sz]ation)\b/i, "organization"],
  [/e-?mail/i, "email"],
  [/\b(user(name)?|login|account|identifier)\b/i, "username"],
];

const NEW_PASSWORD_HINT = /(new|confirm|repeat|retype|again|create|choose|sign[-_ ]?up|register)/i;
const SIGN_UP_HINT = /(sign[-_ ]?up|register|create[-_ ]?account|join|change[-_ ]?password|reset)/i;

const TEXT_TYPES = new Set(["", "text", "email", "tel", "search", "number", "url"]);

/** Whether a person could type into this input. Layout is checked only where there is one. */
export function isUsable(input: HTMLInputElement): boolean {
  if (input.disabled || input.readOnly) return false;
  const type = input.type.toLowerCase();
  if (type !== "password" && !TEXT_TYPES.has(type)) return false;
  if (input.hidden || input.getAttribute("aria-hidden") === "true") return false;
  const style = input.ownerDocument.defaultView?.getComputedStyle(input);
  if (style && (style.display === "none" || style.visibility === "hidden")) return false;
  const rect = input.getBoundingClientRect();
  const hasLayout = input.ownerDocument.documentElement.getBoundingClientRect().width > 0;
  if (hasLayout && (rect.width < 4 || rect.height < 4)) return false;
  return true;
}

function labelText(input: HTMLInputElement): string {
  const parts: string[] = [];
  if (input.labels) for (const label of input.labels) parts.push(label.textContent ?? "");
  const labelledBy = input.getAttribute("aria-labelledby");
  if (labelledBy) {
    for (const id of labelledBy.split(/\s+/)) {
      parts.push(input.ownerDocument.getElementById(id)?.textContent ?? "");
    }
  }
  parts.push(input.getAttribute("aria-label") ?? "");
  return parts.join(" ");
}

/** The role of one input, or null if Kandado has nothing to fill there. */
export function roleOf(input: HTMLInputElement): FieldRole | null {
  const type = input.type.toLowerCase();
  const tokens = (input.getAttribute("autocomplete") ?? "").toLowerCase().split(/\s+/);
  // "off" says nothing about what the field is; the other signals still count.
  for (const token of [...tokens].reverse()) {
    const role = AUTOCOMPLETE[token];
    if (role) {
      // A password-type input keeps its password meaning whatever else it says.
      if (type === "password" && role !== "new-password" && role !== "current-password") break;
      return role;
    }
  }
  if (type === "password") {
    const text = `${input.name} ${input.id} ${labelText(input)} ${input.placeholder}`;
    return NEW_PASSWORD_HINT.test(text) ? "new-password" : "current-password";
  }
  if (type === "email") return "email";
  if (type === "tel") return "tel";
  for (const source of [`${input.name} ${input.id}`, labelText(input), input.placeholder]) {
    if (!source.trim()) continue;
    for (const [pattern, role] of HINTS) if (pattern.test(source)) return role;
  }
  return null;
}

/** The inputs that belong together: the form, or the nearest ancestor holding a few inputs. */
function scopeOf(input: HTMLInputElement): ParentNode {
  if (input.form) return input.form;
  let node: HTMLElement | null = input.parentElement;
  while (node && node !== input.ownerDocument.body) {
    if (node.querySelectorAll("input").length >= 2) return node;
    node = node.parentElement;
  }
  return input.ownerDocument;
}

const CARD_ROLES: ReadonlySet<FieldRole> = new Set([
  "cc-number",
  "cc-name",
  "cc-exp",
  "cc-exp-month",
  "cc-exp-year",
  "cc-csc",
]);
const IDENTITY_ROLES: ReadonlySet<FieldRole> = new Set([
  "name",
  "given-name",
  "family-name",
  "tel",
  "street-address",
  "address-line2",
  "city",
  "region",
  "postal-code",
  "country",
  "organization",
]);

/**
 * What the form around a focused input is, or null if Kandado has nothing to
 * offer there. Sign-up and change-password forms get the new-password dropdown
 * on their password fields; their email field stays a login field.
 */
export function detect(input: HTMLInputElement, pageUrl = ""): DetectedForm | null {
  if (!isUsable(input)) return null;
  const role = roleOf(input);
  if (!role) return null;

  const fields = new Map<FieldRole, HTMLInputElement[]>();
  const inputs = Array.from(scopeOf(input).querySelectorAll("input")).filter(isUsable);
  for (const candidate of inputs) {
    const candidateRole = roleOf(candidate);
    if (!candidateRole) continue;
    const list = fields.get(candidateRole) ?? [];
    list.push(candidate);
    fields.set(candidateRole, list);
  }
  const passwords = inputs.filter((candidate) => candidate.type.toLowerCase() === "password");

  if (role === "totp") return { kind: "totp", fields };
  if (CARD_ROLES.has(role)) return { kind: "card", fields };

  if (passwords.length > 0) {
    // A form with two password fields, or one marked new, is a sign-up or change.
    const formText = `${input.form?.getAttribute("action") ?? ""} ${input.form?.id ?? ""} ${pageUrl}`;
    const isNew =
      fields.has("new-password") || passwords.length >= 2 || SIGN_UP_HINT.test(formText);
    if (isNew) {
      // A change form has three: current, new, confirm. Otherwise every one is new.
      const current = passwords.length >= 3 ? passwords.slice(0, 1) : [];
      fields.set(
        "new-password",
        passwords.filter((field) => !current.includes(field)),
      );
      if (current.length > 0) fields.set("current-password", current);
      else fields.delete("current-password");
      if (input.type.toLowerCase() === "password" && current.includes(input)) {
        return { kind: "login", fields };
      }
      if (input.type.toLowerCase() === "password") {
        return { kind: "new-password", fields };
      }
      // The email or username of a sign-up form: logins for the site (E-19 comes with aliases).
      return role === "username" || role === "email" ? { kind: "login", fields } : null;
    }
    if (role === "current-password" || role === "username" || role === "email") {
      return { kind: "login", fields };
    }
  }
  // A username step on its own (the password comes on the next page).
  if (
    (role === "username" || role === "email") &&
    input.getAttribute("autocomplete")?.includes("username")
  ) {
    return { kind: "login", fields };
  }
  if (IDENTITY_ROLES.has(role) || role === "email") {
    const identityRoles = [...fields.keys()].filter((key) => IDENTITY_ROLES.has(key));
    // One lone phone or name box is a search or a comment form, not an address.
    if (identityRoles.length >= 2) return { kind: "identity", fields };
  }
  return null;
}
