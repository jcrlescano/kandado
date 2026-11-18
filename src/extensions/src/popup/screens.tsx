// The popup screens, E-01 to E-14 (E-08, E-12 and E-15 arrive with their phases).
// What each contains is defined in docs/design-system/project/screens.md.

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { ItemContentJson, Summary, VaultInfo } from "../background/keyring";
import type { Suggestion } from "../background/matcher";
import type { ErrorCode, Generated, GeneratorOptions, Status } from "../shared/messages";
import { K } from "../ui/kandado";
import { AskError, ask } from "../ui/messaging";
import type { IconName } from "../ui/vendor/kandado";
import {
  BackHead,
  Body,
  ColumnFoot,
  Empty,
  Foot,
  Group,
  Head,
  hostOf,
  List,
  letterOf,
  Menu,
  type MenuEntry,
  PinDots,
  Pop,
  Section,
  SelectField,
} from "./parts";

const codeOf = (error: unknown): ErrorCode => (error instanceof AskError ? error.code : "failed");

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
      <Body center>
        <K.Logo size={64} />
        <h1 className="kt-display" style={{ fontSize: 26, lineHeight: "32px" }}>
          Kandado
        </h1>
        <p className="kt-p">Sign in to fill passwords and save logins as you browse.</p>
        {error && (
          <p className="kx-error" role="alert">
            {SIGN_IN_ERRORS[error] ?? "Something went wrong. Try again."}
          </p>
        )}
      </Body>
      <ColumnFoot>
        <K.Button variant="primary" size="lg" block disabled={busy} onClick={signIn}>
          {busy ? "Signing in…" : "Continue with Google"}
        </K.Button>
      </ColumnFoot>
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
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const input = useRef<HTMLInputElement | null>(null);

  // "Unlock with desktop app": shown when this extension is paired (E-31, D-45).
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
  const desktopButton = status.desktop ? (
    <K.Button
      variant="primary"
      block
      icon="grid"
      disabled={busy}
      onClick={() => void unlockWithDesktop()}
    >
      Unlock with desktop app
    </K.Button>
  ) : null;

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

  if (mode === "pin") {
    return (
      <Pop>
        <Body center>
          <K.Logo size={48} />
          <h1 className="kt-display" style={{ fontSize: 24, lineHeight: "30px" }}>
            Enter your PIN
          </h1>
          <label style={{ cursor: "text", display: "flex", flexDirection: "column", gap: 12 }}>
            <span className="kx-hidden-input">PIN</span>
            <PinDots filled={pin.length} error={error !== null && pin.length === 0} />
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
            <p className="kx-error" role="alert">
              {error}
            </p>
          ) : (
            <p className="kt-p">{status.email}</p>
          )}
        </Body>
        <ColumnFoot>
          {desktopButton}
          <K.Button variant="ghost" block onClick={() => setMode("password")}>
            Use master password
          </K.Button>
        </ColumnFoot>
      </Pop>
    );
  }
  return (
    <Pop>
      <Body center>
        <K.Logo size={48} />
        <h1 className="kt-display" style={{ fontSize: 24, lineHeight: "30px" }}>
          Unlock Kandado
        </h1>
        <p className="kt-p">{status.email}</p>
        <form
          style={{ width: "100%", textAlign: "left" }}
          onSubmit={(event) => {
            event.preventDefault();
            void tryPassword();
          }}
        >
          <K.TextField
            label="Master password"
            type="password"
            icon="lock"
            autoComplete="current-password"
            value={password}
            autoFocus
            error={error ?? undefined}
            onChange={(event) => {
              setPassword(event.target.value);
              setError(null);
            }}
          />
        </form>
      </Body>
      <ColumnFoot>
        <K.Button variant="primary" block disabled={busy || !password} onClick={tryPassword}>
          {busy ? "Unlocking…" : "Unlock"}
        </K.Button>
        {status.pin && (
          <K.Button variant="ghost" block onClick={() => setMode("pin")}>
            Use PIN
          </K.Button>
        )}
        {desktopButton && (
          <K.Button
            variant="ghost"
            block
            icon="grid"
            disabled={busy}
            onClick={() => void unlockWithDesktop()}
          >
            Unlock with desktop app
          </K.Button>
        )}
      </ColumnFoot>
    </Pop>
  );
}

