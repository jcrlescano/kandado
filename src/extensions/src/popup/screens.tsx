// The popup screens, E-01 to E-14 (E-08, E-12 and E-15 arrive with their phases).
// What each contains is defined in docs/design-system/project/screens.md; how
// they look follows the Kandado Hardware extension boards.

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { ItemContentJson, Summary, VaultInfo } from "../background/keyring";
import type { Suggestion } from "../background/matcher";
import type { ErrorCode, Generated, GeneratorOptions, Status } from "../shared/messages";
import type { ItemKind } from "../shared/types";
import { copySecret } from "../ui/clipboard";
import {
  Badge,
  Field,
  Icon,
  ItemRow,
  Key,
  Label,
  Lcd,
  LcdKey,
  LcdStrip,
  LockMark,
  Logo,
  Panel,
  PinDisplay,
  SecretValue,
  Segmented,
  Select,
  StatusLight,
  Switch,
  Tag,
  TextField,
  TotpCode,
  Well,
} from "../ui/hardware";
import { AskError, ask } from "../ui/messaging";
import { BackHead, Body, Empty, Foot, hostOf, letterOf, Menu, type MenuEntry, Pop } from "./parts";

const codeOf = (error: unknown): ErrorCode => (error instanceof AskError ? error.code : "failed");

const KIND_TAGS: Record<ItemKind, string> = {
  login: "Login",
  alias: "Alias",
  card: "Card",
  note: "Note",
  identity: "ID",
  passkey: "Passkey",
  password: "Password",
};

// ---- E-01 ---------------------------------------------------------------------

const SIGN_IN_ERRORS: Partial<Record<ErrorCode, string>> = {
  no_keys:
    "This account has no master password yet. Set up Kandado in the desktop app first, then sign in here.",
  sign_in_failed: "Google sign-in did not finish. Try again.",
  offline: "Can't reach Kandado. Check your connection and try again.",
};

export function SignedOut({ onStatus }: { onStatus: (status: Status) => void }) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<ErrorCode | null>(null);
  const signIn = async () => {
    setBusy(true);
    setError(null);
    try {
      onStatus(await ask({ type: "sign-in" }));
    } catch (caught) {
      const code = codeOf(caught);
      if (code !== "sign_in_cancelled") setError(code);
    } finally {
      setBusy(false);
    }
  };
  return (
    <Pop>
      <Well as="section" className="kx-hero kx-hero-grow">
        <LockMark open label="Kandado, open" />
        <LcdStrip light="off" text="Signed out" />
      </Well>
      <Panel className="kx-sign-in">
        <h1 className="hw-h2">Your logins, one click away</h1>
        <p className="hw-sub kx-sub">Sign in to fill and save logins on the sites you visit.</p>
        {error && (
          <p className="hw-error kx-sub" role="alert">
            {SIGN_IN_ERRORS[error] ?? "Something went wrong. Try again."}
          </p>
        )}
        <Key tone="go" size="lg" block disabled={busy} onClick={signIn}>
          {busy ? "Signing in…" : "Continue with Google"}
        </Key>
        <p className="hw-sub kx-sub">
          New to Kandado? Set it up in the desktop app first, then sign in here.
        </p>
      </Panel>
    </Pop>
  );
}

// ---- E-02 ---------------------------------------------------------------------

