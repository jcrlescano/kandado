// Passkeys (E-25, E-26) in the worker, over the real WebAssembly core.
//
// The registrations and assertions the core makes are checked with Node's Web
// Crypto, which shares no code with the Rust p256 crate that made them: the
// way a relying party would check them.

import { beforeAll, describe, expect, it } from "vitest";
import type { Keyring } from "../../src/background/keyring";
import {
  type PasskeyContext,
  type PasskeyFrame,
  type PasskeyOutcome,
  type PasskeyRequest,
  Passkeys,
} from "../../src/background/passkeys";
import { sync } from "../../src/background/sync";
import { uuidv7 } from "../../src/shared/bytes";
import { FakeServer, login, newDevice, newWorld, testCore, type World } from "./helpers";

const fromB64 = (text: string): Uint8Array<ArrayBuffer> =>
  Uint8Array.from(Buffer.from(text, "base64url"));
const b64 = (bytes: Uint8Array) => Buffer.from(bytes).toString("base64url");
const sha256 = async (bytes: Uint8Array<ArrayBuffer>) =>
  new Uint8Array(await crypto.subtle.digest("SHA-256", bytes));

const TOP: PasskeyFrame = { tabId: 7, frameId: 0, url: "https://www.example.com/settings" };

function createCall(extra: Record<string, unknown> = {}): PasskeyRequest {
  return {
    op: "create",
    request: {
      rp_id: "example.com",
      rp_name: "Example",
      user_id: b64(new TextEncoder().encode("user-42")),
      user_name: "juan@example.com",
      user_display_name: "Juan",
      challenge: b64(new Uint8Array(32).fill(1)),
      algorithms: [-7, -257],
      ...extra,
    },
  } as PasskeyRequest;
}

function getCall(extra: Record<string, unknown> = {}): PasskeyRequest {
  return {
    op: "get",
    request: { rp_id: "example.com", challenge: b64(new Uint8Array(32).fill(2)), ...extra },
  } as PasskeyRequest;
}

/** A byte that must be there (the tests read fixed layouts). */
const byteAt = (bytes: Uint8Array, index: number): number => {
  const value = bytes[index];
  if (value === undefined) throw new Error(`no byte ${index}`);
  return value;
};

/** The COSE key's x and y, read out of a "none" attestation object. */
function coseXY(attestation: Uint8Array): { authData: Uint8Array; x: Uint8Array; y: Uint8Array } {
  const marker = new TextEncoder().encode("authData");
  let at = -1;
  for (let i = 0; i + marker.length <= attestation.length; i++) {
    if (marker.every((byte, j) => attestation[i + j] === byte)) at = i + marker.length;
  }
  expect(attestation[at]).toBe(0x58); // byte string, one-byte length
  const authData = attestation.slice(at + 2, at + 2 + byteAt(attestation, at + 1));
  const idLength = (byteAt(authData, 53) << 8) | byteAt(authData, 54);
  const cose = authData.slice(55 + idLength);
  return { authData, x: cose.slice(10, 42), y: cose.slice(45, 77) };
}

/** An ASN.1 DER ECDSA signature as the raw r || s Web Crypto checks. */
function derToRaw(der: Uint8Array): Uint8Array<ArrayBuffer> {
  let at = 2;
  const part = () => {
    expect(der[at]).toBe(0x02);
    const length = byteAt(der, at + 1);
    let value = der.slice(at + 2, at + 2 + length);
    at += 2 + length;
    while (value.length > 32 && value[0] === 0) value = value.slice(1);
    const out = new Uint8Array(32);
    out.set(value, 32 - value.length);
    return out;
  };
  const r = part();
  const s = part();
  return new Uint8Array([...r, ...s]);
}

async function verifies(
  publicKey: CryptoKey,
  assertion: { authenticator_data: string; client_data_json: string; signature: string },
): Promise<boolean> {
  const authData = fromB64(assertion.authenticator_data);
  const clientHash = await sha256(fromB64(assertion.client_data_json));
  return crypto.subtle.verify(
    { name: "ECDSA", hash: "SHA-256" },
    publicKey,
    derToRaw(fromB64(assertion.signature)),
    new Uint8Array([...authData, ...clientHash]),
  );
}

