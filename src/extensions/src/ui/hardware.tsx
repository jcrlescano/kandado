// Kandado Hardware for the extension pages: small typed React components that
// render the design system's `hw-` classes (vendor/hardware/hardware.css, copied
// from docs/ by scripts/sync-ui.mjs). Hardware itself is plain CSS; these only
// add the markup, the ARIA and the few behaviours its guidelines ask for
// (masked secrets, re-masking on blur, countdown bars that step every 3 s).

import {
  type ButtonHTMLAttributes,
  type CSSProperties,
  forwardRef,
  type HTMLAttributes,
  type InputHTMLAttributes,
  type KeyboardEvent,
  type ReactNode,
  type SelectHTMLAttributes,
  useEffect,
  useId,
  useRef,
  useState,
} from "react";
import { ICON_VIEW_BOX, ICONS, type IconName } from "./vendor/hardware/icons";
import "./vendor/hardware/tokens.css";
import "./vendor/hardware/hardware.css";
import "./fonts.css";
import "./extension.css";
import { copySecret } from "./clipboard";

const cx = (...names: (string | false | null | undefined)[]) => names.filter(Boolean).join(" ");

/** What a password looks like until Show: 12 bullets, whatever its length. */
export const MASK = "••••••••••••";

// ---- Keys --------------------------------------------------------------------

export interface KeyProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  /** go: the one main action of a view. dark: Lock. danger: irreversible. flat: low emphasis. */
  tone?: "go" | "dark" | "danger" | "flat";
  size?: "sm" | "lg";
  block?: boolean;
  /** Square key; give it an aria-label. */
  icon?: boolean;
}

export const Key = forwardRef<HTMLButtonElement, KeyProps>(function Key(
  { tone, size, block, icon, className, type, ...rest },
  ref,
) {
  return (
    <button
      ref={ref}
      type={type ?? "button"}
      className={cx(
        "hw-key",
        tone && `hw-key--${tone}`,
        size && `hw-key--${size}`,
        block && "hw-key--block",
        icon && "hw-key--icon",
        className,
      )}
      {...rest}
    />
  );
});

/** A key set into the LCD glass (COPY, NEW). */
export function LcdKey({ className, type, ...rest }: ButtonHTMLAttributes<HTMLButtonElement>) {
  return <button type={type ?? "button"} className={cx("hw-lcd-key", className)} {...rest} />;
}

// ---- Surfaces ------------------------------------------------------------------

type SurfaceTag = "div" | "header" | "footer" | "section" | "nav" | "main" | "form" | "aside";

export interface SurfaceProps extends HTMLAttributes<HTMLElement> {
  as?: SurfaceTag;
}

/** A raised module on the body ground. Never nest a panel in a panel. */
export function Panel({ as: Tag = "div", className, ...rest }: SurfaceProps) {
  return <Tag className={cx("hw-panel", className)} {...rest} />;
}

/** Cut into a panel: inputs, lists, groups of rows. */
export function Well({ as: Tag = "div", className, ...rest }: SurfaceProps) {
  return <Tag className={cx("hw-well", className)} {...rest} />;
}

/** A key face used as a container. */
export function Raised({ as: Tag = "div", className, ...rest }: SurfaceProps) {
  return <Tag className={cx("hw-raised", className)} {...rest} />;
}

export function Banner({ tone, children }: { tone?: "danger"; children: ReactNode }) {
  return (
    <div className={cx("hw-banner", tone === "danger" && "hw-banner--danger")} role="note">
      <StatusLight tone="warn" />
      <span>{children}</span>
    </div>
  );
}

// ---- Status --------------------------------------------------------------------

/** A status light. It never carries meaning alone: put a word beside it. */
export function StatusLight({
  tone = "ok",
  style,
}: {
  tone?: "ok" | "warn" | "off";
  style?: CSSProperties;
}) {
  return (
    <span
      className={cx("hw-led", tone === "warn" && "hw-led--warn", tone === "off" && "hw-led--off")}
      style={style}
      aria-hidden="true"
    />
  );
}

/** An engraved label: an item's kind, a small state. */
export function Tag({ tone, children }: { tone?: "go" | "warn"; children: ReactNode }) {
  return <span className={cx("hw-tag", tone && `hw-tag--${tone}`)}>{children}</span>;
}