export function Locked({
  status,
  onStatus,
}: {
  status: Extract<Status, { state: "locked" }>;
  onStatus: (status: Status) => void;
}) {
  const [mode, setMode] = useState<"pin" | "password">(status.pin ? "pin" : "password");
  const [pin, setPin] = useState("");
  const [password, setPassword] = useState("");
  const [shown, setShown] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const input = useRef<HTMLInputElement | null>(null);

  // The master password is masked again when the popup loses focus.
  useEffect(() => {
    if (!shown) return;
    const hide = () => setShown(false);
    window.addEventListener("blur", hide);
    return () => window.removeEventListener("blur", hide);
  }, [shown]);

  // "Unlock with the desktop app": shown when this extension is paired (E-31, D-45).
  const unlockWithDesktop = async () => {
    setBusy(true);
    setError(null);
    try {
      onStatus(await ask({ type: "unlock-desktop" }));
    } catch (caught) {
      const code = codeOf(caught);
      setError(
        code === "desktop_locked"
          ? "Unlock the Kandado desktop app first, then try again."
          : code === "desktop_unavailable"
            ? "The Kandado desktop app isn't running."
            : code === "desktop_off"
              ? "Unlocking the extension is turned off in the desktop app."
              : "The desktop app could not unlock Kandado here.",
      );
    } finally {
      setBusy(false);
    }
  };

  // Focus the PIN or password field whenever the mode changes.
  // biome-ignore lint/correctness/useExhaustiveDependencies: mode is the trigger, not a value read
  useEffect(() => {
    input.current?.focus();
  }, [mode]);

  const tryPin = async (value: string) => {
    setBusy(true);
    try {
      onStatus(await ask({ type: "unlock-pin", pin: value }));
    } catch (caught) {
      const code = codeOf(caught);
      setPin("");
      if (code === "pin_discarded") {
        setMode("password");
        setError("Too many wrong PINs. Unlock with your master password.");
      } else {
        const left = await ask({ type: "pin-attempts" }).catch(() => 0);
        setError(left === 1 ? "Wrong PIN. 1 attempt left." : `Wrong PIN. ${left} attempts left.`);
      }
    } finally {
      setBusy(false);
    }
  };

  const tryPassword = async () => {
    if (!password) return;
    setBusy(true);
    setError(null);
    try {
      const next = await ask({ type: "unlock", password });
      setPassword("");
      onStatus(next);
    } catch (caught) {
      setError(
        codeOf(caught) === "wrong_password"
          ? "That is not your master password."
          : "Kandado could not unlock. Try again.",
      );
    } finally {
      setBusy(false);
    }
  };

  const signOut = () => void ask({ type: "sign-out" }).then(onStatus, () => {});

  const hero = (
    <Well as="section" className="kx-hero">
      <LockMark label="Kandado is locked" />
      <LcdStrip light="warn" text="Locked" label={status.email} />
    </Well>
  );

  if (mode === "pin") {
    return (
      <Pop>
        {hero}
        <Panel className="kx-unlock">
          <h1 className="hw-h3">Enter your PIN</h1>
          <label className="kx-pin-label">
            <span className="kx-hidden-input">PIN</span>
            <PinDisplay filled={pin.length} error={error !== null && pin.length === 0} />
            <input
              ref={input}
              className="kx-hidden-input"
              inputMode="numeric"
              autoComplete="off"
              aria-label="PIN"
              maxLength={6}
              value={pin}
              disabled={busy}
              onChange={(event) => {
                const value = event.target.value.replace(/\D/g, "").slice(0, 6);
                setPin(value);
                setError(null);
                if (value.length === 6) void tryPin(value);
              }}
            />
          </label>
          {error ? (
            <p className="hw-error kx-sub" role="alert">
              {error}
            </p>
          ) : (
            <p className="hw-sub kx-sub">Six digits, set for this browser session.</p>
          )}
          {status.desktop && (
            <Key tone="go" size="lg" block disabled={busy} onClick={() => void unlockWithDesktop()}>
              Unlock with the desktop app
            </Key>
          )}
          <Key block onClick={() => setMode("password")}>
            Use master password
          </Key>
          <button type="button" className="kx-link" onClick={signOut}>
            Sign out
          </button>
        </Panel>
      </Pop>
    );
  }
  return (
    <Pop>
      {hero}
      <Panel
        as="form"
        className="kx-unlock"
        onSubmit={(event) => {
          event.preventDefault();
          void tryPassword();
        }}
      >
        <TextField
          ref={input}
          label="Master password"
          type={shown ? "text" : "password"}
          mono
          autoComplete="current-password"
          spellCheck={false}
          value={password}
          error={error}
          onChange={(event) => {
            setPassword(event.target.value);
            setError(null);
          }}
          trailing={
            <Key size="sm" aria-pressed={shown} onClick={() => setShown(!shown)}>
              {shown ? "Hide" : "Show"}
            </Key>
          }
        />
        <Key type="submit" tone="go" size="lg" block disabled={busy || !password}>
          {busy ? "Unlocking…" : "Unlock"}
        </Key>
        {status.pin && (
          <Key block onClick={() => setMode("pin")}>
            Use PIN
          </Key>
        )}
        {status.desktop && (
          <Key block disabled={busy} onClick={() => void unlockWithDesktop()}>
            Unlock with the desktop app
          </Key>
        )}
        <button type="button" className="kx-link" onClick={signOut}>
          Sign out
        </button>
      </Panel>
    </Pop>
  );
}

