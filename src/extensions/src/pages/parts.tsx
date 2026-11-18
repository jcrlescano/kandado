// Parts the full-page tabs share: the settings layout (E-28 to E-31: a nav panel
// beside the page panel, as the Hardware settings board draws it) and its rows,
// and the PIN setup that E-31 and the popup both use.

import { type ReactNode, useEffect, useRef, useState } from "react";
import { Key, Logo, NavButton, Panel, PinDisplay, Well } from "../ui/hardware";

export type SettingsPage = "general" | "autofill" | "excluded" | "security";

export const PAGES: { id: SettingsPage; label: string }[] = [
  { id: "general", label: "General" },
  { id: "autofill", label: "Autofill" },
  { id: "excluded", label: "Excluded sites" },
  { id: "security", label: "Security" },
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
    <div className="kx-settings">
      <Panel as="nav" className="kx-settings-nav" aria-label="Settings">
        <Logo size={28} wordmark="EXTENSION" />
        {PAGES.map((page) => (
          <NavButton
            key={page.id}
            on={page.id === active}
            aria-current={page.id === active ? "page" : undefined}
            onClick={() => onNavigate(page.id)}
          >
            {page.label}
          </NavButton>
        ))}
      </Panel>
      <Panel as="main" className="kx-settings-main">
        <h1 className="hw-h2">{title}</h1>
        {children}
      </Panel>
    </div>
  );
}

/** One settings row inside a `Rows` well: its name, a line of consequence, its control. */
export function Row({
  label,
  description,
  strong,
  children,
}: {
  label: string;
  description?: ReactNode;
  /** The row is a heading for its control (Theme). */
  strong?: boolean;
  children?: ReactNode;
}) {
  return (
    <div className="kx-row">
      <span className="kx-row-text">
        <span className={`kx-row-label${strong ? " kx-row-strong" : ""}`}>{label}</span>
        {description && <span className="hw-sub">{description}</span>}
      </span>
      {children}
    </div>
  );
}

export function Rows({ children }: { children: ReactNode }) {
  return <Well className="kx-rows">{children}</Well>;
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
      <h2 className="hw-h2">{first === null ? "Choose a PIN" : "Enter it again"}</h2>
      <p className="hw-sub">
        Six digits unlock Kandado until the browser closes. After that, your master password is
        needed again.
      </p>
      <label className="kx-pin-label">
        <span className="kx-hidden-input">PIN</span>
        <PinDisplay filled={pin.length} error={error !== null} />
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
        <p className="hw-error kx-sub" role="alert">
          {error}
        </p>
      )}
      <Key onClick={onCancel}>{cancelLabel}</Key>
    </div>
  );
}