/** An engraved label above a field or a list. */
export function Label({
  children,
  htmlFor,
  className,
}: {
  children: ReactNode;
  htmlFor?: string;
  className?: string;
}) {
  return htmlFor ? (
    <label className={cx("hw-label", className)} htmlFor={htmlFor}>
      {children}
    </label>
  ) : (
    <span className={cx("hw-label", className)}>{children}</span>
  );
}

export function Kbd({ children }: { children: ReactNode }) {
  return <kbd className="hw-kbd">{children}</kbd>;
}

/** The initial of an item, on a small dark key. */
export function Badge({ letter, small = true }: { letter: string; small?: boolean }) {
  return (
    <span className={cx("hw-badge-ico", small && "hw-badge-ico--sm")} aria-hidden="true">
      {letter}
    </span>
  );
}

// ---- LCD -----------------------------------------------------------------------

export function Lcd({ className, ...rest }: HTMLAttributes<HTMLDivElement>) {
  return <div className={cx("hw-lcd", className)} {...rest} />;
}

/**
 * A status strip: a light, a short readout and an optional label at the end. The
 * readout is written in sentence case and shown in capitals by the stylesheet,
 * so screen readers and tests read words, not letters.
 */
export function LcdStrip({
  light,
  text,
  label,
  className,
}: {
  light?: "ok" | "warn" | "off";
  text: string;
  label?: string;
  className?: string;
}) {
  return (
    <Lcd className={cx("kx-lcd-strip", className)}>
      {light && <StatusLight tone={light} />}
      <span className="hw-lcd-digits kx-caps kx-lcd-text">{text}</span>
      {label && <span className="hw-lcd-label kx-lcd-end">{label}</span>}
    </Lcd>
  );
}

/** "482913" → "482 913". */
export function grouped(code: string): string {
  return code.length === 6 ? `${code.slice(0, 3)} ${code.slice(3)}` : code;
}

/** Ten countdown bars that step once every 3 seconds. */
export function Countdown({ remaining, period = 30 }: { remaining: number; period?: number }) {
  const lit = Math.max(0, Math.min(10, Math.ceil((remaining / period) * 10)));
  return (
    <span className="kx-bars" role="img" aria-label={`${remaining} seconds left`}>
      {Array.from({ length: 10 }, (_, index) => (
        <span
          // biome-ignore lint/suspicious/noArrayIndexKey: ten fixed bars; position is the identity
          key={index}
          className={cx("hw-lcd-bar", index >= lit && "hw-lcd-bar--off")}
        />
      ))}
    </span>
  );
}

function secondsLeft(period: number): number {
  return period - (Math.floor(Date.now() / 1000) % period);
}

/** A two-factor code on the LCD, with its countdown and a COPY key. */
export function TotpCode({ code, period = 30 }: { code: string; period?: number }) {
  const [left, setLeft] = useState(() => secondsLeft(period));
  const [copied, setCopied] = useState(false);
  useEffect(() => {
    const timer = window.setInterval(() => setLeft(secondsLeft(period)), 1000);
    return () => window.clearInterval(timer);
  }, [period]);
  const copy = async () => {
    if (await copySecret(code)) {
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1400);
    }
  };
  return (
    <Lcd className="kx-totp">
      <span className="kx-totp-text">
        <span className="hw-lcd-label">Two-factor code</span>
        <span className="hw-lcd-digits kx-totp-code">{grouped(code)}</span>
      </span>
      <Countdown remaining={left} period={period} />
      <LcdKey aria-label="Copy two-factor code" onClick={() => void copy()}>
        {copied ? "COPIED" : "COPY"}
      </LcdKey>
    </Lcd>
  );
}

/** Six PIN positions on the LCD: lit when typed, dim when still to come. */
export function PinDisplay({ filled, error }: { filled: number; error?: boolean }) {
  return (
    <Lcd className={cx("kx-pin", error && "kx-pin-error")} aria-hidden="true">
      {[0, 1, 2, 3, 4, 5].map((index) => (
        <span key={index} className={cx("kx-pin-cell", index < filled && "kx-pin-on")} />
      ))}
    </Lcd>
  );
}

// ---- Fields --------------------------------------------------------------------