// ---- E-03, E-04, E-05, E-13, E-14 ------------------------------------------------

function matches(item: Summary, query: string): boolean {
  const needle = query.trim().toLowerCase();
  if (!needle) return true;
  return (
    item.title.toLowerCase().includes(needle) ||
    item.subtitle.toLowerCase().includes(needle) ||
    item.websites.some((site) => site.url.toLowerCase().includes(needle))
  );
}

function byTitle(a: Summary, b: Summary): number {
  return Number(b.favourite) - Number(a.favourite) || a.title.localeCompare(b.title);
}

const plural = (count: number, one: string, many: string) => `${count} ${count === 1 ? one : many}`;

export interface HomeProps {
  tabUrl: string | undefined;
  vaultFilter: string | null;
  onVaultFilter: (vault: string | null) => void;
  onOpen: (item: Summary) => void;
  /** A suggestion for this site was clicked: fill it into the page. */
  onFill: (item: Summary) => void;
  onCreateLogin: () => void;
  onGenerate: () => void;
  onLock: () => void;
  onSignOut: () => void;
  onToast: (text: string, tone?: "neutral" | "danger") => void;
  email: string;
}

export function Home(props: HomeProps) {
  const [items, setItems] = useState<Summary[]>([]);
  const [vaults, setVaults] = useState<VaultInfo[]>([]);
  const [suggestions, setSuggestions] = useState<Suggestion[]>([]);
  const [query, setQuery] = useState("");
  const [overlay, setOverlay] = useState<null | "menu" | "vaults">(null);
  const [loaded, setLoaded] = useState(false);
  const host = hostOf(props.tabUrl);

  const load = useCallback(async () => {
    const [all, vaultList, hits] = await Promise.all([
      ask({ type: "items" }),
      ask({ type: "vaults" }),
      host && props.tabUrl
        ? ask({ type: "suggestions", url: props.tabUrl })
        : Promise.resolve([] as Suggestion[]),
    ]);
    setItems(all);
    setVaults(vaultList);
    setSuggestions(hits);
    setLoaded(true);
  }, [host, props.tabUrl]);

  useEffect(() => {
    void load();
    // Pull what other devices changed, then draw again.
    void ask({ type: "sync" })
      .then((result) => (result.changed ? load() : undefined))
      .catch(() => {});
  }, [load]);

  const filter = props.vaultFilter;
  const inVault = (item: Summary) => !filter || item.vault_id === filter;
  const results = useMemo(
    () =>
      items
        .filter((item) => (!filter || item.vault_id === filter) && matches(item, query))
        .sort(byTitle),
    [items, query, filter],
  );
  const suggested = suggestions.filter((hit) => inVault(hit.item));
  const suggestedIds = new Set(suggested.map((hit) => hit.item.id));
  const activeVault = vaults.find((vault) => vault.id === props.vaultFilter);

  const copyPassword = async (item: Summary) => {
    try {
      const content = await ask({ type: "item", id: item.id });
      const copied = await copySecret(String(content.data.password ?? ""));
      props.onToast(
        copied ? "Password copied. Clears in 60 s." : "Could not copy",
        copied ? "neutral" : "danger",
      );
    } catch {
      props.onToast("Could not copy", "danger");
    }
  };

  // A suggestion for the site fills the page; any other row opens the item.
  const row = (item: Summary, { insecure = false, fills = false } = {}) => {
    const end = insecure ? (
      <Tag tone="warn">Not secure</Tag>
    ) : item.kind === "login" ? (
      <Key
        size="sm"
        tone={fills ? undefined : "flat"}
        aria-label={`Copy password for ${item.title}`}
        onClick={(event) => {
          event.stopPropagation();
          void copyPassword(item);
        }}
      >
        Copy
      </Key>
    ) : (
      <Tag>{KIND_TAGS[item.kind] ?? item.kind}</Tag>
    );
    return (
      <ItemRow
        key={item.id}
        title={item.favourite ? `${item.title} ★` : item.title}
        subtitle={item.subtitle || undefined}
        end={end}
        onClick={() => (fills && !insecure ? props.onFill(item) : props.onOpen(item))}
      />
    );
  };

  const menus: Record<"menu" | "vaults", () => React.ReactNode> = {
    menu: () => (
      <Menu
        label="Menu"
        onClose={() => setOverlay(null)}
        style={{ right: 10, top: 56, width: 260 }}
        header={
          <div className="kx-menu-account">
            <span className="hw-label">Signed in as</span>
            <span className="hw-sub">{props.email}</span>
          </div>
        }
        entries={[
          {
            key: "settings",
            label: "Settings",
            onSelect: () => void chrome.runtime.openOptionsPage(),
          },
          {
            key: "sync",
            label: "Sync now",
            onSelect: () =>
              void ask({ type: "sync" })
                .then(() => load())
                .then(() => props.onToast("Synced"))
                .catch(() => props.onToast("Couldn't sync. Check your connection.", "danger")),
          },
          "-",
          { key: "lock", label: "Lock", onSelect: props.onLock },
          { key: "sign-out", label: "Sign out", danger: true, onSelect: props.onSignOut },
        ]}
      />
    ),
    vaults: () => (
      <Menu
        label="Vaults"
        onClose={() => setOverlay(null)}
        style={{ left: 44, top: 56, width: 240 }}
        entries={[
          {
            key: "all",
            label: "All vaults",
            checked: props.vaultFilter === null,
            onSelect: () => props.onVaultFilter(null),
          },
          ...vaults.map(
            (vault): MenuEntry => ({
              key: vault.id,
              label: vault.name,
              light: true,
              checked: props.vaultFilter === vault.id,
              onSelect: () => props.onVaultFilter(vault.id),
            }),
          ),
        ]}
      />
    ),
  };

  const searching = query.trim() !== "";
  const vaultName = activeVault?.name ?? "All vaults";
  const lcd = host
    ? {
        light: suggested.length > 0 ? ("ok" as const) : ("off" as const),
        text: `${host} · ${plural(suggested.length, "login", "logins")}`,
        label: suggested.length > 0 ? "Click to fill" : undefined,
      }
    : {
        light: "ok" as const,
        text: `${vaultName} · ${plural(results.length, "item", "items")}`,
        label: undefined,
      };
  return (
    <Pop overlay={overlay ? menus[overlay]() : undefined}>
      <Panel as="header" className="kx-phead">
        <div className="kx-phead-row">
          <Logo size={26} />
          <Key
            size="sm"
            className="kx-vault-key"
            aria-haspopup="menu"
            aria-expanded={overlay === "vaults"}
            onClick={() => setOverlay("vaults")}
          >
            {`${vaultName} ▾`}
          </Key>
          <Key
            size="sm"
            aria-haspopup="menu"
            aria-expanded={overlay === "menu"}
            onClick={() => setOverlay("menu")}
          >
            Menu
          </Key>
          <Key size="sm" icon tone="dark" aria-label="Lock now" onClick={props.onLock}>
            <Icon name="lock" />
          </Key>
        </div>
        <label className="hw-well-row kx-search">
          <Icon name="search" />
          <input
            type="search"
            placeholder={activeVault ? `Search in ${activeVault.name}` : "Search in all vaults"}
            aria-label={activeVault ? `Search in ${activeVault.name}` : "Search in all vaults"}
            value={query}
            // biome-ignore lint/a11y/noAutofocus: the popup opens to search, as the design asks
            autoFocus
            onChange={(event) => setQuery(event.target.value)}
          />
        </label>
      </Panel>

      <LcdStrip {...lcd} />

      <Well className="kx-list">
        {searching ? (
          <>
            <Label className="kx-list-label">{`Results · ${results.length}`}</Label>
            {results.length > 0 ? (
              <div className="kx-listbox" role="listbox" aria-label="Results">
                {/* This site's items stay on top. */}
                {[
                  ...results.filter((item) => suggestedIds.has(item.id)),
                  ...results.filter((item) => !suggestedIds.has(item.id)),
                ].map((item) => row(item))}
              </div>
            ) : (
              <Empty title={`Nothing matches “${query.trim()}”`} />
            )}
          </>
        ) : (
          <>
            {host &&
              loaded &&
              (suggested.length > 0 ? (
                <div className="kx-listbox" role="listbox" aria-label={`Suggestions for ${host}`}>
                  {suggested.map((hit) => row(hit.item, { insecure: hit.insecure, fills: true }))}
                </div>
              ) : (
                <Empty title={`No logins for ${host}`}>
                  <Key size="sm" tone="go" onClick={props.onCreateLogin}>
                    {`Create login for ${host}`}
                  </Key>
                </Empty>
              ))}
            <Label className="kx-list-label">All items</Label>
            {loaded && results.length === 0 ? (
              <Empty
                title="Nothing here yet"
                body="Logins you save in Kandado on any device show up here."
              />
            ) : (
              <div className="kx-listbox" role="listbox" aria-label="All items">
                {results.map((item) => row(item))}
              </div>
            )}
          </>
        )}
      </Well>

      <Foot>
        <Key onClick={props.onGenerate}>Generate</Key>
        <Key tone="go" onClick={props.onCreateLogin}>
          + Create login
        </Key>
      </Foot>
    </Pop>
  );
}

