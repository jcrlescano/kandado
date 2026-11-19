// The save and update prompts, E-23 and E-24. An extension page in an iframe at
// the top right of the site's page. It shows what the worker decided and sends
// back the answer; the captured password itself never comes to this page.

import { useEffect, useLayoutEffect, useRef, useState } from "react";
import type { PromptAnswer, PromptView } from "../../background/prompts";
import { Badge, Key, Select } from "../../ui/hardware";
import { ask } from "../../ui/messaging";

export function Prompt({ token }: { token: string }) {
  const [view, setView] = useState<PromptView | null>(null);
  const [vault, setVault] = useState("");
  const [busy, setBusy] = useState(false);
  const root = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    void ask({ type: "prompt-view", token }).then(
      (next) => {
        setView(next);
        if (next.mode === "save") setVault(next.vaults[0]?.id ?? "");
      },
      () => setView({ mode: "closed" }),
    );
  }, [token]);

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

  const answer = async (choice: PromptAnswer) => {
    setBusy(true);
    await ask({ type: "prompt-answer", token, answer: choice }).catch(() => {});
  };

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") void answer({ answer: "not-now" });
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  if (!view || view.mode === "closed") return <div ref={root} />;

  const title =
    view.mode === "save" ? `Save login for ${view.host}?` : `Update password for ${view.title}?`;
  const letter = (view.mode === "save" ? view.host : view.title).trim().charAt(0).toUpperCase();
  const onlyVault = view.mode === "save" && view.vaults.length === 1 ? view.vaults[0] : undefined;
  return (
    <div ref={root}>
      <div className="kx-prompt" role="dialog" aria-label={title}>
        <div className="kx-prompt-top">
          <Badge letter={letter || "K"} />
          <div className="kx-grow">
            <span className="kx-prompt-title">{title}</span>
            {view.username && <span className="hw-sub">{view.username}</span>}
            {onlyVault && <span className="hw-sub">{` · to ${onlyVault.name}`}</span>}
          </div>
          <Key size="sm" icon aria-label="Close" onClick={() => void answer({ answer: "not-now" })}>
            ×
          </Key>
        </div>
        {view.mode === "save" ? (
          view.vaults.length > 1 && (
            <div className="kx-prompt-vault">
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
        ) : (
          <p className="hw-sub kx-sub">
            {`The password you just used is different from the one saved in ${view.vault || "Kandado"}. The old one stays in history.`}
          </p>
        )}
        <div className="kx-prompt-keys">
          {view.mode === "save" && (
            <Key
              size="sm"
              tone="flat"
              disabled={busy}
              onClick={() => void answer({ answer: "never" })}
            >
              Never for this site
            </Key>
          )}
          <Key size="sm" disabled={busy} onClick={() => void answer({ answer: "not-now" })}>
            Not now
          </Key>
          <Key
            size="sm"
            tone="go"
            disabled={busy}
            onClick={() =>
              void answer(
                view.mode === "save" ? { answer: "save", vault_id: vault } : { answer: "update" },
              )
            }
          >
            {view.mode === "save" ? "Save" : "Update"}
          </Key>
        </div>
      </div>
    </div>
  );
}
