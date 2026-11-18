// E-27, the welcome tab. Opens once after install. Sign-in happens here, not in
// the popup; then the tab asks for site access, with what it is for.

import { useEffect, useState } from "react";
import type { ErrorCode, Status } from "../../shared/messages";
import { hasSiteAccess, requestSiteAccess } from "../../ui/access";
import { K } from "../../ui/kandado";
import { AskError, ask } from "../../ui/messaging";
import { Rows } from "../parts";

const SIGN_IN_ERRORS: Partial<Record<ErrorCode, string>> = {
  no_keys:
    "This account has no master password yet. Set up Kandado in the desktop app first, then sign in here.",
  sign_in_failed: "Google sign-in did not finish. Try again.",
  offline: "Can't reach Kandado. Check your connection and try again.",
};

function Step({ number, label }: { number: number; label: string }) {
  return (
    <div className="kt-srow">
      <b className="kt-id">{number}</b>
      <div className="kt-srow-t">
        <div className="kt-srow-l">{label}</div>
      </div>
    </div>
  );
}

export function WelcomeApp() {
  const [status, setStatus] = useState<Status | null>(null);
  const [access, setAccess] = useState<boolean | null>(null);
  const [skipped, setSkipped] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<ErrorCode | null>(null);

  useEffect(() => {
    void ask({ type: "status" }).then(setStatus, () => setStatus({ state: "signed_out" }));
    void hasSiteAccess().then(setAccess);
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

  let body: React.ReactNode;
  if (!status || access === null) {
    body = null;
  } else if (status.state === "signed_out") {
    body = (
      <>
        <K.Logo size={56} />
        <h1 className="kt-display" style={{ fontSize: 30, lineHeight: "36px" }}>
          Kandado is installed
        </h1>
        <p className="kt-p">Pin it so it’s one click away, then sign in.</p>
        <Rows>
          <Step number={1} label="Click the puzzle icon in the toolbar" />
          <Step number={2} label="Pin Kandado" />
          <Step number={3} label="Sign in below" />
        </Rows>
        {error && (
          <p className="kx-error" role="alert">
            {SIGN_IN_ERRORS[error] ?? "Something went wrong. Try again."}
          </p>
        )}
        <div className="kx-actions">
          <K.Button variant="primary" size="lg" disabled={busy} onClick={() => void signIn()}>
            {busy ? "Signing in…" : "Continue with Google"}
          </K.Button>
        </div>
      </>
    );
  } else if (!access && !skipped) {
    body = (
      <>
        <K.Logo size={56} />
        <h1 className="kt-display" style={{ fontSize: 30, lineHeight: "36px" }}>
          Let Kandado fill on websites
        </h1>
        <p className="kt-p">
          To show your logins in the sign-in forms of the sites you visit, Kandado needs access to
          the pages you open. It looks only for forms, and fills only when you pick an item.
        </p>
        <div className="kx-actions">
          <K.Button variant="primary" size="lg" onClick={() => void allow()}>
            Allow on all sites
          </K.Button>
          <K.Button variant="ghost" size="lg" onClick={() => setSkipped(true)}>
            Not now
          </K.Button>
        </div>
      </>
    );
  } else {
    body = (
      <>
        <K.Logo size={56} />
        <h1 className="kt-display" style={{ fontSize: 30, lineHeight: "36px" }}>
          You’re all set
        </h1>
        <p className="kt-p">
          {`Signed in as ${status.email}. Open Kandado from the toolbar to unlock it with your master password.`}
        </p>
        {!access && (
          <p className="kt-p">
            Kandado won’t fill on websites until you allow it in Settings › Autofill.
          </p>
        )}
      </>
    );
  }
  return (
    <div className="kx-welcome kx-fullpage">
      <main className="kt-opt-main">{body}</main>
    </div>
  );
}