// ---- E-06, E-07 and the other kinds ---------------------------------------------

function Totp({ id }: { id: string }) {
  const [code, setCode] = useState("");
  useEffect(() => {
    let stopped = false;
    const tick = () =>
      void ask({ type: "totp", id })
        .then((value) => {
          if (!stopped) setCode(value);
        })
        .catch(() => {});
    tick();
    // Ask again at the start of each 30-second window.
    const timer = window.setInterval(tick, 1000 * (30 - (Math.floor(Date.now() / 1000) % 30)));
    const steady = window.setInterval(tick, 30_000);
    return () => {
      stopped = true;
      window.clearInterval(timer);
      window.clearInterval(steady);
    };
  }, [id]);
  return code ? <TotpCode code={code} /> : null;
}

const text = (value: unknown) => (typeof value === "string" ? value : "");

function expiry(value: unknown): string {
  const date = value as { month?: number; year?: number } | undefined;
  if (!date?.month || !date.year) return "";
  return `${String(date.month).padStart(2, "0")} / ${String(date.year).slice(-2)}`;
}

function Notes({ notes }: { notes: string }) {
  if (!notes) return null;
  return (
    <Field label="Note">
      <Well>
        <p className="kx-note">{notes}</p>
      </Well>
    </Field>
  );
}

