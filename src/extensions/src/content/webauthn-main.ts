// The WebAuthn shim. Runs in the page's main world at document_start, in the
// top frame only, and wraps navigator.credentials.create and .get.
//
// It holds nothing: it turns the page's options into plain values (binary as
// base64url), hands them to Kandado's content script with window.postMessage,
// and turns the answer back into a PublicKeyCredential. When Kandado does not
// take part, or the person picks "Use another device", it calls the browser's
// own function with the page's original options, so nothing changes for them.
//
// A page can post to this channel too, but it gains nothing: the request is
// one it could make itself, the origin is taken by the worker from the
// browser, and the answer only ever goes back to this window.

const CHANNEL = "kandado-webauthn";
/** How long to wait for the content script (document_idle) before giving up on Kandado. */
const READY_TIMEOUT_MS = 4000;

type Outcome =
  | { kind: "native" }
  | { kind: "error"; name: string; message: string }
  | { kind: "created"; response: Registration }
  | { kind: "asserted"; response: Assertion };

interface Registration {
  credential_id: string;
  client_data_json: string;
  attestation_object: string;
  authenticator_data: string;
  public_key: string;
  public_key_algorithm: number;
}

interface Assertion {
  credential_id: string;
  client_data_json: string;
  authenticator_data: string;
  signature: string;
  user_handle: string;
}

