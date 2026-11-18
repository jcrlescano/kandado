// The save and update prompts, E-23 and E-24. An extension page in an iframe at
// the top right of the site's page. It shows what the worker decided and sends
// back the answer; the captured password itself never comes to this page.

import { useEffect, useLayoutEffect, useRef, useState } from "react";
import type { PromptAnswer, PromptView } from "../../background/prompts";
import { SelectField } from "../../popup/parts";
import { K } from "../../ui/kandado";
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
  return (
    <div ref={root}>
      <div className="kt-prompt" role="dialog" aria-label={title}>
        <div className="kt-prompt-top">
          <K.Logo size={22} />
          <div className="kt-srow-t">
            <b>{title}</b>
            {view.username && <span className="kt-srow-d">{view.username}</span>}
          </div>
          <K.IconButton
            icon="x"
            size="sm"
            label="Close"
            onClick={() => void answer({ answer: "not-now" })}
          />
        </div>
        {view.mode === "save" ? (
          <SelectField
            label="Vault"
            icon="vault"
            value={vault}
            options={view.vaults.map((v) => ({ value: v.id, label: v.name }))}
            onChange={setVault}
          />
        ) : (
          <p className="kt-p kt-small">
            {`The password you just used is different from the one saved in ${view.vault || "Kandado"}. The old one stays in history.`}
          </p>
        )}
        <div className="kt-prompt-acts">
          {view.mode === "save" && (
            <K.Button
              variant="ghost"
              size="sm"
              disabled={busy}
              onClick={() => void answer({ answer: "never" })}
            >
              Never for this site
            </K.Button>
          )}
          <K.Button
            variant="ghost"
            size="sm"
            disabled={busy}
            onClick={() => void answer({ answer: "not-now" })}
          >
            Not now
          </K.Button>
          <K.Button
            variant="primary"
            size="sm"
            disabled={busy}
            onClick={() =>
              void answer(
                view.mode === "save" ? { answer: "save", vault_id: vault } : { answer: "update" },
              )
            }
          >
            {view.mode === "save" ? "Save" : "Update"}
          </K.Button>
        </div>
      </div>
    </div>
  );
}
