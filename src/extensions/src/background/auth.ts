// Sign in with Google through the server (PKCE-style handoff, docs/architecture/security.md).
//
// The extension makes a random `verifier` and sends only its SHA-256 `challenge`.
// The server runs the Google flow, and redirects the browser to the extension's
// own redirect URL with a single-use code. The extension trades code and verifier
// for tokens. Google proves identity only; it decrypts nothing.

import { toBase64Url } from "../shared/bytes";
import type { Api } from "./api";

export interface IdentityFlow {
  /** `chrome.identity.getRedirectURL()`: `https://{id}.chromiumapp.org/`. */
  redirectUrl: string;
  /** `chrome.identity.launchWebAuthFlow({ url, interactive: true })`. */
  launch(url: string): Promise<string | undefined>;
}

export class SignInError extends Error {
  constructor(
    readonly reason: "cancelled" | "access_denied" | "wrong_account" | "sign_in_failed" | "failed",
  ) {
    super(reason);
    this.name = "SignInError";
  }
}

export async function challengeFor(verifier: string): Promise<string> {
  const digest = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(verifier));
  return toBase64Url(new Uint8Array(digest));
}

function randomVerifier(): string {
  const bytes = new Uint8Array(32);
  crypto.getRandomValues(bytes);
  return toBase64Url(bytes);
}

/** Runs the whole sign-in and leaves the tokens saved. Returns the user and device ids. */
export async function signIn(
  api: Api,
  baseUrl: string,
  identity: IdentityFlow,
  device: { name: string; platform: string },
): Promise<{ user_id: string; device_id: string }> {
  const verifier = randomVerifier();
  const start = new URL("/v1/auth/google/start", baseUrl);
  start.searchParams.set("client", "extension");
  start.searchParams.set("challenge", await challengeFor(verifier));
  start.searchParams.set("redirect", identity.redirectUrl);

  let result: string | undefined;
  try {
    result = await identity.launch(start.toString());
  } catch {
    throw new SignInError("cancelled");
  }
  if (!result) throw new SignInError("cancelled");

  const params = new URL(result).searchParams;
  const error = params.get("error");
  if (error) {
    throw new SignInError(
      error === "access_denied" || error === "wrong_account" || error === "sign_in_failed"
        ? error
        : "failed",
    );
  }
  const code = params.get("code");
  if (!code) throw new SignInError("failed");

  const tokens = await api.exchangeCode(code, verifier, device);
  return { user_id: tokens.user_id, device_id: tokens.device_id };
}