// ---- E-03, E-04, E-05, E-09, E-13, E-14 ------------------------------------------

const VAULT_ICONS: ReadonlySet<string> = new Set([
  "vault",
  "user",
  "bolt",
  "card",
  "star",
  "shield",
  "globe",
  "key",
]);

function vaultIcon(vault: VaultInfo): IconName {
  return (VAULT_ICONS.has(vault.icon) ? vault.icon : "vault") as IconName;
}

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
  const [overlay, setOverlay] = useState<null | "create" | "menu" | "vaults">(null);
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
      const password = String(content.data.password ?? "");
      await navigator.clipboard.writeText(password);
      props.onToast("Password copied");
    } catch {
      props.onToast("Could not copy", "danger");
    }
  };

  // A suggestion for the site fills the page; any other row opens the item.
  const row = (item: Summary, dense: boolean, insecure = false, fills = false) => {
    const activate = () => (fills && !insecure ? props.onFill(item) : props.onOpen(item));
    return (
      <K.ItemRow
        key={item.id}
        kind={item.kind}
        title={item.title}
        subtitle={item.subtitle || undefined}
        letter={item.kind === "login" ? letterOf(item.title) : undefined}
        favorite={item.favourite}
        dense={dense}
        badge={insecure ? <K.Badge tone="warning">Not secure</K.Badge> : undefined}
        onCopy={item.kind === "login" ? () => void copyPassword(item) : undefined}
        onClick={activate}
        onKeyDown={(event) => {
          if (event.key === "Enter" || event.key === " ") {
            event.preventDefault();
            activate();
          }
        }}
      />
    );
  };

  const menus: Record<"create" | "menu" | "vaults", () => React.ReactNode> = {
    create: () => (
      <Menu
        label="Create"
        onClose={() => setOverlay(null)}
        style={{ left: 12, bottom: 64, width: 240 }}
        entries={[{ key: "login", kind: "login", label: "Login", onSelect: props.onCreateLogin }]}
      />
    ),
    menu: () => (
      <Menu
        label="Menu"
        onClose={() => setOverlay(null)}
        style={{ right: 12, top: 52, width: 260 }}
        entries={[
          { key: "account", icon: "user", label: props.email, inert: true },
          "-",
          {
            key: "settings",
            icon: "settings",
            label: "Settings",
            onSelect: () => void chrome.runtime.openOptionsPage(),
          },
          {
            key: "sync",
            icon: "refresh",
            label: "Sync now",
            onSelect: () =>
              void ask({ type: "sync" })
                .then(() => load())
                .then(() => props.onToast("Synced"))
                .catch(() => props.onToast("Couldn't sync. Check your connection.", "danger")),
          },
          "-",
          { key: "lock", icon: "lock", label: "Lock", onSelect: props.onLock },
          {
            key: "sign-out",
            icon: "logout",
            label: "Sign out",
            danger: true,
            onSelect: props.onSignOut,
          },
        ]}
      />
    ),
    vaults: () => (
      <Menu
        label="Vaults"
        onClose={() => setOverlay(null)}
        style={{ left: 50, top: 52, width: 220 }}
        entries={[
          {
            key: "all",
            icon: "grid",
            label: "All vaults",
            checked: props.vaultFilter === null,
            onSelect: () => props.onVaultFilter(null),
          },
          ...vaults.map(
            (vault): MenuEntry => ({
              key: vault.id,
              icon: vaultIcon(vault),
              label: vault.name,
              checked: props.vaultFilter === vault.id,
              onSelect: () => props.onVaultFilter(vault.id),
            }),
          ),
        ]}
      />
    ),
  };

  const searching = query.trim() !== "";
  return (
    <Pop overlay={overlay ? menus[overlay]() : undefined}>
      <Head>
        <K.Logo size={26} />
        <K.VaultChip
          name={activeVault?.name ?? "All vaults"}
          icon={activeVault ? vaultIcon(activeVault) : "grid"}
          tone={(activeVault?.colour as "brass") ?? undefined}
          dropdown
          aria-haspopup="menu"
          aria-expanded={overlay === "vaults"}
          onClick={() => setOverlay("vaults")}
        />
        <div className="kt-grow" />
        <K.IconButton icon="lock" label="Lock now" size="sm" onClick={props.onLock} />
        <K.IconButton
          icon="more"
          label="Menu"
          size="sm"
          aria-haspopup="menu"
          aria-expanded={overlay === "menu"}
          onClick={() => setOverlay("menu")}
        />
      </Head>
      <Body>
        <K.SearchBar
          placeholder={activeVault ? `Search in ${activeVault.name}` : "Search in all vaults"}
          value={query}
          autoFocus
          onChange={(event) => setQuery(event.target.value)}
        />
        <div className="kx-scroll">
          {searching ? (
            <>
              <Section>{`Results · ${results.length}`}</Section>
              {results.length > 0 ? (
                <List label="Results">
                  {/* This site's items stay on top. */}
                  {[
                    ...results.filter((item) => suggestedIds.has(item.id)),
                    ...results.filter((item) => !suggestedIds.has(item.id)),
                  ].map((item) => row(item, false))}
                </List>
              ) : (
                <Empty icon="search" title={`Nothing matches “${query.trim()}”`} />
              )}
            </>
          ) : (
            <>
              {host && <Section icon="globe">{`Suggestions for ${host}`}</Section>}
              {host &&
                loaded &&
                (suggested.length > 0 ? (
                  <List label={`Suggestions for ${host}`}>
                    {suggested.map((hit) => row(hit.item, false, hit.insecure, true))}
                  </List>
                ) : (
                  <Empty icon="key" title={`No logins for ${host}`}>
                    <K.Button variant="primary" icon="plus" size="sm" onClick={props.onCreateLogin}>
                      {`Create login for ${host}`}
                    </K.Button>
                  </Empty>
                ))}
              <Section>All items</Section>
              {loaded && results.length === 0 ? (
                <Empty
                  icon="vault"
                  title="Nothing here yet"
                  body="Logins you save in Kandado on any device show up here."
                />
              ) : (
                <List label="All items">{results.map((item) => row(item, true))}</List>
              )}
            </>
          )}
        </div>
      </Body>
      <Foot>
        <K.Button
          variant="primary"
          icon="plus"
          block
          aria-haspopup="menu"
          onClick={() => setOverlay("create")}
        >
          Create
        </K.Button>
        <K.Button icon="key" block onClick={props.onGenerate}>
          Generate
        </K.Button>
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
  return code ? <K.TotpCode code={code} /> : null;
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
    <Group>
      <div className="kt-notebox">
        <div className="kd-secret-label">Note</div>
        <p>{notes}</p>
      </div>
    </Group>
  );
}