/** A labelled control: an engraved label above, an error or a hint below. */
export function Field({
  label,
  htmlFor,
  error,
  errorId,
  hint,
  end,
  children,
}: {
  label: string;
  htmlFor?: string;
  error?: string | null;
  errorId?: string;
  hint?: ReactNode;
  /** Shown at the end of the label line (a slider's value). */
  end?: ReactNode;
  children: ReactNode;
}) {
  return (
    <div className="hw-field">
      {end ? (
        <span className="kx-label-line">
          <Label htmlFor={htmlFor}>{label}</Label>
          {end}
        </span>
      ) : (
        <Label htmlFor={htmlFor}>{label}</Label>
      )}
      {children}
      {error ? (
        <span className="hw-error" id={errorId} role="alert">
          {error}
        </span>
      ) : (
        hint && <span className="hw-hint">{hint}</span>
      )}
    </div>
  );
}

/** A read-only value in a well, with keys set into its end. */
export function WellRow({ children, className }: { children: ReactNode; className?: string }) {
  return <div className={cx("hw-well-row", className)}>{children}</div>;
}

export interface TextFieldProps extends Omit<InputHTMLAttributes<HTMLInputElement>, "size"> {
  label: string;
  error?: string | null;
  hint?: ReactNode;
  mono?: boolean;
  /** Keys set into the end of the well (Show, Generate, Add). */
  trailing?: ReactNode;
}

export const TextField = forwardRef<HTMLInputElement, TextFieldProps>(function TextField(
  { label, error, hint, mono, trailing, id, className, ...rest },
  ref,
) {
  const auto = useId();
  const inputId = id ?? auto;
  const errorId = `${inputId}-error`;
  const aria = {
    "aria-invalid": error ? true : undefined,
    "aria-describedby": error ? errorId : undefined,
  } as const;
  return (
    <Field label={label} htmlFor={inputId} error={error} errorId={errorId} hint={hint}>
      {trailing ? (
        <div className={cx("hw-well-row", error && "kx-invalid")}>
          <input
            ref={ref}
            id={inputId}
            className={cx("kx-bare-input", mono && "hw-mono", className)}
            {...aria}
            {...rest}
          />
          {trailing}
        </div>
      ) : (
        <input
          ref={ref}
          id={inputId}
          className={cx("hw-input", mono && "hw-input--mono", className)}
          {...aria}
          {...rest}
        />
      )}
    </Field>
  );
});

/** A native select drawn as a small key, for short lists (vaults, a prompt's vault). */
export function Select({
  options,
  className,
  ...rest
}: SelectHTMLAttributes<HTMLSelectElement> & { options: { value: string; label: string }[] }) {
  return (
    <select className={cx("hw-key hw-key--sm kx-select", className)} {...rest}>
      {options.map((option) => (
        <option key={option.value} value={option.value}>
          {option.label}
        </option>
      ))}
    </select>
  );
}

/** Hides a revealed secret again when the window loses focus. */
function useRemaskOnBlur(shown: boolean, hide: () => void) {
  useEffect(() => {
    if (!shown) return;
    window.addEventListener("blur", hide);
    return () => window.removeEventListener("blur", hide);
  }, [shown, hide]);
}

/**
 * A copyable value in a field well. Concealed values show as 12 bullets until
 * Show, and are masked again when the window loses focus.
 */
export function SecretValue({
  label,
  value,
  concealed = true,
  mono = true,
  link,
}: {
  label: string;
  value: string;
  concealed?: boolean;
  mono?: boolean;
  /** Draw the value as a link (a website). */
  link?: boolean;
}) {
  const [shown, setShown] = useState(false);
  const [copied, setCopied] = useState(false);
  const hide = useRef(() => setShown(false)).current;
  useRemaskOnBlur(concealed && shown, hide);
  const masked = concealed && !shown;
  const copy = async () => {
    if (await copySecret(value)) {
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1400);
    }
  };
  const safeLink = link && /^https?:\/\//i.test(value);
  return (
    <Field label={label}>
      <WellRow>
        {safeLink ? (
          <a className="kx-value hw-mono" href={value} target="_blank" rel="noreferrer noopener">
            {value}
          </a>
        ) : (
          <span className={cx("kx-value", (mono || masked) && "hw-mono", masked && "kx-masked")}>
            {masked ? MASK : value}
          </span>
        )}
        {concealed && (
          <Key size="sm" aria-pressed={shown} onClick={() => setShown(!shown)}>
            {shown ? "Hide" : "Show"}
          </Key>
        )}
        <Key
          size="sm"
          aria-label={copied ? "Copied" : `Copy ${label.toLowerCase()}`}
          onClick={() => void copy()}
        >
          {copied ? "Copied" : "Copy"}
        </Key>
      </WellRow>
    </Field>
  );
}

