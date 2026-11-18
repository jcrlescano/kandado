// @vitest-environment jsdom
import { describe, expect, it, vi } from "vitest";
import { detect, roleOf } from "../../src/content/detect";
import { fill } from "../../src/content/fill";

function page(html: string): Document {
  document.body.innerHTML = html;
  return document;
}

const input = (selector: string) => document.querySelector(selector) as HTMLInputElement;

describe("detection", () => {
  it("a sign-in form is a login form, from either field", () => {
    page(
      `<form><input name="login" id="u"><input type="password" id="p"><button>Sign in</button></form>`,
    );
    expect(detect(input("#u"))?.kind).toBe("login");
    const form = detect(input("#p"));
    expect(form?.kind).toBe("login");
    expect(form?.fields.get("username")?.[0]?.id).toBe("u");
    expect(form?.fields.get("current-password")?.[0]?.id).toBe("p");
  });

  it("autocomplete wins over the name", () => {
    page(
      `<form><input name="password_field" autocomplete="username" id="u"><input type="password" autocomplete="current-password"></form>`,
    );
    expect(roleOf(input("#u"))).toBe("username");
  });

  it("sign-up and change-password forms get the new-password dropdown", () => {
    page(
      `<form><input type="email" id="e"><input type="password" id="p1"><input type="password" id="p2"></form>`,
    );
    const form = detect(input("#p1"));
    expect(form?.kind).toBe("new-password");
    expect(form?.fields.get("new-password")?.map((f) => f.id)).toEqual(["p1", "p2"]);

    page(
      `<form><input type="password" id="old"><input type="password" id="new"><input type="password" id="again"></form>`,
    );
    expect(detect(input("#old"))?.kind).toBe("login");
    expect(
      detect(input("#new"))
        ?.fields.get("new-password")
        ?.map((f) => f.id),
    ).toEqual(["new", "again"]);

    page(
      `<form><input type="email" id="e"><input type="password" autocomplete="new-password" id="p"></form>`,
    );
    expect(detect(input("#p"))?.kind).toBe("new-password");
  });

  it("finds one-time code fields", () => {
    page(`<form><input name="otp" inputmode="numeric" id="c"></form>`);
    expect(detect(input("#c"))?.kind).toBe("totp");
    page(`<form><input autocomplete="one-time-code" id="c"></form>`);
    expect(detect(input("#c"))?.kind).toBe("totp");
  });

  it("finds card and address forms, and ignores a lone box", () => {
    page(
      `<form><input autocomplete="cc-number" id="n"><input autocomplete="cc-name"><input autocomplete="cc-exp"><input name="cvc"></form>`,
    );
    expect(detect(input("#n"))?.kind).toBe("card");
    page(
      `<form><label>Full name <input id="n"></label><label>Phone <input type="tel"></label><label>City <input></label></form>`,
    );
    expect(detect(input("#n"))?.kind).toBe("identity");
    page(`<form><input name="q" id="q" placeholder="Search"></form>`);
    expect(detect(input("#q"))).toBeNull();
    page(`<form><input type="tel" id="t"></form>`);
    expect(detect(input("#t"))).toBeNull();
  });

  it("never targets hidden, disabled or read-only inputs", () => {
    page(
      `<form><input name="user" id="u" disabled><input type="password" id="p" style="display:none"><input type="hidden" name="token" id="h"></form>`,
    );
    expect(detect(input("#u"))).toBeNull();
    expect(detect(input("#p"))).toBeNull();
    expect(detect(input("#h"))).toBeNull();
  });
});

