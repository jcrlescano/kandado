// Loads the shared Rust core (`kandado-wasm`). Everything that has to match the
// desktop app byte for byte (crypto, item encoding, generator, TOTP, strength,
// URL matching) runs in there; this file only starts it.
//
// The import is static on purpose: service workers may not use `import()`.

import * as wasm from "../../wasm/pkg/kandado_wasm.js";

export type Core = typeof wasm;

let loaded: Promise<Core> | null = null;

/** The core, started once per worker. Tests hand in their own with {@link provideCore}. */
export function loadCore(): Promise<Core> {
  loaded ??= (async () => {
    await wasm.default({
      module_or_path: new URL("../../wasm/pkg/kandado_wasm_bg.wasm", import.meta.url),
    });
    return wasm;
  })();
  return loaded;
}

export function provideCore(core: Core): void {
  loaded = Promise.resolve(core);
}