(() => {
  const credentials = navigator.credentials;
  if (!credentials || typeof PublicKeyCredential === "undefined") return;
  const native = {
    create: credentials.create.bind(credentials),
    get: credentials.get.bind(credentials),
  };

  let ready = false;
  const waiting: (() => void)[] = [];
  const answers = new Map<string, (outcome: Outcome) => void>();

  window.addEventListener("message", (event) => {
    if (event.source !== window) return;
    const data = event.data as { source?: string; dir?: string; id?: string; outcome?: Outcome };
    if (data?.source !== CHANNEL) return;
    if (data.dir === "ready") {
      ready = true;
      for (const resume of waiting.splice(0)) resume();
    } else if (data.dir === "outcome" && typeof data.id === "string" && data.outcome) {
      answers.get(data.id)?.(data.outcome);
      answers.delete(data.id);
    }
  });

  function untilReady(): Promise<boolean> {
    if (ready) return Promise.resolve(true);
    return new Promise((resolve) => {
      const timer = setTimeout(() => resolve(false), READY_TIMEOUT_MS);
      waiting.push(() => {
        clearTimeout(timer);
        resolve(true);
      });
    });
  }

  function b64(source: BufferSource | undefined | null): string {
    if (!source) return "";
    const bytes =
      source instanceof ArrayBuffer
        ? new Uint8Array(source)
        : new Uint8Array(source.buffer, source.byteOffset, source.byteLength);
    let binary = "";
    for (const byte of bytes) binary += String.fromCharCode(byte);
    return btoa(binary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
  }

  function bytes(text: string): ArrayBuffer {
    const binary = atob(text.replace(/-/g, "+").replace(/_/g, "/"));
    const out = new Uint8Array(binary.length);
    for (let i = 0; i < binary.length; i++) out[i] = binary.charCodeAt(i);
    return out.buffer;
  }

  async function ask(
    op: "create" | "get",
    request: Record<string, unknown>,
    signal?: AbortSignal | null,
  ): Promise<Outcome> {
    if (signal?.aborted) return { kind: "error", name: "AbortError", message: "Aborted." };
    if (!(await untilReady())) return { kind: "native" };
    const id = crypto.randomUUID();
    return new Promise((resolve) => {
      answers.set(id, resolve);
      signal?.addEventListener(
        "abort",
        () => {
          window.postMessage({ source: CHANNEL, dir: "abort", id }, window.location.origin);
          answers.delete(id);
          resolve({ kind: "error", name: "AbortError", message: "Aborted." });
        },
        { once: true },
      );
      window.postMessage(
        { source: CHANNEL, dir: "request", id, op, request },
        window.location.origin,
      );
    });
  }

  function define(target: object, values: Record<string, unknown>): void {
    for (const [name, value] of Object.entries(values)) {
      Object.defineProperty(target, name, { value, enumerable: true, configurable: true });
    }
  }

  /** A PublicKeyCredential the page can use like the browser's own. */
  function credential(
    id: string,
    response: object,
    responseJson: Record<string, unknown>,
    extensions: Record<string, unknown>,
  ): PublicKeyCredential {
    const out = Object.create(PublicKeyCredential.prototype) as PublicKeyCredential;
    define(out, {
      id,
      rawId: bytes(id),
      type: "public-key",
      response,
      authenticatorAttachment: "platform",
      getClientExtensionResults: () => extensions,
      toJSON: () => ({
        id,
        rawId: id,
        type: "public-key",
        authenticatorAttachment: "platform",
        clientExtensionResults: extensions,
        response: responseJson,
      }),
    });
    return out;
  }

  function created(r: Registration, wantsCredProps: boolean): PublicKeyCredential {
    const response = Object.create(AuthenticatorAttestationResponse.prototype);
    define(response, {
      clientDataJSON: bytes(r.client_data_json),
      attestationObject: bytes(r.attestation_object),
      getAuthenticatorData: () => bytes(r.authenticator_data),
      getPublicKey: () => bytes(r.public_key),
      getPublicKeyAlgorithm: () => r.public_key_algorithm,
      getTransports: () => ["hybrid", "internal"],
    });
    return credential(
      r.credential_id,
      response,
      {
        clientDataJSON: r.client_data_json,
        attestationObject: r.attestation_object,
        authenticatorData: r.authenticator_data,
        publicKey: r.public_key,
        publicKeyAlgorithm: r.public_key_algorithm,
        transports: ["hybrid", "internal"],
      },
      wantsCredProps ? { credProps: { rk: true } } : {},
    );
  }

  function asserted(r: Assertion): PublicKeyCredential {
    const response = Object.create(AuthenticatorAssertionResponse.prototype);
    define(response, {
      clientDataJSON: bytes(r.client_data_json),
      authenticatorData: bytes(r.authenticator_data),
      signature: bytes(r.signature),
      userHandle: r.user_handle ? bytes(r.user_handle) : null,
    });
    return credential(
      r.credential_id,
      response,
      {
        clientDataJSON: r.client_data_json,
        authenticatorData: r.authenticator_data,
        signature: r.signature,
        userHandle: r.user_handle || null,
      },
      {},
    );
  }

  function fail(outcome: { name: string; message: string }): never {
    throw new DOMException(outcome.message, outcome.name);
  }

  credentials.create = async function create(options?: CredentialCreationOptions) {
    const key = options?.publicKey;
    if (!key) return native.create(options);
    const outcome = await ask(
      "create",
      {
        rp_id: key.rp?.id,
        rp_name: key.rp?.name,
        user_id: b64(key.user?.id),
        user_name: key.user?.name,
        user_display_name: key.user?.displayName,
        challenge: b64(key.challenge),
        algorithms: (key.pubKeyCredParams ?? []).map((param) => param.alg),
        exclude_credentials: (key.excludeCredentials ?? []).map((cred) => b64(cred.id)),
      },
      options?.signal,
    );
    switch (outcome.kind) {
      case "created":
        return created(outcome.response, key.extensions?.credProps === true);
      case "error":
        return fail(outcome);
      default:
        return native.create(options);
    }
  };

  credentials.get = async function get(options?: CredentialRequestOptions) {
    const key = options?.publicKey;
    // The browser's own autofill list (conditional mediation) stays the browser's.
    if (!key || options?.mediation === "conditional") return native.get(options);
    const outcome = await ask(
      "get",
      {
        rp_id: key.rpId,
        challenge: b64(key.challenge),
        allow_credentials: (key.allowCredentials ?? []).map((cred) => b64(cred.id)),
      },
      options?.signal,
    );
    switch (outcome.kind) {
      case "asserted":
        return asserted(outcome.response);
      case "error":
        return fail(outcome);
      default:
        return native.get(options);
    }
  };
})();
