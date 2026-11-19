// The in-page dropdown, E-16 to E-22 (E-19 arrives with aliases in P9). It is an
// extension page inside the site's page, so the site cannot read it. It asks the
// worker what to show for its token, and tells it what the person picked; the
// worker does the fill.

import { type ReactNode, useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import type { DropdownChoice, DropdownRow, DropdownView } from "../../background/autofill";
import type { WorkerMessage } from "../../shared/messages";
import { grouped, ItemRow, Kbd, LcdStrip, Tag, Well } from "../../ui/hardware";
import { ask } from "../../ui/messaging";

interface Entry {
  key: string;
  choice: DropdownChoice | "close";
  render: (on: boolean, hover: () => void) => ReactNode;
}

/** A row that does something (search, unlock, use a password), drawn like an item row. */
function Action({
  label,
  detail,
  mono,
  on,
  onPick,
  onHover,
}: {
  label: string;
  detail?: string;
  /** The detail is a value (a password), set in mono. */
  mono?: boolean;
  on: boolean;
  onPick: () => void;
  onHover: () => void;
}) {
  return (
    <button
      type="button"
      className={`hw-item kx-item kx-dd-action${on ? " hw-item--on" : ""}`}
      onClick={onPick}
      onMouseEnter={onHover}
    >
      <span className="kx-item-text">
        <span className="hw-item-title">{label}</span>
        {detail && <span className={`hw-item-sub${mono ? " kx-dd-mono" : ""}`}>{detail}</span>}
      </span>
      {on && <Kbd>Enter</Kbd>}
    </button>
  );
}

export function Dropdown({ token }: { token: string }) {
  const [view, setView] = useState<DropdownView | null>(null);
  const [active, setActive] = useState(0);
  const [confirm, setConfirm] = useState<DropdownRow | null>(null);
  const root = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    void ask({ type: "dropdown-view", token }).then(setView, () => setView({ mode: "closed" }));
  }, [token]);

  // A two-factor code counts down, and a new one is fetched when its time is up.
  useEffect(() => {
    if (view?.mode !== "totp") return;
    const timer = window.setInterval(() => {
      const left = 30 - (Math.floor(Date.now() / 1000) % 30);
      if (left === 30) {
        void ask({ type: "dropdown-view", token }).then(setView, () => {});
      } else {
        setView((current) =>
          current?.mode === "totp"
            ? { ...current, codes: current.codes.map((code) => ({ ...code, remaining: left })) }
            : current,
        );
      }
    }, 1000);
    return () => window.clearInterval(timer);
  }, [view?.mode, token]);

  // Tell the page how tall to make the frame.
  useLayoutEffect(() => {
    const element = root.current;
    if (!element) return;
    const report = () =>
      void ask({
        type: "dropdown-size",
        token,
        height: document.body.getBoundingClientRect().height,
      }).catch(() => {});
    report();
    const observer = new ResizeObserver(report);
    observer.observe(element);
    return () => observer.disconnect();
  }, [token]);

  const close = useCallback(
    () => void ask({ type: "dropdown-close", token }).catch(() => {}),
    [token],
  );

  const pick = useCallback(
    async (choice: DropdownChoice | "close") => {
      if (choice === "close") {
        close();
        return;
      }
      const result = await ask({ type: "dropdown-pick", token, choice }).catch(
        () => "refused" as const,
      );
      if (result === "confirm-insecure" && "item" in choice) {
        const row =
          view && "rows" in view
            ? view.rows.find((candidate) => candidate.id === choice.item)
            : undefined;
        setConfirm(row ?? null);
        setActive(0);
      } else if (result === "opened-popup") {
        close();
      }
      // "filled": the page removes the dropdown.
    },
    [token, view, close],
  );

  const entries = entriesFor(view, confirm, pick);

  // Keys typed in the page's field arrive through the worker.
  useEffect(() => {
    const onMessage = (message: WorkerMessage) => {
      if (message.type !== "kandado-dropdown-key" || message.token !== token) return;
      if (message.key === "escape") close();
      else if (message.key === "down")
        setActive((index) => Math.min(entries.length - 1, index + 1));
      else if (message.key === "up") setActive((index) => Math.max(0, index - 1));
      else if (message.key === "enter") {
        const entry = entries[active];
        if (entry) void pick(entry.choice);
      }
    };
    chrome.runtime.onMessage.addListener(onMessage);
    return () => chrome.runtime.onMessage.removeListener(onMessage);
  }, [token, entries, active, pick, close]);

  if (!view || view.mode === "closed") return <div ref={root} />;
  return (
    <div ref={root}>
      <div className="kx-dropdown">
        <LcdStrip light={view.mode === "locked" ? "warn" : "ok"} text={headOf(view)} />
        <Well className="kx-dropdown-well" role="listbox" aria-label="Kandado">
          {entries.map((entry, index) => (
            <div key={entry.key}>{entry.render(index === active, () => setActive(index))}</div>
          ))}
        </Well>
      </div>
    </div>
  );
}

function headOf(view: Exclude<DropdownView, { mode: "closed" }>): string {
  switch (view.mode) {
    case "locked":
      return "Kandado is locked";
    case "new-password":
      return "Kandado · new password";
    case "cards":
      return "Kandado · cards";
    case "identities":
      return "Kandado · identities";
    default:
      return `Kandado · ${view.host}`;
  }
}

function entriesFor(
  view: DropdownView | null,
  confirm: DropdownRow | null,
  pick: (choice: DropdownChoice | "close") => void,
): Entry[] {
  if (!view) return [];
  const act = (
    key: string,
    choice: DropdownChoice | "close",
    label: string,
    detail?: string,
    mono?: boolean,
  ): Entry => ({
    key,
    choice,
    render: (on, hover) => (
      <Action
        label={label}
        mono={mono}
        detail={detail}
        on={on}
        onPick={() => pick(choice)}
        onHover={hover}
      />
    ),
  });
  const itemRow = (row: DropdownRow): Entry => ({
    key: row.id,
    choice: { item: row.id },
    render: (on, hover) => (
      <ItemRow
        onMouseEnter={hover}
        title={row.favourite ? `${row.title} ★` : row.title}
        subtitle={row.subtitle || undefined}
        selected={on}
        end={row.insecure ? <Tag tone="warn">Not secure</Tag> : on ? <Kbd>Enter</Kbd> : undefined}
        onClick={() => pick({ item: row.id })}
      />
    ),
  });

  if (confirm) {
    return [
      act(
        "confirm",
        { item: confirm.id, confirmInsecure: true },
        "Fill anyway",
        `${confirm.title} was saved for a secure page. This page is not encrypted.`,
      ),
      act("cancel", "close", "Don't fill"),
    ];
  }
  switch (view.mode) {
    case "closed":
      return [];
    case "locked":
      return [act("unlock", { search: true }, "Unlock Kandado to fill", `Logins for ${view.host}`)];
    case "logins":
      return [...view.rows.map(itemRow), act("search", { search: true }, "Search all items")];
    case "new-password":
      return [
        act("use", { generated: true }, "Use a strong password", view.password, true),
        act("more", { search: true }, "More options…", "Opens the generator"),
      ];
    case "cards":
    case "identities":
      return view.rows.map(itemRow);
    case "totp":
      return view.codes.map((entry) =>
        act(
          entry.id,
          { item: entry.id },
          `Fill ${grouped(entry.code)}`,
          `${entry.title} · ${entry.remaining} s left`,
        ),
      );
  }
}
