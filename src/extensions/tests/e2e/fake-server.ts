// A small stand-in for kandado-server, for driving the built extension in a
// browser: the Google step redirects straight back, and sync follows the real
// server's rules (FakeServer in tests/unit/helpers.ts). The account, its vault and
// its items are made with the WebAssembly core, the way the desktop app makes them.

import { existsSync, readFileSync } from "node:fs";
import { createServer, type Server } from "node:http";
import { resolve } from "node:path";
import type { ItemContentJson } from "../../src/background/keyring";
import { uuidv7 } from "../../src/shared/bytes";
import type { PushRequest, WireItem } from "../../src/shared/types";
import { FakeServer, login, newWorld, PASSWORD, testCore, type World } from "../unit/helpers";

export interface Running {
  url: string;
  world: World;
  sync: FakeServer;
  /** What `GET /v1/monitor/breaches` answers (E-15). Empty unless a test sets it. */
  breaches: import("../../src/background/breaches").BreachSite[];
  close(): Promise<void>;
}

export async function startFakeServer(port: number, items: ItemContentJson[]): Promise<Running> {
  const core = await testCore();
  const world = await newWorld();
  const sync = new FakeServer(world.vault);

  const session = core.Session.unlockWithPassword(
    world.account.user_id,
    world.account.keys_json,
    PASSWORD,
  );
  session.openVault(world.vaultId, Buffer.from(world.vault.key_sealed, "base64"));
  const now = Math.floor(Date.now() / 1000);
  const request: PushRequest = { vaults: [], items: [] };
  for (const content of items) {
    const id = uuidv7();
    const sealed = session.sealNewItem(world.vaultId, id, JSON.stringify(content));
    const item: WireItem = {
      id,
      vault_id: world.vaultId,
      kind: sealed.kind as WireItem["kind"],
      key_wrapped: Buffer.from(sealed.key_wrapped).toString("base64"),
      content_enc: Buffer.from(sealed.content_enc).toString("base64"),
      version_id: uuidv7(),
      created_at: now,
      updated_at: now,
      trashed_at: null,
      deleted: false,
      revision: 0,
    };
    request.items.push(item);
  }
  await sync.push(request);
  session.free();

  const tokens = () => ({
    access_token: `access-${uuidv7()}`,
    expires_in: 900,
    refresh_token: `refresh-${uuidv7()}`,
    user_id: world.account.user_id,
    device_id: world.account.device_id,
  });

  const running: Running = {
    url: `http://127.0.0.1:${port}`,
    world,
    sync,
    breaches: [],
    close: async () => {},
  };

  const server: Server = createServer(async (req, res) => {
    const url = new URL(req.url ?? "/", "http://localhost");
    const body = await new Promise<string>((done) => {
      let data = "";
      req.on("data", (chunk) => {
        data += chunk;
      });
      req.on("end", () => done(data));
    });
    const json = (value: unknown, status = 200) => {
      res.writeHead(status, { "content-type": "application/json" });
      res.end(JSON.stringify(value));
    };
    switch (`${req.method} ${url.pathname}`) {
      case "GET /v1/auth/google/start": {
        const back = new URL(url.searchParams.get("redirect") ?? "");
        back.searchParams.set("code", "one-time-code");
        res.writeHead(302, { location: back.toString() });
        res.end();
        return;
      }
      case "POST /v1/auth/token":
      case "POST /v1/auth/refresh":
        return json(tokens());
      case "POST /v1/auth/logout":
        res.writeHead(204);
        res.end();
        return;
      case "GET /v1/me":
        return json({
          id: world.account.user_id,
          email: world.account.email,
          display_name: world.account.display_name,
          plan: "free",
          has_keys: true,
          device_id: world.account.device_id,
        });
      case "GET /v1/account/keys":
        return json(JSON.parse(world.account.keys_json));
      case "GET /v1/monitor/breaches":
        return json({ breaches: running.breaches, fetched_at: new Date().toISOString() });
      case "GET /v1/sync":
        return json(
          await sync.pull(
            Number(url.searchParams.get("cursor")),
            Number(url.searchParams.get("limit")),
          ),
        );
      case "POST /v1/sync":
        return json(await sync.push(JSON.parse(body)));
      case "GET /site":
      case "GET /other":
        res.writeHead(200, { "content-type": "text/html" });
        res.end("<!doctype html><title>Test site</title><h1>Test site</h1>");
        return;
      default: {
        // Fixture pages: /login, /signup, /checkout, /address, /two-factor, /framed.
        const name = url.pathname.slice(1);
        const file = resolve(import.meta.dirname, "fixtures", `${name}.html`);
        if (req.method === "GET" && /^[a-z-]+$/.test(name) && existsSync(file)) {
          res.writeHead(200, { "content-type": "text/html" });
          res.end(readFileSync(file));
          return;
        }
        return json({ code: "not_found", title: "Not found", status: 404 }, 404);
      }
    }
  });
  await new Promise<void>((done) => server.listen(port, "127.0.0.1", done));
  running.close = () => new Promise((done) => server.close(() => done()));
  return running;
}

export const SAMPLE_ITEMS: ItemContentJson[] = [
  { ...login("GitHub", "Tq7#vL2m!Ox0-r9P", "https://github.com"), favourite: true },
  login("Shopee", "s3cret-shop", "https://shopee.ph"),
  // The site the tests open: the fake server itself.
  {
    ...login("Local test site", "local-pw", "http://localhost:8787"),
    data: {
      username: "juan",
      password: "local-pw",
      websites: [{ url: "http://localhost:8787", match_mode: "domain" }],
      totp: "JBSWY3DPEHPK3PXP",
    },
  },
  {
    title: "Juan Dela Cruz",
    notes: "",
    favourite: false,
    custom_fields: [],
    kind: "identity",
    data: {
      first_name: "Juan",
      last_name: "Dela Cruz",
      phone: "+63 917 000 0000",
      address: { line1: "12 Mabini St.", city: "Quezon City" },
    },
  },
  {
    title: "BPI Visa",
    notes: "",
    favourite: false,
    custom_fields: [],
    kind: "card",
    data: {
      cardholder: "JUAN DELA CRUZ",
      number: "4821009433104821",
      expiry: { month: 8, year: 2029 },
      cvv: "214",
      pin: "",
    },
  },
  {
    title: "Wi-Fi at home",
    notes: "Network: Bahay-5G",
    favourite: false,
    custom_fields: [],
    kind: "note",
    data: {},
  },
];