describe("filling", () => {
  it("fills username and password the way typing does", () => {
    page(`<form><input name="login" id="u"><input type="password" id="p"></form>`);
    const events: string[] = [];
    input("#p").addEventListener("input", () => events.push("input"));
    input("#p").addEventListener("change", () => events.push("change"));
    const form = detect(input("#u"));
    if (!form) throw new Error("not detected");
    expect(fill(form, { username: "juan", password: "s3cret" })).toBe(2);
    expect(input("#u").value).toBe("juan");
    expect(input("#p").value).toBe("s3cret");
    expect(events).toEqual(["input", "change"]);
  });

  it("puts a generated password in every new-password field", () => {
    page(
      `<form><input type="email" id="e"><input type="password" id="p1"><input type="password" id="p2"></form>`,
    );
    const form = detect(input("#p1"));
    if (!form) throw new Error("not detected");
    fill(form, { newPassword: "cobalt-Meadow-anchor-7" });
    expect(input("#p1").value).toBe("cobalt-Meadow-anchor-7");
    expect(input("#p2").value).toBe("cobalt-Meadow-anchor-7");
    expect(input("#e").value).toBe("");
  });

  it("writes card expiry in the shape the field asks for", () => {
    page(
      `<form><input autocomplete="cc-number" id="n"><input autocomplete="cc-exp" placeholder="MM / YY" id="x"><input autocomplete="cc-csc" id="c"></form>`,
    );
    const form = detect(input("#n"));
    if (!form) throw new Error("not detected");
    fill(form, {
      card: { number: "4111111111111111", name: "", month: 8, year: 2029, cvc: "214" },
    });
    expect(input("#x").value).toBe("08/29");
    expect(input("#c").value).toBe("214");

    page(
      `<form><input autocomplete="cc-number" id="n"><input autocomplete="cc-exp-month" id="m"><input autocomplete="cc-exp-year" id="y"></form>`,
    );
    const split = detect(input("#n"));
    if (!split) throw new Error("not detected");
    fill(split, { card: { number: "4111", name: "", month: 8, year: 2029, cvc: "" } });
    expect([input("#m").value, input("#y").value]).toEqual(["08", "2029"]);
  });

  it("fills a React-controlled input through the native setter", () => {
    page(`<form><input name="login" id="u"><input type="password" id="p"></form>`);
    // A framework that shadows `value` on the instance, as React does.
    const field = input("#u");
    const tracker = vi.fn();
    Object.defineProperty(field, "value", {
      configurable: true,
      get: () => "",
      set: tracker,
    });
    const form = detect(input("#p"));
    if (!form) throw new Error("not detected");
    fill(form, { username: "juan" });
    expect(tracker).not.toHaveBeenCalled(); // the prototype setter was used instead
  });
});

describe("capture on submit", () => {
  it("takes the username and the current password of a login form", async () => {
    const { captureFrom, formAround } = await import("../../src/content/capture");
    page(
      `<form><input name="login" id="u" value="juan"><input type="password" id="p" value="pw-1"><button id="b">Sign in</button></form>`,
    );
    const form = formAround(input("#u").form?.querySelector("#b") as Element, "");
    expect(form && captureFrom(form)).toEqual({ username: "juan", password: "pw-1" });
  });

  it("takes the new password of a sign-up form, and nothing without a password", async () => {
    const { captureFrom, formAround } = await import("../../src/content/capture");
    page(
      `<form><input type="email" id="e" value="a@b.c"><input type="password" id="p1" value="new-1"><input type="password" value="new-1"><button id="b">Go</button></form>`,
    );
    const form = formAround(document.querySelector("#b") as Element, "");
    expect(form && captureFrom(form)).toEqual({ username: "a@b.c", password: "new-1" });
    page(
      `<form><input name="login" value="juan"><input type="password" id="p"><button id="b">Go</button></form>`,
    );
    const empty = formAround(document.querySelector("#b") as Element, "");
    expect(empty && captureFrom(empty)).toBeNull();
  });

  it("finds a form built without <form>", async () => {
    const { captureFrom, formAround } = await import("../../src/content/capture");
    page(
      `<div><div><input name="user" value="juan"><input type="password" value="pw"></div><div><button id="b">Log in</button></div></div>`,
    );
    const form = formAround(document.querySelector("#b") as Element, "");
    expect(form && captureFrom(form)).toEqual({ username: "juan", password: "pw" });
  });
});
