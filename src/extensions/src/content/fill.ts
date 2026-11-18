// Writes values into the page's inputs the way typing would, so frameworks that
// track input state (React, Vue, Angular) see the change. A value arrives here
// only at the moment the person picked an item, and is not kept.

import type { DetectedForm, FieldRole } from "./detect";

/** What the worker sends for one fill. Only the parts the form has are used. */
export interface FillValues {
  username?: string;
  password?: string;
  /** For sign-up and change forms: goes into every new-password field. */
  newPassword?: string;
  totp?: string;
  card?: {
    number: string;
    name: string;
    /** 1 to 12. */
    month: number | null;
    /** Four digits. */
    year: number | null;
    cvc: string;
  };
  identity?: {
    name: string;
    givenName: string;
    familyName: string;
    email: string;
    tel: string;
    street: string;
    line2: string;
    city: string;
    region: string;
    postalCode: string;
    country: string;
    organization: string;
  };
}

/** Sets an input's value through the native setter, then fires the events typing fires. */
export function setValue(input: HTMLInputElement, value: string): void {
  const prototype = Object.getPrototypeOf(input) as HTMLInputElement;
  const setter = Object.getOwnPropertyDescriptor(prototype, "value")?.set;
  input.focus();
  if (setter) setter.call(input, value);
  else input.value = value;
  input.dispatchEvent(new Event("input", { bubbles: true }));
  input.dispatchEvent(new Event("change", { bubbles: true }));
}

function expiryText(input: HTMLInputElement, month: number, year: number): string {
  const mm = String(month).padStart(2, "0");
  // Fit the field: "MM/YY" for short boxes and placeholders that ask for it, else "MM/YYYY".
  const hint = `${input.placeholder} ${input.getAttribute("maxlength") ?? ""}`;
  const long = /yyyy/i.test(hint) || Number(input.getAttribute("maxlength")) >= 7;
  return `${mm}/${long ? String(year) : String(year).slice(-2)}`;
}

function yearText(input: HTMLInputElement, year: number): string {
  return Number(input.getAttribute("maxlength")) === 2 || /^yy$/i.test(input.placeholder.trim())
    ? String(year).slice(-2)
    : String(year);
}

/** Fills the detected form. Returns how many inputs were written. */
export function fill(form: DetectedForm, values: FillValues): number {
  let written = 0;
  const put = (role: FieldRole, value: string | undefined, all = false) => {
    if (value === undefined || value === "") return;
    const targets = form.fields.get(role) ?? [];
    for (const input of all ? targets : targets.slice(0, 1)) {
      setValue(input, value);
      written += 1;
    }
  };

  if (values.username !== undefined) {
    if (form.fields.has("username")) put("username", values.username);
    else put("email", values.username);
  }
  put("current-password", values.password);
  // A login form whose only password field looked new (no hint either way).
  if (values.password !== undefined && !form.fields.has("current-password")) {
    put("new-password", values.password);
  }
  put("new-password", values.newPassword, true);
  put("totp", values.totp);

  if (values.card) {
    const card = values.card;
    put("cc-number", card.number);
    put("cc-name", card.name);
    put("cc-csc", card.cvc);
    if (card.month !== null && card.year !== null) {
      for (const input of form.fields.get("cc-exp") ?? []) {
        setValue(input, expiryText(input, card.month, card.year));
        written += 1;
      }
      put("cc-exp-month", String(card.month).padStart(2, "0"));
      for (const input of form.fields.get("cc-exp-year") ?? []) {
        setValue(input, yearText(input, card.year));
        written += 1;
      }
    }
  }

  if (values.identity) {
    const who = values.identity;
    put("name", who.name);
    put("given-name", who.givenName);
    put("family-name", who.familyName);
    put("email", who.email);
    put("tel", who.tel);
    put("street-address", who.street);
    put("address-line2", who.line2);
    put("city", who.city);
    put("region", who.region);
    put("postal-code", who.postalCode);
    put("country", who.country);
    put("organization", who.organization);
  }
  return written;
}
