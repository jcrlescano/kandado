// E-27, the welcome tab. Opens once after install. Sign-in happens here, not in
// the popup; then the tab asks for site access, with what it is for.

import { useEffect, useState } from "react";
import type { ErrorCode, Status } from "../../shared/messages";
import { hasSiteAccess, requestSiteAccess } from "../../ui/access";
import { Key, Lcd, Logo, Panel, Raised, StatusLight, Tag, Well } from "../../ui/hardware";
import { AskError, ask } from "../../ui/messaging";

const SIGN_IN_ERRORS: Partial<Record<ErrorCode, string>> = {
  no_keys:
    "This account has no master password yet. Set up Kandado in the desktop app first, then sign in here.",
  sign_in_failed: "Google sign-in did not finish. Try again.",
  offline: "Can't reach Kandado. Check your connection and try again.",
};

type StepState = "done" | "now" | "later";

function Step({
  number,
  label,
  detail,
  state,
}: {
  number: number;
  label: string;
  detail?: string;
  state: StepState;
}) {
  return (
    <Well as="div" className={`kx-step${state === "now" ? " kx-step-now" : ""}`}>
      <StatusLight tone={state === "done" ? "ok" : state === "now" ? "warn" : "off"} />
      <span className="kx-step-text">
        <strong>{label}</strong>
        {detail && (
          <>
            <br />
            <span className="hw-sub">{detail}</span>
          </>
        )}
        <span className="kx-hidden-input">
          {state === "done" ? " (done)" : state === "now" ? " (now)" : ""}
        </span>
      </span>
      <Tag tone={state === "now" ? "go" : undefined}>{number}</Tag>
    </Well>
  );
}

/** Whether Kandado is pinned to the toolbar, where the browser can tell. */
async function pinned(): Promise<boolean> {
  try {
    const settings = await chrome.action.getUserSettings();
    return settings.isOnToolbar === true;
  } catch {
    return false;
  }
}

export function WelcomeApp() {
  const [status, setStatus] = useState<Status | null>(null);
  const [access, setAccess] = useState<boolean | null>(null);
  const [onToolbar, setOnToolbar] = useState(false);
  const [skipped, setSkipped] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<ErrorCode | null>(null);

  useEffect(() => {
    void ask({ type: "status" }).then(setStatus, () => setStatus({ state: "signed_out" }));
    void hasSiteAccess().then(setAccess);
    void pinned().then(setOnToolbar);
    const timer = window.setInterval(() => void pinned().then(setOnToolbar), 2000);
    return () => window.clearInterval(timer);
  }, []);

  const signIn = async () => {
    setBusy(true);
    setError(null);
    try {
      setStatus(await ask({ type: "sign-in" }));
    } catch (caught) {
      const code = caught instanceof AskError ? caught.code : "failed";
      if (code !== "sign_in_cancelled") setError(code);
    } finally {
      setBusy(false);
    }
  };

  const allow = async () => {
    setAccess(await requestSiteAccess());
  };

  const signedIn = status !== null && status.state !== "signed_out";
  let action: React.ReactNode = null;
  if (!status || access === null) {
    action = null;
  } else if (status.state === "signed_out") {
    action = (
      <>
        <h2 className="hw-h2">Sign in</h2>
        <p className="hw-sub">Use the Google account you use in the Kandado desktop app.</p>
        <Key tone="go" size="lg" block disabled={busy} onClick={() => void signIn()}>
          {busy ? "Signing in…" : "Continue with Google"}
        </Key>
        {error && (
          <Lcd className="kx-welcome-error" role="alert">
            <StatusLight tone="warn" />
            <span className="kx-welcome-error-text">
              {SIGN_IN_ERRORS[error] ?? "Something went wrong. Try again."}
            </span>
          </Lcd>
        )}
      </>
    );
  } else if (!access && !skipped) {
    action = (
      <>
        <h2 className="hw-h2">Let Kandado fill on websites</h2>
        <p className="hw-sub">
          To show your logins in the sign-in forms of the sites you visit, Kandado needs access to
          the pages you open. It looks only for forms, and fills only when you pick an item.
        </p>
        <Key tone="go" size="lg" block onClick={() => void allow()}>
          Allow on all sites
        </Key>
        <Key block onClick={() => setSkipped(true)}>
          Not now
        </Key>
      </>
    );
  } else {
    action = (
      <>
        <h2 className="hw-h2">You’re all set</h2>
        <p className="hw-sub">
          {`Signed in as ${status.email}. Open Kandado from the toolbar to unlock it with your master password.`}
        </p>
        {!access && (
          <p className="hw-sub">
            Kandado won’t fill on websites until you allow it in Settings › Autofill.
          </p>
        )}
      </>
    );
  }
  return (
    <div className="kx-welcome">
      <Panel as="main" className="kx-welcome-panel">
        <section className="kx-welcome-intro">
          <Logo size={30} wordmark />
          <div>
            <h1 className="hw-display">Kandado is in your browser.</h1>
            <p className="hw-sub">Pin it so it’s one click away, then sign in.</p>
          </div>
          <ol className="kx-steps">
            <li>
              <Step
                number={1}
                label="Click the puzzle icon in the toolbar"
                detail="Top right of your browser, next to the address bar."
                state={onToolbar ? "done" : "now"}
              />
            </li>
            <li>
              <Step
                number={2}
                label="Pin Kandado"
                detail="The green padlock stays in your toolbar."
                state={onToolbar ? "done" : "later"}
              />
            </li>
            <li>
              <Step
                number={3}
                label="Sign in"
                state={signedIn ? "done" : onToolbar ? "now" : "later"}
              />
            </li>
          </ol>
        </section>
        <Raised as="section" className="kx-welcome-action">
          {action}
        </Raised>
      </Panel>
    </div>
  );
}
