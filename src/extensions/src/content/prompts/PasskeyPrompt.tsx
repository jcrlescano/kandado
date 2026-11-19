// The passkey prompts, E-25 (save a passkey) and E-26 (sign in with one), in
// the same top-right iframe as the save and update prompts. They show what the
// worker decided and send back the answer; no key ever comes to this page.
//
// "Use another device" hands the request to the browser's own WebAuthn, with
// the page's original options. Cancel answers the page with NotAllowedError
// (or InvalidStateError when the site already has this passkey).

import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import type { PasskeyAnswer, PasskeyView } from "../../background/passkeys";
import { ItemRow, Key, LcdStrip, Logo, Select, WellRow } from "../../ui/hardware";
import { ask } from "../../ui/messaging";

const plural = (count: number, one: string, many: string) => `${count} ${count === 1 ? one : many}`;

export function PasskeyPrompt({ token }: { token: string }) {
  const [view, setView] = useState<PasskeyView | null>(null);
  const [vault, setVault] = useState("");
  const [chosen, setChosen] = useState("");
  const [busy, setBusy] = useState(false);
  const root = useRef<HTMLDivElement | null>(null);

  const load = useCallback(() => {
    void ask({ type: "passkey-view", token }).then(
      (next) => {
        setView(next);
        if (next.mode === "passkey-create") setVault(next.vaults[0]?.id ?? "");
        if (next.mode === "passkey-get") setChosen(next.passkeys[0]?.id ?? "");
      },
      () => setView({ mode: "closed" }),
    );
  }, [token]);

  useEffect(load, [load]);

  useLayoutEffect(() => {
    const element = root.current;
    if (!element) return;
    const report = () =>
      void ask({
        type: "prompt-size",
        token,
        height: document.body.getBoundingClientRect().height,
      }).catch(() => {});
    report();
    const observer = new ResizeObserver(report);
    observer.observe(element);
    return () => observer.disconnect();
  }, [token]);

  const answer = async (choice: PasskeyAnswer) => {
    setBusy(true);
    await ask({ type: "passkey-answer", token, answer: choice }).catch(() => {});
  };

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") void answer({ answer: "cancel" });
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  if (!view || view.mode === "closed") return <div ref={root} />;

  const title =
    view.mode === "passkey-create"
      ? `Save a passkey for ${view.host}?`
      : view.mode === "passkey-get"
        ? "Sign in with a passkey"
        : view.op === "create"
          ? `Save a passkey for ${view.host}?`
          : "Sign in with a passkey";

  const keys = (primary?: { label: string; disabled?: boolean; onClick: () => void }) => (
    <div className="kx-prompt-keys kx-passkey-keys">
      <Key size="sm" tone="flat" disabled={busy} onClick={() => void answer({ answer: "native" })}>
        Use another device
      </Key>
      <span className="kx-grow" />
      <Key size="sm" disabled={busy} onClick={() => void answer({ answer: "cancel" })}>
        Cancel
      </Key>
      {primary && (
        <Key size="sm" tone="go" disabled={busy || primary.disabled} onClick={primary.onClick}>
          {primary.label}
        </Key>
      )}
    </div>
  );

  return (
    <div ref={root}>
      <div className="kx-prompt" role="dialog" aria-label={title}>
        <div className="kx-prompt-top kx-passkey-top">
          <Logo size={24} />
          <span className="kx-prompt-title kx-grow">{title}</span>
          <Key
            size="sm"
            icon
            aria-label="Close"
            disabled={busy}
            onClick={() => void answer({ answer: "cancel" })}
          >
            ×
          </Key>
        </div>

        {view.mode === "passkey-locked" && (
          <>
            <LcdStrip light="warn" text={`Kandado is locked · ${view.host}`} />
            <p className="hw-sub kx-sub">
              Unlock Kandado from the toolbar, then choose Try again. Or use a passkey on another
              device.
            </p>
            {keys({ label: "Try again", onClick: load })}
          </>
        )}

        {view.mode === "passkey-create" && (
          <>
            <LcdStrip light={view.already ? "warn" : "ok"} text={`Passkey · ${view.host}`} />
            {view.username && (
              <div className="kx-prompt-field">
                <span className="hw-label">Username</span>
                <WellRow>
                  <span className="kx-dd-mono kx-ellipsis">{view.username}</span>
                </WellRow>
              </div>
            )}
            {view.already ? (
              <p className="hw-sub kx-sub">
                {`Kandado already has a passkey for this account on ${view.host}.`}
              </p>
            ) : (
              view.vaults.length > 1 && (
                <div className="kx-prompt-field">
                  <span className="hw-label" aria-hidden="true">
                    Vault
                  </span>
                  <Select
                    aria-label="Vault"
                    value={vault}
                    options={view.vaults.map((v) => ({ value: v.id, label: v.name }))}
                    onChange={(event) => setVault(event.target.value)}
                  />
                </div>
              )
            )}
            {keys(
              view.already
                ? undefined
                : {
                    label: "Save passkey",
                    disabled: view.vaults.length === 0,
                    onClick: () => void answer({ answer: "save", vault_id: vault }),
                  },
            )}
          </>
        )}

        {view.mode === "passkey-get" && (
          <>
            <LcdStrip
              light="ok"
              text={`${view.host} · ${plural(view.passkeys.length, "passkey", "passkeys")}`}
            />
            <div role="listbox" aria-label="Passkeys" className="hw-well kx-dropdown-well">
              {view.passkeys.map((passkey) => (
                <ItemRow
                  key={passkey.id}
                  title={passkey.username || view.host}
                  subtitle={passkey.vault?.name ?? undefined}
                  selected={passkey.id === chosen}
                  onClick={() => setChosen(passkey.id)}
                  onDoubleClick={() => void answer({ answer: "use", item_id: passkey.id })}
                />
              ))}
            </div>
            {keys({
              label: "Sign in",
              disabled: !chosen,
              onClick: () => void answer({ answer: "use", item_id: chosen }),
            })}
          </>
        )}
      </div>
    </div>
  );
}
