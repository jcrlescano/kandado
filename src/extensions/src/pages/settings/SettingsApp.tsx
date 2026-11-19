// E-28 to E-31, the settings tab (popup Menu › Settings, or the browser's extension
// options). The page is chosen by the address hash, so each one can be linked.

import { useCallback, useEffect, useState } from "react";
import type { DesktopLinkStatus, Status } from "../../shared/messages";
import {
  AUTO_LOCK_CHOICES,
  type ExtensionSettings,
  normaliseSite,
  type ThemeChoice,
} from "../../shared/settings";
import { hasSiteAccess, removeSiteAccess, requestSiteAccess } from "../../ui/access";
import {
  grouped,
  Kbd,
  Key,
  Raised,
  Segmented,
  StatusLight,
  Switch,
  Tag,
  TextField,
  Well,
} from "../../ui/hardware";
import { AskError, ask } from "../../ui/messaging";
import { OptPage, PAGES, PinSetup, Row, Rows, type SettingsPage } from "../parts";

function pageFromHash(): SettingsPage {
  const id = location.hash.slice(1);
  return PAGES.some((page) => page.id === id) ? (id as SettingsPage) : "general";
}

/** "5 min", "1 hour": short, for the Lock after keys. */
function minutesLabel(minutes: number): string {
  if (minutes < 60) return `${minutes} min`;
  const hours = minutes / 60;
  return hours === 1 ? "1 hour" : `${hours} hours`;
}

export function SettingsApp() {
  const [page, setPage] = useState<SettingsPage>(pageFromHash);
  const [settings, setSettings] = useState<ExtensionSettings | null>(null);
  const [status, setStatus] = useState<Status | null>(null);

  useEffect(() => {
    void ask({ type: "settings-get" }).then(setSettings);
    void ask({ type: "status" }).then(setStatus);
    const onHash = () => setPage(pageFromHash());
    window.addEventListener("hashchange", onHash);
    return () => window.removeEventListener("hashchange", onHash);
  }, []);

  const change = useCallback(async (patch: Partial<ExtensionSettings>) => {
    setSettings(await ask({ type: "settings-set", patch }));
  }, []);

  const navigate = (next: SettingsPage) => {
    location.hash = next;
    setPage(next);
  };

  return (
    <OptPage active={page} onNavigate={navigate}>
      {settings && page === "general" && <General settings={settings} change={change} />}
      {settings && page === "autofill" && <Autofill settings={settings} change={change} />}
      {settings && page === "excluded" && <Excluded settings={settings} change={change} />}
      {settings && status && page === "security" && (
        <Security settings={settings} change={change} status={status} onStatus={setStatus} />
      )}
    </OptPage>
  );
}

interface PageProps {
  settings: ExtensionSettings;
  change: (patch: Partial<ExtensionSettings>) => Promise<void>;
}

// ---- E-28 ------------------------------------------------------------------------

function General({ settings, change }: PageProps) {
  const [shortcut, setShortcut] = useState<string>("");
  useEffect(() => {
    void chrome.commands
      .getAll()
      .then((commands) =>
        setShortcut(commands.find((command) => command.name === "_execute_action")?.shortcut ?? ""),
      )
      .catch(() => {});
  }, []);
  const themes: { value: ThemeChoice; label: string }[] = [
    { value: "system", label: "System" },
    { value: "light", label: "Light" },
    { value: "dark", label: "Dark" },
  ];
  return (
    <Rows>
      <Row label="Theme" strong>
        <Segmented
          label="Theme"
          onPanel
          value={settings.theme}
          options={themes}
          onChange={(theme) => void change({ theme })}
        />
      </Row>
      <Row label="Open popup shortcut">
        <span className="kx-kbds">
          {shortcut ? (
            shortcut.split("+").map((key) => <Kbd key={key}>{key}</Kbd>)
          ) : (
            <Tag>Not set</Tag>
          )}
        </span>
        <Key
          size="sm"
          onClick={() => void chrome.tabs.create({ url: "chrome://extensions/shortcuts" })}
        >
          Change
        </Key>
      </Row>
      <Row label="Show item count on the toolbar icon">
        <Switch
          label="Show item count on the toolbar icon"
          checked={settings.toolbarCount}
          onChange={(on) => void change({ toolbarCount: on })}
        />
      </Row>
    </Rows>
  );
}

