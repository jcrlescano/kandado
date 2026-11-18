// Parts the full-page tabs share: the settings layout (E-28 to E-31, the same
// navigation pattern as desktop settings) and its rows, and the PIN setup that
// E-31 and the popup both use.

import { type ReactNode, useEffect, useRef, useState } from "react";
import { PinDots } from "../popup/parts";
import { K } from "../ui/kandado";
import type { IconName } from "../ui/vendor/kandado";

export type SettingsPage = "general" | "autofill" | "excluded" | "security";

export const PAGES: { id: SettingsPage; icon: IconName; label: string }[] = [
  { id: "general", icon: "settings", label: "General" },
  { id: "autofill", icon: "key", label: "Autofill" },
  { id: "excluded", icon: "x", label: "Excluded sites" },
  { id: "security", icon: "lock", label: "Security" },
];

export function OptPage({
  active,
  onNavigate,
  children,
}: {
  active: SettingsPage;
  onNavigate: (page: SettingsPage) => void;
  children: ReactNode;
}) {
  const title = PAGES.find((page) => page.id === active)?.label ?? "";
  return (
    <div className="kt-opt kx-fullpage">
      <nav className="kt-opt-nav" aria-label="Settings">
        <K.Logo size={26} wordmark />
        {PAGES.map((page) => (
          <button
            key={page.id}
            type="button"
            className={`kt-nav kx-nav-button${page.id === active ? " kt-nav-on" : ""}`}
            aria-current={page.id === active ? "page" : undefined}
            onClick={() => onNavigate(page.id)}
          >
            <K.Icon name={page.icon} size={20} />
            <span className="kt-nav-l">{page.label}</span>
          </button>
        ))}
      </nav>
      <main className="kt-opt-main">
        <h1>{title}</h1>
        {children}
      </main>
    </div>
  );
}

/** One settings row (the design's `SRow`), inside a `Rows` group. */
export function Row({
  icon,
  label,
  description,
  value,
  toggle,
  disabled,
  onToggle,
  onOpen,
  action,
}: {
  icon: IconName;
  label: string;
  description?: string;
  value?: string;
  toggle?: boolean;
  disabled?: boolean;
  onToggle?: (on: boolean) => void;
  /** Makes the whole row a button with a chevron. */
  onOpen?: () => void;
  action?: ReactNode;
}) {
  const body = (
    <>
      <K.Icon name={icon} size={20} className="kt-srow-ic" />
      <div className="kt-srow-t">
        <div className="kt-srow-l">{label}</div>
        {description && <div className="kt-srow-d">{description}</div>}
      </div>
      {value && <span className="kt-srow-v">{value}</span>}
      {toggle !== undefined && (
        <K.Toggle ariaLabel={label} checked={toggle} disabled={disabled} onChange={onToggle} />
      )}
      {action}
      {onOpen && <K.Icon name="chevron" size={16} className="kt-srow-ch" />}
    </>
  );
  if (onOpen) {
    return (
      <button type="button" className="kt-srow kx-row-button" onClick={onOpen} disabled={disabled}>
        {body}
      </button>
    );
  }
  return <div className="kt-srow">{body}</div>;
}

export function Rows({ children }: { children: ReactNode }) {
  return <div className="kd-group">{children}</div>;
}

/**
 * Choosing a 6-digit PIN: type it, then type it again. Used by E-31 and by the
 * popup after a master-password unlock when the person asked for a PIN.
 */
export function PinSetup({
  onDone,
  onCancel,
  cancelLabel = "Cancel",
}: {
  onDone: (pin: string) => Promise<void>;
  onCancel: () => void;
  cancelLabel?: string;
}) {
  const [first, setFirst] = useState<string | null>(null);
  const [pin, setPin] = useState("");
  const [error, setError] = useState<string | null>(null);
  const input = useRef<HTMLInputElement | null>(null);

  // biome-ignore lint/correctness/useExhaustiveDependencies: focus again for the second entry
  useEffect(() => {
    input.current?.focus();
  }, [first]);

  const typed = async (value: string) => {
    setPin(value);
    setError(null);
    if (value.length < 6) return;
    if (first === null) {
      setFirst(value);
      setPin("");
      return;
    }
    if (value !== first) {
      setFirst(null);
      setPin("");
      setError("The PINs did not match. Choose one again.");
      return;
    }
    await onDone(value);
  };

  return (
    <div className="kx-pin-setup">
      <h2 className="kt-display" style={{ fontSize: 24, lineHeight: "30px" }}>
        {first === null ? "Choose a PIN" : "Enter it again"}
      </h2>
      <p className="kt-p">
        Six digits unlock Kandado until the browser closes. After that, your master password is
        needed again.
      </p>
      <label style={{ cursor: "text", display: "flex", flexDirection: "column", gap: 12 }}>
        <span className="kx-hidden-input">PIN</span>
        <PinDots filled={pin.length} error={error !== null} />
        <input
          ref={input}
          className="kx-hidden-input"
          inputMode="numeric"
          autoComplete="off"
          aria-label={first === null ? "New PIN" : "New PIN again"}
          maxLength={6}
          value={pin}
          onChange={(event) => void typed(event.target.value.replace(/\D/g, "").slice(0, 6))}
        />
      </label>
      {error && (
        <p className="kx-error" role="alert">
          {error}
        </p>
      )}
      <K.Button variant="ghost" onClick={onCancel}>
        {cancelLabel}
      </K.Button>
    </div>
  );
}