function CustomFields({ content }: { content: ItemContentJson }) {
  if (content.custom_fields.length === 0) return null;
  return (
    <>
      {content.custom_fields.map((field, index) => (
        <SecretValue
          // biome-ignore lint/suspicious/noArrayIndexKey: the list is fixed while the screen is open; labels can repeat
          key={`${field.label}-${index}`}
          label={field.label || "Field"}
          value={field.value}
          concealed={field.hidden}
          mono={field.hidden}
        />
      ))}
    </>
  );
}

function DetailBody({ content }: { content: ItemContentJson }) {
  const d = content.data;
  switch (content.kind) {
    case "login": {
      const websites = Array.isArray(d.websites) ? (d.websites as { url: string }[]) : [];
      return (
        <>
          {text(d.username) && (
            <SecretValue label="Username" value={text(d.username)} concealed={false} />
          )}
          <SecretValue label="Password" value={text(d.password)} />
          {websites.map((site, index) => (
            <SecretValue
              // biome-ignore lint/suspicious/noArrayIndexKey: the list is fixed while the screen is open; addresses can repeat
              key={`${site.url}-${index}`}
              label="Website"
              value={site.url}
              concealed={false}
              link
            />
          ))}
          <Notes notes={content.notes} />
          <CustomFields content={content} />
        </>
      );
    }
    case "card":
      return (
        <>
          {text(d.cardholder) && (
            <SecretValue
              label="Cardholder"
              value={text(d.cardholder)}
              concealed={false}
              mono={false}
            />
          )}
          <SecretValue label="Card number" value={text(d.number)} />
          {expiry(d.expiry) && (
            <SecretValue label="Expires" value={expiry(d.expiry)} concealed={false} />
          )}
          {text(d.cvv) && <SecretValue label="Security code" value={text(d.cvv)} />}
          {text(d.pin) && <SecretValue label="PIN" value={text(d.pin)} />}
          <Notes notes={content.notes} />
          <CustomFields content={content} />
        </>
      );
    case "identity": {
      const address = (d.address ?? {}) as Record<string, unknown>;
      const name = [d.first_name, d.middle_name, d.last_name].map(text).filter(Boolean).join(" ");
      const place = [
        address.line1,
        address.line2,
        address.city,
        address.region,
        address.postal_code,
        address.country,
      ]
        .map(text)
        .filter(Boolean)
        .join(", ");
      const rows: [string, string][] = [
        ["Name", name],
        ["Email", text(d.email)],
        ["Phone", text(d.phone)],
        ["Company", text(d.company)],
        ["Address", place],
      ];
      return (
        <>
          {rows
            .filter(([, value]) => value)
            .map(([label, value]) => (
              <SecretValue
                key={label}
                label={label}
                value={value}
                concealed={false}
                mono={label === "Email" || label === "Phone"}
              />
            ))}
          <Notes notes={content.notes} />
          <CustomFields content={content} />
        </>
      );
    }
    case "password":
      return (
        <>
          <SecretValue label="Password" value={text(d.password)} />
          <Notes notes={content.notes} />
        </>
      );
    case "alias":
      return <SecretValue label="Alias" value={text(d.address)} concealed={false} />;
    case "passkey":
      return (
        <>
          <SecretValue label="Website" value={text(d.rp_id)} concealed={false} />
          <SecretValue label="Username" value={text(d.user_name)} concealed={false} />
        </>
      );
    default:
      return (
        <>
          <Notes notes={content.notes} />
          <CustomFields content={content} />
        </>
      );
  }
}