// ---- E-29 ------------------------------------------------------------------------

function Autofill({ settings, change }: PageProps) {
  const [access, setAccess] = useState<boolean | null>(null);
  useEffect(() => {
    void hasSiteAccess().then(setAccess);
  }, []);
  return (
    <Rows>
      <Row
        label="Fill on websites"
        description={
          access
            ? "Kandado can read the pages you open, to find forms."
            : "Kandado needs access to the pages you open to show logins in their forms."
        }
      >
        <Switch
          label="Fill on websites"
          checked={access === true}
          onChange={(on) =>
            void (on ? requestSiteAccess() : removeSiteAccess().then(() => false)).then(setAccess)
          }
        />
      </Row>
      <Row label="Show Kandado in form fields">
        <Switch
          label="Show Kandado in form fields"
          checked={settings.showInFields}
          onChange={(on) => void change({ showInFields: on })}
        />
      </Row>
      <Row label="Offer to save and update logins">
        <Switch
          label="Offer to save and update logins"
          checked={settings.offerToSave}
          onChange={(on) => void change({ offerToSave: on })}
        />
      </Row>
      <Row
        label="Suggest passwords in new-password fields"
        description={settings.showInFields ? undefined : "Needs Kandado in form fields."}
      >
        <Switch
          label="Suggest passwords in new-password fields"
          checked={settings.suggestPasswords}
          disabled={!settings.showInFields}
          onChange={(on) => void change({ suggestPasswords: on })}
        />
      </Row>
    </Rows>
  );
}

// ---- E-30 ------------------------------------------------------------------------

function Excluded({ settings, change }: PageProps) {
  const [site, setSite] = useState("");
  const [error, setError] = useState<string | null>(null);
  const add = async () => {
    const host = normaliseSite(site);
    if (!host) {
      setError("Enter a site, such as example.com.");
      return;
    }
    if (!settings.excludedSites.includes(host)) {
      await change({ excludedSites: [...settings.excludedSites, host] });
    }
    setSite("");
    setError(null);
  };
  return (
    <Well className="kx-sites">
      {settings.excludedSites.length === 0 ? (
        <p className="hw-sub kx-sites-empty">
          No excluded sites. Sites you add here, or answer “Never for this site”, get no dropdown
          and no save prompt.
        </p>
      ) : (
        settings.excludedSites.map((host) => (
          <Raised key={host} className="kx-site">
            <StatusLight tone="off" />
            <span className="hw-mono kx-site-host">{host}</span>
            <span className="hw-label">Never fill · never save</span>
            <Key
              size="sm"
              icon
              tone="flat"
              aria-label={`Remove ${host}`}
              onClick={() =>
                void change({
                  excludedSites: settings.excludedSites.filter((entry) => entry !== host),
                })
              }
            >
              ×
            </Key>
          </Raised>
        ))
      )}
      <form
        className="kx-add-site"
        onSubmit={(event) => {
          event.preventDefault();
          void add();
        }}
      >
        <TextField
          label="Add a site"
          placeholder="example.com"
          value={site}
          error={error}
          onChange={(event) => {
            setSite(event.target.value);
            setError(null);
          }}
        />
        <Key type="submit">Add</Key>
      </form>
    </Well>
  );
}

// ---- E-31 ------------------------------------------------------------------------

