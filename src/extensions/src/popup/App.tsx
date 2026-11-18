// The popup (380 × 600). Which screen shows follows the worker's state:
// signed out (E-01), locked (E-02), or unlocked (E-03 and everything from it).

import { useCallback, useEffect, useRef, useState } from "react";
import type { ItemContentJson, Summary } from "../background/keyring";
import { PinSetup } from "../pages/parts";
import type { Status } from "../shared/messages";
import { K } from "../ui/kandado";
import { ask } from "../ui/messaging";
import { Detail, Generator, Home, Locked, LoginForm, SignedOut } from "./screens";

type Screen =
  | { name: "home" }
  | { name: "detail"; item: Summary }
  | { name: "create-login" }
  | { name: "edit-login"; item: Summary; content: ItemContentJson }
  | { name: "generator" }
  | { name: "set-pin" };

interface Toast {
  text: string;
  tone: "neutral" | "danger";
}

const VAULT_FILTER_KEY = "popupVault";

/**
 * The page the popup is about: the active tab of the window it opened over. The
 * popup page can also be opened in a tab of its own with `?tab=<id>` naming the
 * tab (the end-to-end tests do this; a browser cannot script the real popup).
 */
async function activeTab(): Promise<{ id?: number; url?: string }> {
  try {
    const named = Number(new URLSearchParams(location.search).get("tab"));
    if (named) return await chrome.tabs.get(named);
    const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
    return tab ?? {};
  } catch {
    return {};
  }
}

export function App() {
  const [status, setStatus] = useState<Status | null>(null);
  const [screen, setScreen] = useState<Screen>({ name: "home" });
  const [tabUrl, setTabUrl] = useState<string | undefined>();
  const [tabId, setTabId] = useState<number | undefined>();
  const [vaultFilter, setVaultFilter] = useState<string | null>(null);
  const [toast, setToast] = useState<Toast | null>(null);
  const toastTimer = useRef<number | undefined>(undefined);

  useEffect(() => {
    void ask({ type: "status" }).then(setStatus, () => setStatus({ state: "signed_out" }));
    void activeTab().then((tab) => {
      setTabUrl(tab.url);
      setTabId(tab.id);
    });
    void chrome.storage.local.get(VAULT_FILTER_KEY).then((stored) => {
      const value = stored[VAULT_FILTER_KEY];
      setVaultFilter(typeof value === "string" ? value : null);
    });
  }, []);

  const showToast = useCallback((text: string, tone: "neutral" | "danger" = "neutral") => {
    window.clearTimeout(toastTimer.current);
    setToast({ text, tone });
    toastTimer.current = window.setTimeout(() => setToast(null), 2400);
  }, []);

  const onStatus = useCallback((next: Status) => {
    setStatus(next);
    setScreen({ name: "home" });
    // Unlocked with the master password, and a PIN was asked for in E-31: choose
    // this browser session's PIN now.
    if (next.state === "unlocked" && !next.pin) {
      void ask({ type: "settings-get" })
        .then((settings) => {
          if (settings.pinWanted) setScreen({ name: "set-pin" });
        })
        .catch(() => {});
    }
  }, []);

  const chooseVault = (vault: string | null) => {
    setVaultFilter(vault);
    void (vault === null
      ? chrome.storage.local.remove(VAULT_FILTER_KEY)
      : chrome.storage.local.set({ [VAULT_FILTER_KEY]: vault }));
  };

  if (!status) return <div className="kt-pop" aria-busy="true" />;
  if (status.state === "signed_out") return <SignedOut onStatus={onStatus} />;
  if (status.state === "locked") return <Locked status={status} onStatus={onStatus} />;

  const home = () => setScreen({ name: "home" });

  /** E-03 and E-06/E-07: fill the item into the page and close; otherwise show it. */
  const canFill = tabId !== undefined && /^https?:/.test(tabUrl ?? "");
  const fillTab = async (item: Summary) => {
    if (tabId === undefined) return;
    const result = await ask({ type: "fill-tab", tabId, itemId: item.id }).catch(
      () => "refused" as const,
    );
    if (result === "filled") window.close();
    else if (screen.name === "detail") showToast("There is no form for it on this page", "danger");
    else setScreen({ name: "detail", item });
  };
  let view: React.ReactNode;
  switch (screen.name) {
    case "home":
      view = (
        <Home
          email={status.email}
          tabUrl={tabUrl}
          vaultFilter={vaultFilter}
          onVaultFilter={chooseVault}
          onOpen={(item) => setScreen({ name: "detail", item })}
          onFill={(item) => void fillTab(item)}
          onCreateLogin={() => setScreen({ name: "create-login" })}
          onGenerate={() => setScreen({ name: "generator" })}
          onLock={() => void ask({ type: "lock" }).then(onStatus)}
          onSignOut={() => void ask({ type: "sign-out" }).then(onStatus)}
          onToast={showToast}
        />
      );
      break;
    case "detail":
      view = (
        <Detail
          item={screen.item}
          onBack={home}
          onEdit={(content) => setScreen({ name: "edit-login", item: screen.item, content })}
          onFill={canFill ? () => void fillTab(screen.item) : undefined}
        />
      );
      break;
    case "create-login":
      view = (
        <LoginForm
          tabUrl={tabUrl}
          onCancel={home}
          onDone={(message) => {
            showToast(message);
            home();
          }}
        />
      );
      break;
    case "edit-login":
      view = (
        <LoginForm
          tabUrl={tabUrl}
          editing={{ item: screen.item, content: screen.content }}
          onCancel={() => setScreen({ name: "detail", item: screen.item })}
          onDone={(message) => {
            showToast(message);
            setScreen({ name: "detail", item: screen.item });
          }}
        />
      );
      break;
    case "generator":
      view = <Generator onBack={home} onToast={showToast} />;
      break;
    case "set-pin":
      view = (
        <div className="kt-pop">
          <div className="kt-pbody" style={{ justifyContent: "center" }}>
            <PinSetup
              cancelLabel="Not now"
              onCancel={home}
              onDone={async (pin) => {
                setStatus(await ask({ type: "set-pin", pin }));
                showToast("PIN set until the browser closes");
                home();
              }}
            />
          </div>
        </div>
      );
      break;
  }
  return (
    <>
      {view}
      {toast && (
        <div className="kx-toast">
          <K.Toast tone={toast.tone}>{toast.text}</K.Toast>
        </div>
      )}
    </>
  );
}