export function Detail({
  item,
  onBack,
  onEdit,
  onFill,
}: {
  item: Summary;
  onBack: () => void;
  onEdit: (content: ItemContentJson) => void;
  /** Set when the tab is a web page: "Fill on this page" (E-06) or "Fill card" (E-07). */
  onFill?: () => void;
}) {
  // The decrypted item lives only while this screen is open.
  const [content, setContent] = useState<ItemContentJson | null>(null);
  const [vault, setVault] = useState("");
  const [failed, setFailed] = useState(false);
  useEffect(() => {
    let current = true;
    ask({ type: "item", id: item.id })
      .then((value) => {
        if (current) setContent(value);
      })
      .catch(() => setFailed(true));
    void ask({ type: "vaults" })
      .then((list) => {
        if (current) setVault(list.find((v) => v.id === item.vault_id)?.name ?? "");
      })
      .catch(() => {});
    return () => {
      current = false;
      setContent(null);
    };
  }, [item.id, item.vault_id]);
  const fills =
    onFill && (item.kind === "login" || item.kind === "card" || item.kind === "identity");
  return (
    <Pop>
      <BackHead
        onBack={onBack}
        title={
          <>
            <Badge letter={letterOf(item.title)} />
            <span className="kx-grow">
              <h1 className="kx-detail-title hw-h3">{item.title}</h1>
              {vault && <span className="hw-label">{vault}</span>}
            </span>
          </>
        }
      >
        {item.kind === "login" && content && (
          <Key size="sm" onClick={() => onEdit(content)}>
            Edit
          </Key>
        )}
      </BackHead>
      {item.has_totp && <Totp id={item.id} />}
      <Body>
        {failed && <p className="hw-error kx-sub">This item could not be opened.</p>}
        {content && <DetailBody content={content} />}
      </Body>
      {fills && (
        <Key tone="go" size="lg" block onClick={onFill}>
          {item.kind === "card" ? "Fill card" : "Fill on this page"}
        </Key>
      )}
    </Pop>
  );
}

// ---- E-10 (and editing a login from E-06) ----------------------------------------

const STRENGTH = ["Weak", "Weak", "Fair", "Good", "Strong"] as const;

function Strength({ score, bits, onLcd }: { score: number; bits?: number; onLcd?: boolean }) {
  const clamped = Math.max(0, Math.min(4, score));
  const words = `${STRENGTH[clamped]}${bits ? ` · ${Math.round(bits)} bits` : ""}`;
  return (
    <span className="kx-strength">
      <StatusLight tone={clamped >= 3 ? "ok" : "warn"} />
      <span className={onLcd ? "hw-lcd-label kx-grow" : "hw-sub"}>{words}</span>
    </span>
  );
}

