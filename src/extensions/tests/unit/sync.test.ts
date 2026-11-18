import { beforeAll, describe, expect, it } from "vitest";
import { Keyring, WrongPassword } from "../../src/background/keyring";
import { sync } from "../../src/background/sync";
import { uuidv7 } from "../../src/shared/bytes";
import { FakeServer, login, newDevice, newWorld, testCore, type World } from "./helpers";

let world: World;

beforeAll(async () => {
  await testCore();
});

async function setup() {
  world = await newWorld();
  const server = new FakeServer(world.vault);
  const a = await newDevice(world);
  const b = await newDevice(world);
  return { server, a, b };
}

const now = () => Math.floor(Date.now() / 1000);

describe("keys", () => {
  it("a wrong master password does not unlock", async () => {
    const w = await newWorld();
    const device = await newDevice(w);
    await expect(
      Keyring.unlock(await testCore(), device.store, w.account, "not the password"),
    ).rejects.toBeInstanceOf(WrongPassword);
  });
});

describe("sync between two devices", () => {
  it("an item made on one device opens on the other", async () => {
    const { server, a, b } = await setup();
    await sync(a.store, a.keyring, server); // brings the vault
    const id = uuidv7();
    await a.keyring.createItem(world.vaultId, id, login("GitHub", "s3cret"), now(), uuidv7());
    const report = await sync(a.store, a.keyring, server);
    expect(report.pushed).toBe(1);

    await sync(b.store, b.keyring, server);
    const items = await b.keyring.items();
    expect(items.map((item) => item.title)).toEqual(["GitHub"]);
    expect((await b.keyring.content(id)).data.password).toBe("s3cret");
    expect((await b.keyring.vaults()).map((vault) => vault.name)).toEqual(["Personal"]);
  });

  it("an edit travels and the item key stays", async () => {
    const { server, a, b } = await setup();
    await sync(a.store, a.keyring, server);
    const id = uuidv7();
    await a.keyring.createItem(world.vaultId, id, login("GitHub"), now(), uuidv7());
    await sync(a.store, a.keyring, server);
    await sync(b.store, b.keyring, server);

    await b.keyring.updateItem(id, login("GitHub", "new password"), now(), uuidv7());
    await sync(b.store, b.keyring, server);
    await sync(a.store, a.keyring, server);
    expect((await a.keyring.content(id)).data.password).toBe("new password");
  });

  it("edits made apart on both devices end the same, the later push winning", async () => {
    const { server, a, b } = await setup();
    await sync(a.store, a.keyring, server);
    const id = uuidv7();
    await a.keyring.createItem(world.vaultId, id, login("Start"), now(), uuidv7());
    await sync(a.store, a.keyring, server);
    await sync(b.store, b.keyring, server);

    await a.keyring.updateItem(id, login("From A"), now(), uuidv7());
    await b.keyring.updateItem(id, login("From B"), now(), uuidv7());
    await sync(a.store, a.keyring, server);
    // B meets A's version while pulling and merges it before it pushes.
    const report = await sync(b.store, b.keyring, server);
    expect(report.pulled).toBe(1);
    await sync(a.store, a.keyring, server);

    expect((await a.keyring.content(id)).title).toBe("From B");
    expect((await b.keyring.content(id)).title).toBe("From B");
  });

  it("an edit on one device and a trashing on the other both stand", async () => {
    const { server, a, b } = await setup();
    await sync(a.store, a.keyring, server);
    const id = uuidv7();
    await a.keyring.createItem(world.vaultId, id, login("Start"), now(), uuidv7());
    await sync(a.store, a.keyring, server);
    await sync(b.store, b.keyring, server);

    await a.keyring.updateItem(id, login("Edited"), now(), uuidv7());
    await b.store.atomically(async (tx) => {
      const row = await tx.item(id);
      if (row) await tx.putItem({ ...row, trashed_at: now(), dirty: true });
    });
    await sync(a.store, a.keyring, server);
    await sync(b.store, b.keyring, server);
    await sync(a.store, a.keyring, server);

    for (const device of [a, b]) {
      const [item] = await device.keyring.items();
      expect(item?.title).toBe("Edited");
      expect(item?.trashed).toBe(true);
    }
  });

  it("an item deleted for good elsewhere is deleted here, whatever was edited", async () => {
    const { server, a, b } = await setup();
    await sync(a.store, a.keyring, server);
    const id = uuidv7();
    await a.keyring.createItem(world.vaultId, id, login("Doomed"), now(), uuidv7());
    await sync(a.store, a.keyring, server);
    await sync(b.store, b.keyring, server);

    await b.keyring.updateItem(id, login("Edited too late"), now(), uuidv7());
    // Another device deletes it for good: the server holds a tombstone.
    const row = server.items.get(id);
    if (!row) throw new Error("not on the server");
    server.items.set(id, { ...row, deleted: true, key_wrapped: "", content_enc: "", revision: 99 });
    server.pulls = 0;
    await sync(b.store, b.keyring, server);
    expect(await b.keyring.items()).toEqual([]);
  });

  it("a long pull in pages gets everything", async () => {
    const { server, a, b } = await setup();
    await sync(a.store, a.keyring, server);
    for (let n = 0; n < 6; n += 1) {
      await a.keyring.createItem(world.vaultId, uuidv7(), login(`Item ${n}`), now(), uuidv7());
    }
    await sync(a.store, a.keyring, server);
    // A server that answers two rows at a time.
    const small = {
      pull: (cursor: number) => server.pull(cursor, 2),
      push: server.push.bind(server),
    };
    await sync(b.store, b.keyring, small);
    expect(await b.keyring.items()).toHaveLength(6);
  });

  it("random concurrent edits always converge", async () => {
    const { server, a, b } = await setup();
    await sync(a.store, a.keyring, server);
    await sync(b.store, b.keyring, server);
    const ids = [uuidv7(), uuidv7(), uuidv7()];
    for (const id of ids) {
      await a.keyring.createItem(world.vaultId, id, login(`Item ${id.slice(-4)}`), now(), uuidv7());
    }
    await sync(a.store, a.keyring, server);
    await sync(b.store, b.keyring, server);

    // A small seeded generator, so a failure can be replayed.
    let seed = 20261118;
    const random = (n: number) => {
      seed = (seed * 1664525 + 1013904223) % 2 ** 32;
      return seed % n;
    };
    for (let step = 0; step < 40; step += 1) {
      const device = random(2) === 0 ? a : b;
      const id = ids[random(ids.length)] as string;
      switch (random(4)) {
        case 0:
        case 1:
          await device.keyring.updateItem(id, login(`Edit ${step}`, `pw${step}`), now(), uuidv7());
          break;
        case 2:
          await device.store.atomically(async (tx) => {
            const row = await tx.item(id);
            if (row)
              await tx.putItem({ ...row, trashed_at: row.trashed_at ? null : now(), dirty: true });
          });
          break;
        default:
          await sync(device.store, device.keyring, server);
      }
    }
    for (let round = 0; round < 3; round += 1) {
      await sync(a.store, a.keyring, server);
      await sync(b.store, b.keyring, server);
    }

    const view = async (device: typeof a) =>
      Promise.all(
        (await device.keyring.items())
          .sort((x, y) => x.id.localeCompare(y.id))
          .map(async (item) => ({
            id: item.id,
            trashed: item.trashed,
            content: await device.keyring.content(item.id),
          })),
      );
    expect(await view(a)).toEqual(await view(b));
  });
});