function Security({
  settings,
  change,
  status,
  onStatus,
}: PageProps & { status: Status; onStatus: (status: Status) => void }) {
  const [settingPin, setSettingPin] = useState(false);
  const unlocked = status.state === "unlocked";
  const pinOn = status.state !== "signed_out" && status.pin;

  if (settingPin) {
    return (
      <Well>
        <PinSetup
          onCancel={() => setSettingPin(false)}
          onDone={async (pin) => {
            onStatus(await ask({ type: "set-pin", pin }));
            await change({ pinWanted: true });
            setSettingPin(false);
          }}
        />
      </Well>
    );
  }
  return (
    <Rows>
      <DesktopLinkRow />
      <Row
        label="Extension PIN"
        description={
          pinOn
            ? "Unlocks Kandado until the browser closes."
            : unlocked
              ? "Six digits to unlock, after the master password once per browser session."
              : "Unlock Kandado to set a PIN."
        }
      >
        {pinOn ? <Tag tone="go">On</Tag> : <Tag>Not set</Tag>}
        {pinOn ? (
          <Key
            size="sm"
            onClick={() =>
              void ask({ type: "clear-pin" }).then(async (next) => {
                onStatus(next);
                await change({ pinWanted: false });
              })
            }
          >
            Turn off
          </Key>
        ) : (
          <Key size="sm" disabled={!unlocked} onClick={() => setSettingPin(true)}>
            Set PIN
          </Key>
        )}
      </Row>
      <Row label="Lock after">
        <Segmented
          label="Lock after"
          onPanel
          value={String(settings.autoLockMinutes)}
          options={AUTO_LOCK_CHOICES.map((minutes) => ({
            value: String(minutes),
            label: minutesLabel(minutes),
          }))}
          onChange={(minutes) => void change({ autoLockMinutes: Number(minutes) })}
        />
      </Row>
      <Row
        label="Lock when the browser closes"
        description="Always on: the keys are kept only in memory."
      >
        <Switch label="Lock when the browser closes" checked disabled />
      </Row>
    </Rows>
  );
}

/**
 * E-31 "Lock and unlock with the desktop app": pairs this extension with the
 * Kandado desktop app on this computer. The person compares six digits here
 * with the ones the app shows (D-45) and accepts there.
 */
function DesktopLinkRow() {
  const [status, setStatus] = useState<DesktopLinkStatus | null>(null);
  const [failure, setFailure] = useState<string | null>(null);

  useEffect(() => {
    let stopped = false;
    const poll = async () => {
      const next = await ask({ type: "desktop-status" }).catch(() => null);
      if (!stopped) setStatus(next);
    };
    void poll();
    const timer = window.setInterval(() => void poll(), 1000);
    return () => {
      stopped = true;
      window.clearInterval(timer);
    };
  }, []);

  const toggle = async (on: boolean) => {
    setFailure(null);
    try {
      if (on) {
        const { code } = await ask({ type: "desktop-pair" });
        setStatus((current) => (current ? { ...current, pairingCode: code } : current));
      } else {
        await ask({ type: "desktop-unpair" });
        setStatus((current) =>
          current ? { ...current, paired: false, pairingCode: null } : current,
        );
      }
    } catch (caught) {
      const code = caught instanceof AskError ? caught.code : "failed";
      setFailure(
        code === "desktop_unavailable"
          ? "The Kandado desktop app isn't running on this computer."
          : code === "desktop_other_account"
            ? "The desktop app is signed in to another account."
            : code === "desktop_locked"
              ? "Unlock the Kandado desktop app first."
              : code === "desktop_off"
                ? "In the desktop app, turn on Settings › Autofill and browser › Unlock the browser extension with this app."
                : code === "locked"
                  ? "Unlock Kandado here first."
                  : "Pairing did not start. Try again.",
      );
    }
  };

  const description = !status
    ? "Looking for the Kandado desktop app…"
    : status.pairingCode
      ? "Check that Kandado on this computer shows the same code, and accept there."
      : status.paired
        ? "This browser unlocks and locks with the desktop app."
        : status.available
          ? "Unlock this extension when the desktop app unlocks, without your master password."
          : "Needs the Kandado desktop app on this computer.";
  return (
    <Row
      label="Lock and unlock with the desktop app"
      description={
        <>
          {description}
          {status?.pairingCode && (
            <span className="hw-lcd kx-lcd-strip kx-pairing">
              <StatusLight tone="warn" />
              <span className="hw-lcd-label">Pairing code</span>
              <span className="hw-lcd-digits kx-lcd-text">{grouped(status.pairingCode)}</span>
            </span>
          )}
          {failure && (
            <span className="hw-error" role="alert" style={{ display: "block" }}>
              {failure}
            </span>
          )}
        </>
      }
    >
      <Switch
        label="Lock and unlock with the desktop app"
        checked={status?.paired === true || status?.pairingCode != null}
        disabled={!status || (!status.available && !status.paired)}
        onChange={(on) => void toggle(on)}
      />
    </Row>
  );
}