export function LoginForm({
  tabUrl,
  editing,
  password: given,
  onDone,
  onCancel,
}: {
  tabUrl: string | undefined;
  editing?: { item: Summary; content: ItemContentJson };
  /** A password made in the generator ("Save as login"). */
  password?: string;
  onDone: (message: string) => void;
  onCancel: () => void;
}) {
  const host = hostOf(tabUrl);
  const start = editing?.content.data;
  const firstSite = Array.isArray(start?.websites)
    ? ((start.websites as { url: string }[])[0]?.url ?? "")
    : "";
  const [title, setTitle] = useState(editing?.content.title ?? titleFor(host));
  const [username, setUsername] = useState(text(start?.username));
  const [password, setPassword] = useState(given ?? text(start?.password));
  const [website, setWebsite] = useState(
    editing ? firstSite : host && tabUrl ? new URL(tabUrl).origin : "",
  );
  const [vaults, setVaults] = useState<VaultInfo[]>([]);
  const [vault, setVault] = useState(editing?.item.vault_id ?? "");
  const [strength, setStrength] = useState<{ score: number; bits?: number } | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const generate = useCallback(async () => {
    const result = await ask({ type: "generate", options: DEFAULT_GENERATOR });
    setPassword(result.password);
    setStrength({ score: result.score, bits: result.bits });
  }, []);

  const typed = useCallback(async (value: string) => {
    setPassword(value);
    setStrength(value ? { score: await ask({ type: "strength", password: value }) } : null);
  }, []);

  useEffect(() => {
    void ask({ type: "vaults" }).then((list) => {
      setVaults(list);
      setVault((current) => current || list[0]?.id || "");
    });
    if (given) void typed(given);
    else if (!editing) void generate();
  }, [editing, generate, given, typed]);

  const save = async () => {
    setBusy(true);
    setError(null);
    const login = { vault_id: vault, title, username, password, url: website };
    try {
      if (editing) await ask({ type: "update-login", id: editing.item.id, login });
      else await ask({ type: "create-login", login });
      onDone(editing ? "Login saved" : "Login created");
    } catch {
      setError("The login could not be saved. Try again.");
      setBusy(false);
    }
  };

  return (
    <Pop>
      <BackHead title={editing ? "Edit login" : "New login"} onBack={onCancel}>
        {vaults.length > 1 && !editing && (
          <Select
            aria-label="Vault"
            value={vault}
            options={vaults.map((v) => ({ value: v.id, label: v.name }))}
            onChange={(event) => setVault(event.target.value)}
          />
        )}
      </BackHead>
      <Body
        as="form"
        onSubmit={(event) => {
          event.preventDefault();
          void save();
        }}
      >
        <TextField label="Name" value={title} onChange={(e) => setTitle(e.target.value)} />
        <TextField
          label="Email or username"
          placeholder="name@example.com"
          autoComplete="off"
          value={username}
          onChange={(e) => setUsername(e.target.value)}
        />
        <Field
          label="Password"
          htmlFor="login-password"
          hint={strength && <Strength {...strength} />}
        >
          <Lcd className="kx-lcd-input">
            <input
              id="login-password"
              autoComplete="off"
              spellCheck={false}
              value={password}
              onChange={(e) => void typed(e.target.value)}
            />
            <LcdKey aria-label="Generate a new password" onClick={() => void generate()}>
              New
            </LcdKey>
          </Lcd>
        </Field>
        <TextField
          label="Website"
          mono
          placeholder="https://"
          value={website}
          onChange={(e) => setWebsite(e.target.value)}
        />
        {error && (
          <p className="hw-error kx-sub" role="alert">
            {error}
          </p>
        )}
        <button type="submit" hidden aria-hidden="true" tabIndex={-1} />
      </Body>
      <Foot>
        <Key onClick={onCancel}>Cancel</Key>
        <Key tone="go" className="kx-wide" disabled={busy || !password} onClick={() => void save()}>
          {editing ? "Save" : "Create login"}
        </Key>
      </Foot>
    </Pop>
  );
}

function titleFor(host: string): string {
  if (!host) return "";
  const name = host.split(".").slice(-2, -1)[0] ?? host;
  return name.charAt(0).toUpperCase() + name.slice(1);
}