// ---- Choice controls -------------------------------------------------------------

/** Arrow keys move between the options of a radio group or tab list. */
function arrowKeys(event: KeyboardEvent<HTMLDivElement>) {
  const step =
    event.key === "ArrowRight" || event.key === "ArrowDown"
      ? 1
      : event.key === "ArrowLeft" || event.key === "ArrowUp"
        ? -1
        : 0;
  if (!step) return;
  const buttons = Array.from(event.currentTarget.querySelectorAll<HTMLButtonElement>("button"));
  const index = buttons.indexOf(document.activeElement as HTMLButtonElement);
  const next = buttons[(index + step + buttons.length) % buttons.length];
  if (!next) return;
  event.preventDefault();
  next.focus();
  next.click();
}

/** 2 to 6 exclusive options sunk into a well; the chosen one rises as a key face. */
export function Segmented<T extends string>({
  label,
  options,
  value,
  onChange,
  block,
  onPanel,
}: {
  label: string;
  options: { value: T; label: string }[];
  value: T;
  onChange: (value: T) => void;
  block?: boolean;
  /** On a well background, so the control still reads as sunk. */
  onPanel?: boolean;
}) {
  return (
    <div
      className={cx("hw-seg kx-seg", block && "kx-seg-block", onPanel && "kx-seg-on-well")}
      role="radiogroup"
      aria-label={label}
      onKeyDown={arrowKeys}
    >
      {options.map((option) => (
        // biome-ignore lint/a11y/useSemanticElements: see above
        <button
          key={option.value}
          type="button"
          role="radio"
          aria-checked={option.value === value}
          tabIndex={option.value === value ? 0 : -1}
          onClick={() => onChange(option.value)}
        >
          {option.label}
        </button>
      ))}
    </div>
  );
}

/** An on/off switch for a setting that applies at once. */
export function Switch({
  label,
  checked,
  disabled,
  onChange,
}: {
  label: string;
  checked: boolean;
  disabled?: boolean;
  onChange?: (on: boolean) => void;
}) {
  return (
    <button
      type="button"
      role="switch"
      className="hw-switch"
      aria-checked={checked}
      aria-label={label}
      disabled={disabled}
      onClick={() => onChange?.(!checked)}
    />
  );
}

// ---- Lists and navigation ---------------------------------------------------------

export interface ItemRowProps extends HTMLAttributes<HTMLDivElement> {
  title: string;
  subtitle?: string;
  selected?: boolean;
  /** A kind tag, a Copy key or a keycap at the end of the row. */
  end?: ReactNode;
}

/** One item in a list well. Enter and Space activate it like a click. */
export function ItemRow({
  title,
  subtitle,
  selected,
  end,
  className,
  onKeyDown,
  ...rest
}: ItemRowProps) {
  return (
    <div
      role="option"
      aria-selected={!!selected}
      tabIndex={0}
      className={cx("hw-item kx-item", selected && "hw-item--on", className)}
      onKeyDown={(event) => {
        onKeyDown?.(event);
        if (event.defaultPrevented || event.target !== event.currentTarget) return;
        if (event.key === "Enter" || event.key === " ") {
          event.preventDefault();
          event.currentTarget.click();
        }
      }}
      {...rest}
    >
      <span className="kx-item-text">
        <span className="hw-item-title">{title}</span>
        {subtitle && <span className="hw-item-sub">{subtitle}</span>}
      </span>
      {end}
    </div>
  );
}

export interface NavButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  on?: boolean;
  raised?: boolean;
  count?: ReactNode;
}

