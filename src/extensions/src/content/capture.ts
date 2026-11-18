// What a submitted form holds, for the save and update prompts (E-23, E-24).
// Read only when the person submits a form that has a password in it.

import { type DetectedForm, detect } from "./detect";

export interface Capture {
  username: string;
  password: string;
}

function firstValue(form: DetectedForm, ...roles: Parameters<DetectedForm["fields"]["get"]>[0][]) {
  for (const role of roles) {
    for (const input of form.fields.get(role) ?? []) {
      if (input.value) return input.value;
    }
  }
  return "";
}

/** The username and password in a login, sign-up or change form, or null. */
export function captureFrom(form: DetectedForm): Capture | null {
  if (form.kind !== "login" && form.kind !== "new-password") return null;
  // On sign-up and change forms the new password is the one to keep.
  const password = form.fields.has("new-password")
    ? firstValue(form, "new-password")
    : firstValue(form, "current-password");
  if (!password) return null;
  return { username: firstValue(form, "username", "email"), password };
}

/** The form around an element (a submit button, a field), if it has a password field. */
export function formAround(element: Element, pageUrl: string): DetectedForm | null {
  let scope: Element | null = element.closest("form");
  if (!scope) {
    // Forms built without <form>: the nearest ancestor with a password field.
    let node: Element | null = element.parentElement;
    while (node && !node.querySelector('input[type="password"]')) node = node.parentElement;
    scope = node;
  }
  if (!scope) return null;
  const passwords = Array.from(scope.querySelectorAll<HTMLInputElement>('input[type="password"]'));
  for (const field of passwords) {
    const form = detect(field, pageUrl);
    if (form && (form.kind === "login" || form.kind === "new-password")) return form;
  }
  return null;
}
