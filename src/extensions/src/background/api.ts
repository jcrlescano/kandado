// The typed client for the Kandado API (`kandado-protocol`). Bearer access token,
// refreshed through the rotating refresh token. A refresh token that is used
// twice revokes the whole device, so two requests never refresh at once.

import type {
  AccountKeysJson,
  Me,
  Problem,
  PullResponse,
  PushRequest,
  PushResponse,
  TokenResponse,
} from "../shared/types";
import type { BreachList } from "./breaches";
import type { Remote } from "./sync";

export interface Tokens {
  access_token: string;
  /** Unix milliseconds. */
  expires_at: number;
  refresh_token: string;
}

/** Where tokens are kept between worker restarts (`chrome.storage.local` in the extension). */
export interface TokenStore {
  load(): Promise<Tokens | undefined>;
  save(tokens: Tokens | undefined): Promise<void>;
}

export class ApiError extends Error {
  constructor(
    readonly status: number,
    readonly code: string | undefined,
    message: string,
  ) {
    super(message);
    this.name = "ApiError";
  }

  /** The server no longer accepts this device's session: sign in again. */
  get sessionEnded(): boolean {
    return this.status === 401;
  }

  /** The step-up check: a fresh Google sign-in is needed (not used by the extension yet). */
  get stepUpRequired(): boolean {
    return this.status === 403 && this.code === "step_up_required";
  }
}

export interface ApiOptions {
  baseUrl: string;
  tokens: TokenStore;
  fetch?: typeof fetch;
  now?: () => number;
  /** Called when the session cannot be refreshed, so the UI can show E-01. */
  onSessionEnded?: () => void;
}

const EXPIRY_MARGIN_MS = 30_000;

export class Api implements Remote {
  private readonly baseUrl: string;
  private readonly fetchImpl: typeof fetch;
  private readonly now: () => number;
  private refreshing: Promise<Tokens> | null = null;

  constructor(private readonly options: ApiOptions) {
    this.baseUrl = options.baseUrl.replace(/\/+$/, "");
    this.fetchImpl = options.fetch ?? ((...args) => fetch(...args));
    this.now = options.now ?? Date.now;
  }

  // ---- public routes ------------------------------------------------------

  /** `POST /v1/auth/token`: trades the single-use code for tokens, and keeps them. */
  async exchangeCode(
    code: string,
    verifier: string,
    device: { name: string; platform: string },
  ): Promise<TokenResponse> {
    const response = await this.send<TokenResponse>("POST", "/v1/auth/token", {
      body: { code, verifier, device },
      auth: false,
    });
    if (response.refresh_token) {
      await this.options.tokens.save({
        access_token: response.access_token,
        expires_at: this.now() + response.expires_in * 1000,
        refresh_token: response.refresh_token,
      });
    }
    return response;
  }

  // ---- signed in ----------------------------------------------------------

  me(): Promise<Me> {
    return this.send<Me>("GET", "/v1/me");
  }

  /** The wrapped keys, as the JSON text the WebAssembly core reads. */
  /** `GET /v1/monitor/breaches`: Have I Been Pwned's breached sites (E-15). */
  breaches(): Promise<BreachList> {
    return this.send<BreachList>("GET", "/v1/monitor/breaches");
  }

  async accountKeys(): Promise<AccountKeysJson> {
    return JSON.stringify(await this.send<unknown>("GET", "/v1/account/keys"));
  }

  pull(cursor: number, limit: number): Promise<PullResponse> {
    return this.send<PullResponse>("GET", `/v1/sync?cursor=${cursor}&limit=${limit}`);
  }

  push(request: PushRequest): Promise<PushResponse> {
    return this.send<PushResponse>("POST", "/v1/sync", { body: request });
  }

  async logout(): Promise<void> {
    try {
      await this.send<unknown>("POST", "/v1/auth/logout");
    } finally {
      await this.options.tokens.save(undefined);
    }
  }

  /** The WebSocket address for `GET /v1/events`, and the token to send with it. */
  async eventsAccess(): Promise<{ url: string; token: string }> {
    const tokens = await this.freshTokens();
    return {
      url: `${this.baseUrl.replace(/^http/, "ws")}/v1/events`,
      token: tokens.access_token,
    };
  }

  // ---- plumbing -----------------------------------------------------------

  private async freshTokens(): Promise<Tokens> {
    const tokens = await this.options.tokens.load();
    if (!tokens) throw new ApiError(401, "signed_out", "not signed in");
    if (tokens.expires_at - EXPIRY_MARGIN_MS > this.now()) return tokens;
    return this.refresh(tokens);
  }

  private refresh(stale: Tokens): Promise<Tokens> {
    this.refreshing ??= (async () => {
      try {
        // Someone refreshed since this caller read its tokens: use theirs. Sending a
        // refresh token that was already rotated away would revoke the device.
        const current = await this.options.tokens.load();
        if (current && current.refresh_token !== stale.refresh_token) return current;
        const response = await this.send<TokenResponse>("POST", "/v1/auth/refresh", {
          body: { refresh_token: stale.refresh_token },
          auth: false,
        });
        const tokens: Tokens = {
          access_token: response.access_token,
          expires_at: this.now() + response.expires_in * 1000,
          refresh_token: response.refresh_token ?? stale.refresh_token,
        };
        // Saved before anything else uses it: the old refresh token is already dead.
        await this.options.tokens.save(tokens);
        return tokens;
      } catch (error) {
        if (error instanceof ApiError && error.sessionEnded) {
          await this.options.tokens.save(undefined);
          this.options.onSessionEnded?.();
        }
        throw error;
      } finally {
        this.refreshing = null;
      }
    })();
    return this.refreshing;
  }

  private async send<T>(
    method: "GET" | "POST",
    path: string,
    options: { body?: unknown; auth?: boolean; retried?: boolean } = {},
  ): Promise<T> {
    const headers: Record<string, string> = { accept: "application/json" };
    let used: Tokens | undefined;
    if (options.auth !== false) {
      used = await this.freshTokens();
      headers.authorization = `Bearer ${used.access_token}`;
    }
    if (options.body !== undefined) headers["content-type"] = "application/json";
    const response = await this.fetchImpl(`${this.baseUrl}${path}`, {
      method,
      headers,
      body: options.body === undefined ? undefined : JSON.stringify(options.body),
    });
    // The server may disagree about the clock: refresh once and try again.
    if (response.status === 401 && used && !options.retried) {
      await this.refresh(used);
      return this.send<T>(method, path, { ...options, retried: true });
    }
    if (!response.ok) {
      let problem: Partial<Problem> = {};
      try {
        problem = (await response.json()) as Problem;
      } catch {
        // Not a problem document.
      }
      throw new ApiError(
        response.status,
        problem.code,
        problem.title ?? `the server answered ${response.status}`,
      );
    }
    if (response.status === 204) return undefined as T;
    return (await response.json()) as T;
  }
}