function CustomFields({ content }: { content: ItemContentJson }) {
  if (content.custom_fields.length === 0) return null;
  return (
    <>
      <div className="kt-label">
        <span>Custom fields</span>
      </div>
      <Group>
        {content.custom_fields.map((field, index) => (
          <K.SecretField
            // biome-ignore lint/suspicious/noArrayIndexKey: the list is fixed while the screen is open; labels can repeat
            key={`${field.label}-${index}`}
            label={field.label || "Field"}
            value={field.value}
            concealed={field.hidden}
          />
        ))}
      </Group>
    </>
  );
}

function DetailBody({ item, content }: { item: Summary; content: ItemContentJson }) {
  const d = content.data;
  switch (content.kind) {
    case "login": {
      const websites = Array.isArray(d.websites) ? (d.websites as { url: string }[]) : [];
      return (
        <>
          <Group>
            {text(d.username) && (
              <K.SecretField
                label="Username"
                value={text(d.username)}
                icon="user"
                concealed={false}
              />
            )}
            <K.SecretField label="Password" value={text(d.password)} icon="key" />
            {item.has_totp && <Totp id={item.id} />}
          </Group>
          {websites.length > 0 && (
            <Group>
              {websites.map((site, index) => (
                <K.SecretField
                  // biome-ignore lint/suspicious/noArrayIndexKey: the list is fixed while the screen is open; addresses can repeat
                  key={`${site.url}-${index}`}
                  label="Website"
                  value={site.url}
                  icon="globe"
                  concealed={false}
                  valueColor="var(--link)"
                />
              ))}
            </Group>
          )}
          <Notes notes={content.notes} />
          <CustomFields content={content} />
        </>
      );
    }
    case "card":
      return (
        <>
          <Group>
            {text(d.cardholder) && (
              <K.SecretField label="Cardholder" value={text(d.cardholder)} concealed={false} />
            )}
            <K.SecretField label="Card number" value={text(d.number)} mono />
            {expiry(d.expiry) && (
              <K.SecretField label="Expires" value={expiry(d.expiry)} concealed={false} />
            )}
            {text(d.cvv) && <K.SecretField label="Security code" value={text(d.cvv)} />}
            {text(d.pin) && <K.SecretField label="PIN" value={text(d.pin)} />}
          </Group>
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
          <Group>
            {rows
              .filter(([, value]) => value)
              .map(([label, value]) => (
                <K.SecretField key={label} label={label} value={value} concealed={false} />
              ))}
          </Group>
          <Notes notes={content.notes} />
          <CustomFields content={content} />
        </>
      );
    }
    case "password":
      return (
        <>
          <Group>
            <K.SecretField label="Password" value={text(d.password)} icon="key" mono />
          </Group>
          <Notes notes={content.notes} />
        </>
      );
    case "alias":
      return (
        <Group>
          <K.SecretField label="Alias" value={text(d.address)} icon="mail" concealed={false} />
        </Group>
      );
    case "passkey":
      return (
        <Group>
          <K.SecretField label="Website" value={text(d.rp_id)} icon="globe" concealed={false} />
          <K.SecretField label="Username" value={text(d.user_name)} icon="user" concealed={false} />
        </Group>
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
  const [failed, setFailed] = useState(false);
  useEffect(() => {
    let current = true;
    ask({ type: "item", id: item.id })
      .then((value) => {
        if (current) setContent(value);
      })
      .catch(() => setFailed(true));
    return () => {
      current = false;
      setContent(null);
    };
  }, [item.id]);
  return (
    <Pop>
      <BackHead
        title={item.title}
        onBack={onBack}
        right={
          item.kind === "login" && content ? (
            <K.IconButton icon="note" size="sm" label="Edit" onClick={() => onEdit(content)} />
          ) : undefined
        }
      />
      <Body>
        <div className="kx-scroll">
          {failed && <p className="kx-error">This item could not be opened.</p>}
          {content && <DetailBody item={item} content={content} />}
        </div>
      </Body>
      {onFill && (item.kind === "login" || item.kind === "card" || item.kind === "identity") && (
        <Foot>
          <K.Button variant="primary" icon="bolt" block onClick={onFill}>
            {item.kind === "card" ? "Fill card" : "Fill on this page"}
          </K.Button>
        </Foot>
      )}
    </Pop>
  );
}

// ---- E-10 (and editing a login from E-06) ----------------------------------------

export function LoginForm({
  tabUrl,
  editing,
  onDone,
  onCancel,
}: {
  tabUrl: string | undefined;
  editing?: { item: Summary; content: ItemContentJson };
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
  const [password, setPassword] = useState(text(start?.password));
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

  useEffect(() => {
    void ask({ type: "vaults" }).then((list) => {
      setVaults(list);
      setVault((current) => current || list[0]?.id || "");
    });
    if (!editing) void generate();
  }, [editing, generate]);

  const typed = async (value: string) => {
    setPassword(value);
    setStrength(value ? { score: await ask({ type: "strength", password: value }) } : null);
  };

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
      <BackHead title={editing ? "Edit login" : "New login"} onBack={onCancel} />
      <Body>
        <form
          className="kx-scroll"
          onSubmit={(event) => {
            event.preventDefault();
            void save();
          }}
        >
          <K.TextField label="Title" value={title} onChange={(e) => setTitle(e.target.value)} />
          <K.TextField
            label="Username or email"
            icon="user"
            placeholder="name@example.com"
            autoComplete="off"
            value={username}
            onChange={(e) => setUsername(e.target.value)}
          />
          <K.TextField
            label="Password"
            icon="key"
            mono
            autoComplete="off"
            spellCheck={false}
            value={password}
            onChange={(e) => void typed(e.target.value)}
            trailing={
              <K.IconButton
                icon="refresh"
                size="sm"
                label="Generate"
                onClick={() => void generate()}
              />
            }
          />
          {strength && (
            <K.StrengthMeter
              score={Math.max(0, Math.min(4, strength.score)) as 0 | 1 | 2 | 3 | 4}
              detail={strength.bits ? `${Math.round(strength.bits)} bits` : undefined}
            />
          )}
          <K.TextField
            label="Website"
            icon="globe"
            placeholder="https://"
            value={website}
            onChange={(e) => setWebsite(e.target.value)}
          />
          {vaults.length > 1 && !editing && (
            <SelectField
              label="Vault"
              icon="vault"
              value={vault}
              options={vaults.map((v) => ({ value: v.id, label: v.name }))}
              onChange={setVault}
            />
          )}
          {error && (
            <p className="kx-error" role="alert">
              {error}
            </p>
          )}
          <button type="submit" hidden aria-hidden="true" tabIndex={-1} />
        </form>
      </Body>
      <Foot>
        <K.Button block onClick={onCancel}>
          Cancel
        </K.Button>
        <K.Button variant="primary" block disabled={busy || !password} onClick={() => void save()}>
          Save
        </K.Button>
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

function colourise(value: string) {
  return Array.from(value).map((ch, index) => {
    const cls = /[0-9]/.test(ch) ? "kd-ch-num" : /[A-Za-z ]/.test(ch) ? null : "kd-ch-sym";
    return cls ? (
      // biome-ignore lint/suspicious/noArrayIndexKey: characters repeat; position is the identity
      <span key={index} className={cls}>
        {ch}
      </span>
    ) : (
      ch
    );
  });
}

/**
 * The design system's PasswordGenerator layout (`kd-gen`), with the password
 * made by the shared Rust generator, so every client generates the same way.
 */
export function Generator({
  onBack,
  onToast,
}: {
  onBack: () => void;
  onToast: (text: string) => void;
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
    await navigator.clipboard.writeText(result.password);
    onToast("Password copied");
  };

  return (
    <Pop>
      <BackHead title="Generate password" onBack={onBack} />
      <Body>
        <div className="kd-gen">
          <div className="kd-gen-out">
            <div className="kd-gen-value kd-mono" aria-live="polite">
              {result ? colourise(result.password) : ""}
            </div>
            <div className="kd-gen-actions">
              <K.IconButton
                icon="refresh"
                label="Generate again"
                onClick={() => void run(options)}
              />
              <K.IconButton icon="copy" label="Copy password" onClick={() => void copy()} />
            </div>
          </div>
          {result && (
            <K.StrengthMeter
              score={Math.max(0, Math.min(4, result.score)) as 0 | 1 | 2 | 3 | 4}
              detail={`${Math.round(result.bits)} bits`}
            />
          )}
          <div className="kd-seg" role="tablist">
            {(["random", "memorable"] as const).map((mode) => (
              <button
                key={mode}
                type="button"
                role="tab"
                aria-selected={options.mode === mode}
                className={`kd-seg-btn${options.mode === mode ? " kd-seg-on" : ""}`}
                onClick={() => setMode(mode)}
              >
                {mode === "random" ? "Random" : "Memorable"}
              </button>
            ))}
          </div>
          <label className="kd-range">
            <span className="kd-range-top">
              <span>{memorable ? "Words" : "Characters"}</span>
              <span className="kd-range-val">{size}</span>
            </span>
            <input
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
          </label>
          <K.Toggle
            label={memorable ? "Capitalise" : "Uppercase letters"}
            checked={options.uppercase}
            onChange={(value) => set({ uppercase: value })}
          />
          <K.Toggle
            label="Numbers"
            checked={options.numbers}
            onChange={(value) => set({ numbers: value })}
          />
          <K.Toggle
            label={memorable ? "Hyphens between words" : "Symbols"}
            checked={options.symbols}
            onChange={(value) => set({ symbols: value })}
          />
        </div>
      </Body>
    </Pop>
  );
}