describe("passkeys", () => {
  let world: World;
  let keyring: Keyring;
  let delivered: { tabId: number; token: string; outcome: PasskeyOutcome }[];
  let enabled: boolean;
  let locked: boolean;

  beforeAll(async () => {
    await testCore();
  });

  async function setup() {
    world = await newWorld();
    const device = await newDevice(world);
    await sync(device.store, device.keyring, new FakeServer(world.vault));
    keyring = device.keyring;
    delivered = [];
    enabled = true;
    locked = false;
    const context: PasskeyContext = {
      keyring: () => (locked ? null : keyring),
      signedIn: () => true,
      enabled: async () => enabled,
      excludedSites: async () => ["excluded.example"],
      create: (k, vaultId, json) => k.createPasskey(vaultId, uuidv7(), json, 100, uuidv7()),
      assert: (k, itemId, json) => k.assertPasskey(itemId, json),
      deliver: async (tabId, token, outcome) => {
        delivered.push({ tabId, token, outcome });
      },
      now: () => Date.now(),
    };
    return new Passkeys(context);
  }

  async function register(passkeys: Passkeys) {
    const start = await passkeys.start(TOP, createCall());
    if (start.native) throw new Error("expected Kandado to answer");
    const view = await passkeys.view(start.token);
    expect(view).toMatchObject({
      mode: "passkey-create",
      host: "example.com",
      username: "juan@example.com",
      already: false,
    });
    await passkeys.answer(start.token, { answer: "save", vault_id: world.vaultId });
    const outcome = delivered.at(-1)?.outcome;
    if (outcome?.kind !== "created") throw new Error(`expected created, got ${outcome?.kind}`);
    return outcome.response as {
      credential_id: string;
      attestation_object: string;
      client_data_json: string;
      public_key: string;
      public_key_algorithm: number;
    };
  }

  it("makes a passkey a relying party accepts, and signs in with it", async () => {
    const passkeys = await setup();
    const registration = await register(passkeys);
    expect(delivered.at(-1)?.tabId).toBe(TOP.tabId);

    // The registration, read as a relying party reads it.
    const client = JSON.parse(Buffer.from(fromB64(registration.client_data_json)).toString());
    expect(client).toEqual({
      type: "webauthn.create",
      challenge: b64(new Uint8Array(32).fill(1)),
      // The browser's origin for the frame, not anything the page said.
      origin: "https://www.example.com",
      crossOrigin: false,
    });
    const { authData, x, y } = coseXY(fromB64(registration.attestation_object));
    expect(authData.slice(0, 32)).toEqual(await sha256(new TextEncoder().encode("example.com")));
    expect(authData[32]).toBe(0x01 | 0x04 | 0x08 | 0x10 | 0x40);
    expect(registration.public_key_algorithm).toBe(-7);
    const fromSpki = await crypto.subtle.importKey(
      "spki",
      fromB64(registration.public_key),
      { name: "ECDSA", namedCurve: "P-256" },
      true,
      ["verify"],
    );
    const raw = new Uint8Array(await crypto.subtle.exportKey("raw", fromSpki));
    expect(raw).toEqual(new Uint8Array([4, ...x, ...y]));

    // E-26: the passkey is offered, and its assertion verifies.
    const start = await passkeys.start(TOP, getCall());
    if (start.native) throw new Error("expected Kandado to answer");
    const view = await passkeys.view(start.token);
    if (view.mode !== "passkey-get") throw new Error(view.mode);
    expect(view.passkeys).toHaveLength(1);
    const [offered] = view.passkeys;
    if (!offered) throw new Error("nothing offered");
    expect(offered).toMatchObject({ username: "juan@example.com" });
    expect(offered.vault?.name).toBe("Personal");
    await passkeys.answer(start.token, { answer: "use", item_id: offered.id });
    const outcome = delivered.at(-1)?.outcome;
    if (outcome?.kind !== "asserted") throw new Error(outcome?.kind);
    const assertion = outcome.response as {
      credential_id: string;
      authenticator_data: string;
      client_data_json: string;
      signature: string;
      user_handle: string;
    };
    expect(assertion.credential_id).toBe(registration.credential_id);
    expect(Buffer.from(fromB64(assertion.user_handle)).toString()).toBe("user-42");
    expect(await verifies(fromSpki, assertion)).toBe(true);
    const tampered = { ...assertion, client_data_json: b64(new TextEncoder().encode("{}")) };
    expect(await verifies(fromSpki, tampered)).toBe(false);

    // The stored item keeps its key inside the core.
    const item = (await keyring.items()).find((summary) => summary.kind === "passkey");
    expect(item?.passkey).toEqual({
      rp_id: "example.com",
      credential_id: registration.credential_id,
    });
    const content = await keyring.content(item?.id ?? "");
    expect(content.data.private_key).toBe("");
  });

  it("an allow list picks the passkey; a site that knows only others goes to the browser", async () => {
    const passkeys = await setup();
    const { credential_id } = await register(passkeys);
    const allowed = await passkeys.start(TOP, getCall({ allow_credentials: [credential_id] }));
    expect(allowed.native).toBe(false);
    expect(await passkeys.start(TOP, getCall({ allow_credentials: ["AAAA"] }))).toEqual({
      native: true,
    });
    // Another site has none of ours.
    const elsewhere = { ...TOP, url: "https://other.example.org/" };
    expect(await passkeys.start(elsewhere, getCall({ rp_id: undefined }))).toEqual({
      native: true,
    });
  });

  it("leaves to the browser what Kandado should not take", async () => {
    const passkeys = await setup();
    // A frame inside the page, an RSA-only site, an excluded site, a non-web page.
    expect(await passkeys.start({ ...TOP, frameId: 3 }, createCall())).toEqual({ native: true });
    expect(await passkeys.start(TOP, createCall({ algorithms: [-257] }))).toEqual({
      native: true,
    });
    expect(
      await passkeys.start({ ...TOP, url: "https://excluded.example/" }, createCall({ rp_id: "" })),
    ).toEqual({ native: true });
    expect(await passkeys.start({ ...TOP, url: "file:///tmp/a.html" }, createCall())).toEqual({
      native: true,
    });
    // Malformed requests from the page.
    expect(await passkeys.start(TOP, createCall({ challenge: 42 }))).toEqual({ native: true });
    expect(await passkeys.start(TOP, createCall({ user_id: "" }))).toEqual({ native: true });
    // Turned off in settings (E-29).
    enabled = false;
    expect(await passkeys.start(TOP, createCall())).toEqual({ native: true });
  });

  it("cancel, another device, a passkey the site already has, and abort", async () => {
    const passkeys = await setup();
    const tokenOf = async (call: PasskeyRequest) => {
      const start = await passkeys.start(TOP, call);
      if (start.native) throw new Error("expected Kandado to answer");
      return start.token;
    };
    let token = await tokenOf(createCall());
    await passkeys.answer(token, { answer: "cancel" });
    expect(delivered.at(-1)?.outcome).toMatchObject({ kind: "error", name: "NotAllowedError" });

    token = await tokenOf(createCall());
    await passkeys.answer(token, { answer: "native" });
    expect(delivered.at(-1)?.outcome).toEqual({ kind: "native" });

    const { credential_id } = await register(passkeys);
    token = await tokenOf(createCall({ exclude_credentials: [credential_id] }));
    expect(await passkeys.view(token)).toMatchObject({ mode: "passkey-create", already: true });
    await passkeys.answer(token, { answer: "cancel" });
    expect(delivered.at(-1)?.outcome).toMatchObject({ kind: "error", name: "InvalidStateError" });

    token = await tokenOf(getCall());
    // Only the top frame of the same tab may abort it.
    await passkeys.abort({ ...TOP, tabId: 99 }, token);
    expect(passkeys.tabOf(token)).toBe(TOP.tabId);
    await passkeys.abort(TOP, token);
    expect(delivered.at(-1)).toMatchObject({ token, outcome: { name: "AbortError" } });
    expect(passkeys.tabOf(token)).toBeNull();
    expect(await passkeys.view(token)).toEqual({ mode: "closed" });
  });

  it("refuses an item that is not a passkey for this site", async () => {
    const passkeys = await setup();
    await register(passkeys);
    const loginId = uuidv7();
    await keyring.createItem(world.vaultId, loginId, login("GitHub"), 100, uuidv7());
    const start = await passkeys.start(TOP, getCall());
    if (start.native) throw new Error("expected Kandado to answer");
    await passkeys.answer(start.token, { answer: "use", item_id: loginId });
    expect(delivered.at(-1)?.outcome).toMatchObject({ kind: "error", name: "NotAllowedError" });
  });

  it("a passkey made in one browser signs in from another after sync", async () => {
    world = await newWorld();
    const server = new FakeServer(world.vault);
    const a = await newDevice(world);
    const b = await newDevice(world);
    await sync(a.store, a.keyring, server);
    await sync(b.store, b.keyring, server);

    const request = JSON.stringify({ ...createCall().request, origin: "https://example.com" });
    const registration = JSON.parse(
      await a.keyring.createPasskey(world.vaultId, uuidv7(), request, 100, uuidv7()),
    );
    await sync(a.store, a.keyring, server);
    await sync(b.store, b.keyring, server);

    const onB = (await b.keyring.items()).find((item) => item.kind === "passkey");
    expect(onB?.passkey?.credential_id).toBe(registration.credential_id);
    const assertion = JSON.parse(
      await b.keyring.assertPasskey(
        onB?.id ?? "",
        JSON.stringify({ origin: "https://example.com", challenge: b64(new Uint8Array(32)) }),
      ),
    );
    const key = await crypto.subtle.importKey(
      "spki",
      fromB64(registration.public_key),
      { name: "ECDSA", namedCurve: "P-256" },
      false,
      ["verify"],
    );
    expect(await verifies(key, assertion)).toBe(true);
  });

  it("while locked it says so and saves nothing", async () => {
    const passkeys = await setup();
    locked = true;
    const start = await passkeys.start(TOP, createCall());
    if (start.native) throw new Error("expected Kandado to answer");
    expect(await passkeys.view(start.token)).toEqual({
      mode: "passkey-locked",
      host: "example.com",
      op: "create",
    });
    await passkeys.answer(start.token, { answer: "save" });
    expect(delivered.at(-1)?.outcome).toMatchObject({ kind: "error", name: "NotAllowedError" });
    locked = false;
    expect((await keyring.items()).filter((item) => item.kind === "passkey")).toHaveLength(0);
  });
});