export const NavButton = forwardRef<HTMLButtonElement, NavButtonProps>(function NavButton(
  { on, raised, count, className, children, type, ...rest },
  ref,
) {
  return (
    <button
      ref={ref}
      type={type ?? "button"}
      className={cx("hw-navbtn", raised && "hw-navbtn--raised", on && "hw-navbtn--on", className)}
      {...rest}
    >
      {children}
      {count !== undefined && <span className="hw-count kx-count">{count}</span>}
    </button>
  );
});

// ---- The mark ---------------------------------------------------------------------

/** The padlock mark, drawn inline so it follows the theme. */
export function Logo({
  size = 26,
  label = "Kandado",
  wordmark,
}: {
  size?: number;
  label?: string;
  wordmark?: boolean | string;
}) {
  const mark = (
    <svg
      width={(size * 30) / 32}
      height={size}
      viewBox="0 0 30 32"
      role="img"
      aria-label={wordmark ? undefined : label}
      aria-hidden={wordmark ? true : undefined}
    >
      <path className="hw-logo-shackle" d="M8 13 V9 a7 7 0 0 1 14 0 V13" />
      <rect className="hw-logo-body" x="3" y="12" width="24" height="18" rx="5" />
      <circle className="hw-logo-hole" cx="15" cy="20" r="2.6" />
      <rect className="hw-logo-hole" x="14" y="21" width="2" height="5" rx="1" />
    </svg>
  );
  if (!wordmark) return mark;
  return (
    <span className="kx-wordmark">
      {mark}
      <span className="hw-mono kx-logotype">
        {typeof wordmark === "string" ? wordmark : "KANDADO"}
      </span>
    </span>
  );
}

/** The large mark on the signed-out (open shackle) and locked (closed) screens. */
export function LockMark({ open, label }: { open?: boolean; label: string }) {
  return open ? (
    <svg width="110" height="130" viewBox="0 0 180 220" role="img" aria-label={label}>
      <path
        className="hw-logo-shackle kx-mark-shackle"
        d="M48 72 V60 a42 42 0 0 1 84 0 V40"
        transform="translate(0 32)"
      />
      <rect className="hw-logo-body" x="18" y="104" width="144" height="108" rx="26" />
      <circle className="hw-logo-hole" cx="90" cy="150" r="13" />
      <rect className="hw-logo-hole" x="84" y="154" width="12" height="30" rx="6" />
    </svg>
  ) : (
    <svg width="96" height="110" viewBox="0 0 180 200" role="img" aria-label={label}>
      <path className="hw-logo-shackle kx-mark-shackle" d="M48 92 V60 a42 42 0 0 1 84 0 V92" />
      <rect className="hw-logo-body" x="18" y="84" width="144" height="108" rx="26" />
      <circle className="hw-logo-hole" cx="90" cy="130" r="13" />
      <rect className="hw-logo-hole" x="84" y="134" width="12" height="30" rx="6" />
    </svg>
  );
}

// ---- Icons ------------------------------------------------------------------------

/**
 * An icon from the Hardware set (vendor/hardware/icons.ts, generated from the
 * design system's icons.json). Most actions are still words on keys; an icon
 * goes before its word, or alone on a square key with an aria-label.
 */
export function Icon({ name, size }: { name: IconName; size?: "sm" | "lg" }) {
  return (
    <svg
      className={cx("hw-ico", size === "sm" && "hw-ico--sm", size === "lg" && "hw-ico--lg")}
      viewBox={ICON_VIEW_BOX}
      aria-hidden="true"
    >
      {ICONS[name].map(([tag, props], index) => {
        const Shape = tag as "path";
        // biome-ignore lint/suspicious/noArrayIndexKey: an icon's shapes never reorder
        return <Shape key={index} {...props} />;
      })}
    </svg>
  );
}

// ---- Toast ------------------------------------------------------------------------

/** A quiet confirmation on the LCD ("Password copied"). */
export function Toast({
  tone = "neutral",
  children,
}: {
  tone?: "neutral" | "danger";
  children: ReactNode;
}) {
  return (
    <Lcd className={cx("kx-toast", tone === "danger" && "kx-toast-danger")} role="status">
      <StatusLight tone={tone === "danger" ? "warn" : "ok"} />
      <span className="kx-toast-text">{children}</span>
    </Lcd>
  );
}
