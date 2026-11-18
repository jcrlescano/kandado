// E-28 to E-31, the settings tab (popup ⋯ › Settings, or the browser's extension
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
import { K } from "../../ui/kandado";
import { AskError, ask } from "../../ui/messaging";
import { OptPage, PAGES, PinSetup, Row, Rows, type SettingsPage } from "../parts";

function pageFromHash(): SettingsPage {
  const id = location.hash.slice(1);
  return PAGES.some((page) => page.id === id) ? (id as SettingsPage) : "general";
}

function minutesLabel(minutes: number): string {
  if (minutes < 60) return minutes === 1 ? "1 minute" : `${minutes} minutes`;
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
  const themes: { id: ThemeChoice; label: string }[] = [
    { id: "system", label: "System" },
    { id: "dark", label: "Dark" },
    { id: "light", label: "Light" },
  ];
  return (
    <>
      {/* Pressed / not pressed buttons: the design draws a segmented control. */}
      <div className="kt-label" aria-hidden="true">
        <span>Theme</span>
      </div>
      <fieldset className="kx-fieldset">
        <legend className="kx-hidden-input">
          <span>Theme</span>
        </legend>
        <div className="kd-seg">
          {themes.map((theme) => (
            <button
              key={theme.id}
              type="button"
              aria-pressed={settings.theme === theme.id}
              className={`kd-seg-btn${settings.theme === theme.id ? " kd-seg-on" : ""}`}
              onClick={() => void change({ theme: theme.id })}
            >
              {theme.label}
            </button>
          ))}
        </div>
      </fieldset>
      <Rows>
        <Row
          icon="bolt"
          label="Show item count on the toolbar icon"
          toggle={settings.toolbarCount}
          onToggle={(on) => void change({ toolbarCount: on })}
        />
        <Row
          icon="key"
          label="Open popup shortcut"
          value={shortcut ? shortcut.replace(/\+/g, " ") : "Not set"}
          onOpen={() => void chrome.tabs.create({ url: "chrome://extensions/shortcuts" })}
        />
      </Rows>
    </>
  );
}

// ---- E-29 ------------------------------------------------------------------------

function Autofill({ settings, change }: PageProps) {
  const [access, setAccess] = useState<boolean | null>(null);
  useEffect(() => {
    void hasSiteAccess().then(setAccess);
  }, []);
  return (
    <>
      <Rows>
        <Row
          icon="globe"
          label="Fill on websites"
          description={
            access
              ? "Kandado can read the pages you open, to find forms."
              : "Kandado needs access to the pages you open to show logins in their forms."
          }
          toggle={access === true}
          onToggle={(on) =>
            void (on ? requestSiteAccess() : removeSiteAccess().then(() => false)).then(setAccess)
          }
        />
      </Rows>
      <Rows>
        <Row
          icon="key"
          label="Show Kandado in form fields"
          toggle={settings.showInFields}
          onToggle={(on) => void change({ showInFields: on })}
        />
        <Row
          icon="refresh"
          label="Suggest passwords in new-password fields"
          toggle={settings.suggestPasswords}
          disabled={!settings.showInFields}
          onToggle={(on) => void change({ suggestPasswords: on })}
        />
        <Row
          icon="plus"
          label="Offer to save and update logins"
          toggle={settings.offerToSave}
          onToggle={(on) => void change({ offerToSave: on })}
        />
      </Rows>
    </>
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
    <>
      <form
        onSubmit={(event) => {
          event.preventDefault();
          void add();
        }}
      >
        <K.TextField
          label="Add a site"
          icon="globe"
          placeholder="example.com"
          value={site}
          error={error ?? undefined}
          onChange={(event) => {
            setSite(event.target.value);
            setError(null);
          }}
          trailing={
            <K.Button size="sm" type="submit">
              Add
            </K.Button>
          }
        />
      </form>
      {settings.excludedSites.length === 0 ? (
        <p className="kt-p">
          No excluded sites. Sites you add here, or answer “Never for this site”, get no dropdown
          and no save prompt.
        </p>
      ) : (
        <Rows>
          {settings.excludedSites.map((host) => (
            <Row
              key={host}
              icon="globe"
              label={host}
              description="Never fill · never save"
              action={
                <K.IconButton
                  icon="x"
                  size="sm"
                  label={`Remove ${host}`}
                  onClick={() =>
                    void change({
                      excludedSites: settings.excludedSites.filter((entry) => entry !== host),
                    })
                  }
                />
              }
            />
          ))}
        </Rows>
      )}
    </>
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
      <div className="kx-panel">
        <PinSetup
          onCancel={() => setSettingPin(false)}
          onDone={async (pin) => {
            onStatus(await ask({ type: "set-pin", pin }));
            await change({ pinWanted: true });
            setSettingPin(false);
          }}
        />
      </div>
    );
  }
  return (
    <>
      <DesktopLinkRow />
      <Rows>
        <Row
          icon="key"
          label="Extension PIN"
          description={
            pinOn
              ? "Unlocks Kandado until the browser closes."
              : unlocked
                ? "Six digits to unlock, after the master password once per browser session."
                : "Unlock Kandado to set a PIN."
          }
          value={pinOn ? "On" : "Off"}
          disabled={!unlocked && !pinOn}
          onOpen={pinOn ? undefined : unlocked ? () => setSettingPin(true) : undefined}
          action={
            pinOn ? (
              <K.Button
                size="sm"
                variant="ghost"
                onClick={() =>
                  void ask({ type: "clear-pin" }).then(async (next) => {
                    onStatus(next);
                    await change({ pinWanted: false });
                  })
                }
              >
                Turn off
              </K.Button>
            ) : undefined
          }
        />
        <Row
          icon="clock"
          label="Lock after"
          action={
            <select
              className="kx-inline-select"
              aria-label="Lock after"
              value={String(settings.autoLockMinutes)}
              onChange={(event) => void change({ autoLockMinutes: Number(event.target.value) })}
            >
              {AUTO_LOCK_CHOICES.map((minutes) => (
                <option key={minutes} value={String(minutes)}>
                  {minutesLabel(minutes)}
                </option>
              ))}
            </select>
          }
        />
        <Row
          icon="lock"
          label="Lock when the browser closes"
          description="Always on: the keys are kept only in memory."
          toggle
          disabled
        />
      </Rows>
    </>
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
                ? "In the desktop app, turn on Settings \u203a Autofill and browser \u203a Unlock the browser extension with this app."
                : code === "locked"
                  ? "Unlock Kandado here first."
                  : "Pairing did not start. Try again.",
      );
    }
  };

  const description = !status
    ? "Looking for the Kandado desktop app…"
    : status.pairingCode
      ? `Check that Kandado on this computer shows ${status.pairingCode.slice(0, 3)} ${status.pairingCode.slice(3)}, and accept there.`
      : status.paired
        ? "This browser unlocks and locks with the desktop app."
        : status.available
          ? "Unlock this extension when the desktop app unlocks, without your master password."
          : "Needs the Kandado desktop app on this computer.";
  return (
    <>
      <Rows>
        <Row
          icon="grid"
          label="Lock and unlock with the desktop app"
          description={description}
          toggle={status?.paired === true || status?.pairingCode != null}
          disabled={!status || (!status.available && !status.paired)}
          onToggle={(on) => void toggle(on)}
        />
      </Rows>
      {failure && (
        <p className="kx-error" role="alert">
          {failure}
        </p>
      )}
    </>
  );
}
