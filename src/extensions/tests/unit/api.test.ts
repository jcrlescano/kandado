import { describe, expect, it } from "vitest";
import { Api, ApiError, type TokenStore, type Tokens } from "../../src/background/api";
import { challengeFor, SignInError, signIn } from "../../src/background/auth";

function memoryTokens(initial?: Tokens): TokenStore & { current: Tokens | undefined } {
  const store = {
    current: initial,
    async load() {
      return store.current;
    },
    async save(tokens: Tokens | undefined) {
      store.current = tokens;
    },
  };
  return store;
}

const json = (body: unknown, status = 200) =>
  new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } });

describe("api", () => {
  it("refreshes once when several requests find the token expired, and saves the new one", async () => {
    let now = 1_000_000;
    const tokens = memoryTokens({ access_token: "old", expires_at: now - 1, refresh_token: "r1" });
    const seen: string[] = [];
    let refreshes = 0;
    const api = new Api({
      baseUrl: "http://server",
      tokens,
      now: () => now,
      fetch: async (input, init) => {
        const url = String(input);
        if (url.endsWith("/v1/auth/refresh")) {
          refreshes += 1;
          expect(JSON.parse(String(init?.body))).toEqual({ refresh_token: "r1" });
          await Promise.resolve();
          return json({
            access_token: "new",
            expires_in: 900,
            refresh_token: "r2",
            user_id: "u",
            device_id: "d",
          });
        }
        seen.push(new Headers(init?.headers).get("authorization") ?? "");
        return json({ id: "u" });
      },
    });
    await Promise.all([api.me(), api.me(), api.me()]);
    expect(refreshes).toBe(1);
    expect(seen).toEqual(["Bearer new", "Bearer new", "Bearer new"]);
    expect(tokens.current?.refresh_token).toBe("r2");
    now += 1;
  });

  it("never sends a refresh token that was already rotated away", async () => {
    // Another worker refreshed after this caller read its tokens.
    const tokens = memoryTokens({ access_token: "a", expires_at: 5_000_000, refresh_token: "r2" });
    let refreshed = false;
    const api = new Api({
      baseUrl: "http://server",
      tokens,
      now: () => 1_000_000,
      fetch: async (input) => {
        if (String(input).endsWith("/v1/auth/refresh")) refreshed = true;
        // The first answer says the token is expired; the retry works.
        return refreshed || tokens.current?.access_token === "a"
          ? json({ id: "u" }, 200)
          : json({}, 401);
      },
    });
    await api.me();
    expect(refreshed).toBe(false);
  });

  it("a 401 that survives a refresh ends the session", async () => {
    let ended = false;
    const tokens = memoryTokens({ access_token: "a", expires_at: 5_000_000, refresh_token: "r1" });
    const api = new Api({
      baseUrl: "http://server",
      tokens,
      now: () => 1_000_000,
      onSessionEnded: () => {
        ended = true;
      },
      fetch: async (input) =>
        String(input).endsWith("/v1/auth/refresh")
          ? json({ code: "unauthorized", title: "Not signed in", status: 401 }, 401)
          : json({ code: "unauthorized", title: "Not signed in", status: 401 }, 401),
    });
    await expect(api.me()).rejects.toMatchObject({ status: 401 });
    expect(ended).toBe(true);
    expect(tokens.current).toBeUndefined();
  });

  it("reads problem documents into ApiError", async () => {
    const api = new Api({
      baseUrl: "http://server",
      tokens: memoryTokens({ access_token: "a", expires_at: 5_000_000, refresh_token: "r" }),
      now: () => 1_000_000,
      fetch: async () =>
        json({ code: "step_up_required", title: "Confirmation needed", status: 403 }, 403),
    });
    const error = await api.me().catch((caught: unknown) => caught);
    expect(error).toBeInstanceOf(ApiError);
    expect((error as ApiError).stepUpRequired).toBe(true);
  });
});

describe("sign-in", () => {
  it("sends only the challenge, then trades the code and the verifier", async () => {
    let started = "";
    let exchanged: { code: string; verifier: string } | undefined;
    const tokens = memoryTokens();
    const api = new Api({
      baseUrl: "http://server",
      tokens,
      now: () => 0,
      fetch: async (_input, init) => {
        exchanged = JSON.parse(String(init?.body));
        return json({
          access_token: "t",
          expires_in: 900,
          refresh_token: "r",
          user_id: "u1",
          device_id: "d1",
        });
      },
    });
    const result = await signIn(
      api,
      "http://server",
      {
        redirectUrl: "https://abc.chromiumapp.org/",
        launch: async (url) => {
          started = url;
          return "https://abc.chromiumapp.org/?code=single-use";
        },
      },
      { name: "Chrome", platform: "Chrome" },
    );
    expect(result).toEqual({ user_id: "u1", device_id: "d1" });

    const query = new URL(started).searchParams;
    expect(query.get("client")).toBe("extension");
    expect(query.get("redirect")).toBe("https://abc.chromiumapp.org/");
    expect(started).not.toContain(exchanged?.verifier ?? "?");
    expect(query.get("challenge")).toBe(await challengeFor(exchanged?.verifier ?? ""));
    expect(exchanged?.code).toBe("single-use");
    expect(tokens.current?.refresh_token).toBe("r");
  });

  it("reports a refused or cancelled sign-in", async () => {
    const api = new Api({
      baseUrl: "http://server",
      tokens: memoryTokens(),
      fetch: async () => json({}),
    });
    const flow = (result: string | undefined) => ({
      redirectUrl: "https://abc.chromiumapp.org/",
      launch: async () => result,
    });
    const device = { name: "Chrome", platform: "Chrome" };
    await expect(signIn(api, "http://s", flow(undefined), device)).rejects.toMatchObject({
      reason: "cancelled",
    });
    await expect(
      signIn(api, "http://s", flow("https://abc.chromiumapp.org/?error=access_denied"), device),
    ).rejects.toBeInstanceOf(SignInError);
  });

  it("tells a closed window from a page that could not load", async () => {
    const api = new Api({
      baseUrl: "http://server",
      tokens: memoryTokens(),
      fetch: async () => json({}),
    });
    const rejecting = (message: string) => ({
      redirectUrl: "https://abc.chromiumapp.org/",
      launch: async () => {
        throw new Error(message);
      },
    });
    const device = { name: "Edge", platform: "Chrome" };
    for (const message of [
      "The user did not approve access.",
      "User cancelled or denied access.",
    ]) {
      await expect(signIn(api, "http://s", rejecting(message), device)).rejects.toMatchObject({
        reason: "cancelled",
      });
    }
    await expect(
      signIn(api, "http://s", rejecting("Authorization page could not be loaded."), device),
    ).rejects.toMatchObject({ reason: "failed" });
  });
});
