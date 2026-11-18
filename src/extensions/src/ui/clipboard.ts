// Copying secrets. What Kandado puts on the clipboard is taken off again after
// CLEAR_AFTER_MS, unless something else was copied in the meantime. The timer
// lives in the page that copied, so it only runs while that page stays open
// (the settings tab, a popup kept open); a popup that closes takes it with it.

export const CLEAR_AFTER_MS = 60_000;

let pending: { value: string; timer: number } | null = null;

async function clearIfStill(value: string) {
  try {
    // Reading needs a permission the extension does not ask for; when it is
    // refused, the value is cleared anyway, as the person was told it would be.
    const current = await navigator.clipboard.readText().catch(() => value);
    if (current === value) await navigator.clipboard.writeText("");
  } catch {
    // The page lost focus or the clipboard is blocked: nothing more to do.
  }
}

/** Copies a secret and schedules clearing it. Resolves false if the browser refused. */
export async function copySecret(value: string): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(value);
  } catch {
    return false;
  }
  if (pending) window.clearTimeout(pending.timer);
  const timer = window.setTimeout(() => {
    pending = null;
    void clearIfStill(value);
  }, CLEAR_AFTER_MS);
  pending = { value, timer };
  return true;
}
