// Layout parts the popup screens share, built from Kandado Hardware: the popup is
// 380 × 600 with a 10px inset, a header panel, an LCD strip, a list well and the
// keys at the bottom.

import { type CSSProperties, type FormEvent, type ReactNode, useEffect, useRef } from "react";
import { Key, NavButton, Panel, StatusLight } from "../ui/hardware";

export function Pop({ children, overlay }: { children: ReactNode; overlay?: ReactNode }) {
  return (
    <div className="kx-pop">
      {children}
      {overlay}
    </div>
  );
}

/** The header of an inner screen: a Back key, a title and optional keys. */
export function BackHead({
  title,
  onBack,
  children,
}: {
  title: ReactNode;
  onBack: () => void;
  children?: ReactNode;
}) {
  return (
    <Panel as="header" className="kx-phead">
      <div className="kx-phead-row">
        <Key size="sm" icon aria-label="Back" onClick={onBack}>
          ←
        </Key>
        {typeof title === "string" ? <h1 className="hw-h3 kx-phead-title">{title}</h1> : title}
        {children}
      </div>
    </Panel>
  );
}

/** The panel that holds a screen's fields; it scrolls when they do not fit. */
export function Body({
  children,
  as,
  onSubmit,
}: {
  children: ReactNode;
  as?: "div" | "form";
  onSubmit?: (event: FormEvent<HTMLElement>) => void;
}) {
  return (
    <Panel as={as ?? "div"} className="kx-body" onSubmit={onSubmit}>
      {children}
    </Panel>
  );
}

export function Foot({ children }: { children: ReactNode }) {
  return <footer className="kx-foot">{children}</footer>;
}

/** Nothing to list: a title, an optional line and an optional key. */
export function Empty({
  title,
  body,
  children,
}: {
  title: string;
  body?: string;
  children?: ReactNode;
}) {
  return (
    <div className="kx-empty">
      <p className="kx-empty-title">{title}</p>
      {body && <p className="hw-sub kx-sub">{body}</p>}
      {children}
    </div>
  );
}

export interface MenuEntry {
  key: string;
  label: string;
  /** A vault's status light before its name. */
  light?: boolean;
  danger?: boolean;
  checked?: boolean;
  onSelect?: () => void;
}

/** A menu over the popup (vaults, the account menu). Escape or a click outside closes it. */
export function Menu({
  entries,
  style,
  label,
  header,
  onClose,
}: {
  entries: (MenuEntry | "-")[];
  style: CSSProperties;
  label: string;
  /** Shown at the top, not a menu item (the account in the account menu). */
  header?: ReactNode;
  onClose: () => void;
}) {
  const first = useRef<HTMLButtonElement | null>(null);
  useEffect(() => {
    first.current?.focus();
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);
  let focused = false;
  return (
    <>
      <div className="kx-menu-scrim" onClick={onClose} aria-hidden="true" />
      <Panel className="kx-menu" style={style}>
        {header}
        <div
          role="menu"
          aria-label={label}
          style={{ display: "flex", flexDirection: "column", gap: 2 }}
        >
          {entries.map((entry, index) => {
            if (entry === "-") {
              const before = entries[index - 1];
              const after = typeof before === "object" ? before.key : "start";
              return <hr key={`after-${after}`} className="kx-menu-sep" />;
            }
            const isFirst = !focused;
            focused = true;
            return (
              <NavButton
                key={entry.key}
                {...(entry.checked === undefined
                  ? { role: "menuitem" }
                  : { role: "menuitemradio", "aria-checked": entry.checked })}
                on={entry.checked}
                className={entry.danger ? "kx-menu-danger" : undefined}
                ref={isFirst ? first : undefined}
                onClick={() => {
                  onClose();
                  entry.onSelect?.();
                }}
              >
                {entry.light && <StatusLight />}
                <span>{entry.label}</span>
              </NavButton>
            );
          })}
        </div>
      </Panel>
    </>
  );
}

/** The first letter of a title, for an item's badge. */
export function letterOf(title: string): string {
  return title.trim().charAt(0).toUpperCase() || "•";
}

/** The host of a page, without "www.". */
export function hostOf(url: string | undefined): string {
  if (!url) return "";
  try {
    const parsed = new URL(url);
    if (parsed.protocol !== "https:" && parsed.protocol !== "http:") return "";
    return parsed.hostname.replace(/^www\./, "");
  } catch {
    return "";
  }
}