// ---- E-11 ---------------------------------------------------------------------

const DEFAULT_GENERATOR: GeneratorOptions = {
  mode: "random",
  length: 20,
  uppercase: true,
  numbers: true,
  symbols: true,
};

/**
 * The generator, with the password made by the shared Rust generator, so every
 * client generates the same way. The password shows on the LCD.
 */
export function Generator({
  onBack,
  onToast,
  onSaveAsLogin,
}: {
  onBack: () => void;
  onToast: (text: string, tone?: "neutral" | "danger") => void;
  onSaveAsLogin: (password: string) => void;
}) {
  const [options, setOptions] = useState<GeneratorOptions>(DEFAULT_GENERATOR);
  const [result, setResult] = useState<Generated | null>(null);
  const memorable = options.mode === "memorable";

  const run = useCallback(async (next: GeneratorOptions) => {
    setResult(await ask({ type: "generate", options: next }));
  }, []);
  useEffect(() => {
    void run(options);
  }, [options, run]);

  const set = (
    patch: Partial<{
      length: number;
      words: number;
      uppercase: boolean;
      numbers: boolean;
      symbols: boolean;
    }>,
  ) => setOptions((current) => ({ ...current, ...patch }) as GeneratorOptions);
  const setMode = (mode: "random" | "memorable") =>
    setOptions((current) =>
      mode === "memorable"
        ? { mode, words: 4, uppercase: current.uppercase, numbers: current.numbers, symbols: true }
        : {
            mode,
            length: 20,
            uppercase: current.uppercase,
            numbers: current.numbers,
            symbols: current.symbols,
          },
    );
  const size = memorable
    ? (options as { words: number }).words
    : (options as { length: number }).length;

  const copy = async () => {
    if (!result) return;
    const copied = await copySecret(result.password);
    onToast(
      copied ? "Password copied. Clears in 60 s." : "Could not copy",
      copied ? "neutral" : "danger",
    );
  };

  const toggles: { key: "uppercase" | "numbers" | "symbols"; label: string }[] = [
    { key: "uppercase", label: memorable ? "Capitalise" : "Capitals" },
    { key: "numbers", label: "Numbers" },
    { key: "symbols", label: memorable ? "Hyphens between words" : "Symbols" },
  ];

  return (
    <Pop>
      <BackHead title="Generate password" onBack={onBack} />
      <Lcd className="kx-gen-out">
        <span className="hw-lcd-digits kx-gen-value" aria-live="polite">
          {result?.password ?? ""}
        </span>
        <span className="kx-gen-meta">
          {result && <Strength score={result.score} bits={result.bits} onLcd />}
          <LcdKey aria-label="Generate again" onClick={() => void run(options)}>
            New
          </LcdKey>
        </span>
      </Lcd>
      <Body>
        <Segmented
          label="Kind"
          block
          value={options.mode}
          options={[
            { value: "random", label: "Random" },
            { value: "memorable", label: "Memorable" },
          ]}
          onChange={setMode}
        />
        <Field
          label={memorable ? "Words" : "Characters"}
          htmlFor="generator-size"
          end={<span className="hw-mono kx-range-value">{size}</span>}
        >
          <input
            id="generator-size"
            className="kx-range"
            type="range"
            min={memorable ? 3 : 8}
            max={memorable ? 8 : 64}
            value={size}
            onChange={(event) =>
              set(
                memorable
                  ? { words: Number(event.target.value) }
                  : { length: Number(event.target.value) },
              )
            }
          />
        </Field>
        <Well className="kx-toggles">
          {toggles.map((toggle) => (
            <div key={toggle.key} className="kx-row">
              <span className="kx-row-text">{toggle.label}</span>
              <Switch
                label={toggle.label}
                checked={options[toggle.key]}
                onChange={(value) => set({ [toggle.key]: value })}
              />
            </div>
          ))}
        </Well>
      </Body>
      <Foot>
        <Key disabled={!result} onClick={() => result && onSaveAsLogin(result.password)}>
          Save as login
        </Key>
        <Key tone="go" disabled={!result} onClick={() => void copy()}>
          Copy password
        </Key>
      </Foot>
    </Pop>
  );
}
