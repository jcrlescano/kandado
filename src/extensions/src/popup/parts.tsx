// Small layout parts the popup screens share. They reproduce the popup layout
// the design system draws (its `kt-*` classes) with real data and real events.

import { type ReactNode, useEffect, useRef } from "react";
import { K } from "../ui/kandado";
import type { IconName, ItemKind } from "../ui/vendor/kandado";

export function Pop({ children, overlay }: { children: ReactNode; overlay?: ReactNode }) {
  return (
    <div className="kt-pop">
      {children}
      {overlay}
    </div>
  );
}

export function Head({ children }: { children: ReactNode }) {
  return <div className="kt-phead">{children}</div>;
}

/** The header of an inner screen: Back, a title and optional actions. */
export function BackHead({
  title,
  onBack,
  right,
}: {
  title: string;
  onBack: () => void;
  right?: ReactNode;
}) {
  return (
    <Head>
      <K.IconButton icon="chevron" label="Back" size="sm" className="kt-flip" onClick={onBack} />
      <div className="kt-top-title kt-small-title">{title}</div>
      {right}
    </Head>
  );
}

export function Body({ children, center }: { children: ReactNode; center?: boolean }) {
  return (
    <div
      className="kt-pbody"
      style={
        center
          ? { alignItems: "center", justifyContent: "center", textAlign: "center", gap: 14 }
          : undefined
      }
    >
      {children}
    </div>
  );
}

export function Foot({ children }: { children: ReactNode }) {
  return <div className="kt-pfoot">{children}</div>;
}

/** The column footer of the signed-out and locked screens. */
export function ColumnFoot({ children }: { children: ReactNode }) {
  return (
    <div className="kt-mfoot kt-col" style={{ padding: 12 }}>
      {children}
    </div>
  );
}

export function Section({ icon, children }: { icon?: IconName; children: ReactNode }) {
  return (
    <div className="kt-psec">
      {icon && <K.Icon name={icon} size={14} />}
      {children}
    </div>
  );
}

export function List({ children, label }: { children: ReactNode; label: string }) {
  return (
    <div className="kt-plist" role="listbox" aria-label={label}>
      {children}
    </div>
  );
}

export function Group({ children }: { children: ReactNode }) {
  return <div className="kd-group">{children}</div>;
}

export function Empty({
  icon,
  title,
  body,
  children,
}: {
  icon: IconName;
  title: string;
  body?: string;
  children?: ReactNode;
}) {
  return (
    <div className="kt-empty">
      <div className="kt-empty-ic kt-tone-teal">
        <K.Icon name={icon} size={28} />
      </div>
      <h3>{title}</h3>
      {body && <p>{body}</p>}
      {children}
    </div>
  );
}

export interface MenuEntry {
  key: string;
  label: string;
  icon?: IconName;
  kind?: ItemKind;
  hint?: string;
  danger?: boolean;
  checked?: boolean;
  /** Shown but not a button (the account line in E-13). */
  inert?: boolean;
  onSelect?: () => void;
}

/** A menu over the popup (E-09, E-13, E-14). Escape or a click outside closes it. */
export function Menu({
  entries,
  style,
  label,
  onClose,
}: {
  entries: (MenuEntry | "-")[];
  style: React.CSSProperties;
  label: string;
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
      <div className="kx-scrim" onClick={onClose} aria-hidden="true" />
      <div
        className="kt-menu"
        role="menu"
        aria-label={label}
        style={{ position: "absolute", zIndex: 6, ...style }}
      >
        {entries.map((entry, index) => {
          if (entry === "-") {
            const before = entries[index - 1];
            const after = typeof before === "object" ? before.key : "start";
            return <div key={`after-${after}`} className="kt-menu-sep" />;
          }
          const content = (
            <>
              {entry.kind ? (
                <K.ItemIcon kind={entry.kind} size={28} />
              ) : (
                entry.icon && <K.Icon name={entry.icon} size={18} />
              )}
              <span>{entry.label}</span>
              {entry.hint && <kbd className="kd-kbd">{entry.hint}</kbd>}
              {entry.checked && <K.Icon name="check" size={16} className="kt-menu-check" />}
            </>
          );
          const classes = [
            "kt-menu-item",
            entry.danger && "kt-danger",
            entry.checked && "kt-menu-on",
          ]
            .filter(Boolean)
            .join(" ");
          if (entry.inert) {
            return (
              <div key={entry.key} className={classes}>
                {content}
              </div>
            );
          }
          const isFirst = !focused;
          focused = true;
          return (
            <button
              key={entry.key}
              type="button"
              {...(entry.checked === undefined
                ? { role: "menuitem" }
                : { role: "menuitemradio", "aria-checked": entry.checked })}
              className={`${classes} kx-menu-button`}
              ref={isFirst ? first : undefined}
              onClick={() => {
                onClose();
                entry.onSelect?.();
              }}
            >
              {content}
            </button>
          );
        })}
      </div>
    </>
  );
}

/** A native select drawn as the design system's field. */
export function SelectField({
  label,
  icon,
  value,
  options,
  onChange,
}: {
  label: string;
  icon?: IconName;
  value: string;
  options: { value: string; label: string }[];
  onChange: (value: string) => void;
}) {
  return (
    <div className="kd-field">
      <label className="kd-field-label" htmlFor={`select-${label}`}>
        {label}
      </label>
      <div className="kd-field-box">
        {icon && <K.Icon name={icon} size={18} className="kd-field-icon" />}
        <select
          id={`select-${label}`}
          className="kd-field-input"
          value={value}
          onChange={(event) => onChange(event.target.value)}
        >
          {options.map((option) => (
            <option key={option.value} value={option.value}>
              {option.label}
            </option>
          ))}
        </select>
      </div>
    </div>
  );
}

/** Six PIN dots (E-02). */
export function PinDots({ filled, error }: { filled: number; error?: boolean }) {
  return (
    <div className="kt-pin" aria-hidden="true">
      {[0, 1, 2, 3, 4, 5].map((index) => (
        <span
          key={index}
          className={["kt-pin-dot", index < filled && "kt-pin-on", error && "kt-pin-err"]
            .filter(Boolean)
            .join(" ")}
        />
      ))}
    </div>
  );
}

/** The first letter of a title, for a login's tile. */
export function letterOf(title: string): string | undefined {
  const letter = title.trim().charAt(0).toUpperCase();
  return letter || undefined;
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
