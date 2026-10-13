import { memo, useState } from "react";
import type { Action } from "./bindings/Action";
import type { SettingRow } from "./bindings/SettingRow";
import type { UpdateAsk } from "./bindings/UpdateAsk";
import type { UpdateStatus } from "./bindings/UpdateStatus";
import "./Settings.css";
import {
  commitValue,
  optionLabel,
  sections,
  setSetting,
  updateLine,
} from "./settingsModel";

/** The Settings page: the desktop app's Behavior rows by group, and the updater. */
export const Settings = memo(function Settings({
  rows,
  update,
  version,
  send,
  onUpdate,
}: {
  rows: SettingRow[] | null;
  update: UpdateStatus | null;
  version: string | null;
  send: (action: Action) => void;
  onUpdate: (ask: UpdateAsk) => void;
}) {
  return (
    <div className="settings" tabIndex={-1} data-focus>
      <h1>Settings</h1>
      {rows === null && <p className="settings-empty">Loading…</p>}
      {sections(rows ?? [], update !== null).map((section) => (
        <section
          key={section.group}
          aria-labelledby={`settings-${section.group}`}
        >
          <h2 id={`settings-${section.group}`}>{section.heading}</h2>
          {section.group === "updates" && update && (
            <UpdateRow status={update} version={version} onUpdate={onUpdate} />
          )}
          {section.rows.map((row) => (
            <div key={row.id} className="setting-row">
              <div className="setting-text">
                <span id={`${row.id}-name`} className="setting-name">
                  {row.name}
                </span>
                <span id={`${row.id}-about`} className="setting-about">
                  {row.description}
                </span>
              </div>
              <SettingControl row={row} send={send} />
            </div>
          ))}
        </section>
      ))}
    </div>
  );
});

/** The updater's status line and its one action. */
export function UpdateRow({
  status,
  version,
  onUpdate,
}: {
  status: UpdateStatus;
  version: string | null;
  onUpdate: (ask: UpdateAsk) => void;
}) {
  const { text, button, failed } = updateLine(status, version);
  return (
    <div className="setting-row update-row">
      <span
        className={failed ? "setting-name failed" : "setting-name"}
        role="status"
      >
        {text}
      </span>
      {button && (
        <button
          type="button"
          className={
            button.ask === "check" ? "update-button" : "update-button go"
          }
          disabled={button.disabled}
          onClick={() => onUpdate(button.ask)}
        >
          {button.label}
        </button>
      )}
    </div>
  );
}

/** The control for one row: a switch, a select, or a field that commits on blur and Enter. */
export function SettingControl({
  row,
  send,
}: {
  row: SettingRow;
  send: (action: Action) => void;
}) {
  const labels = {
    "aria-labelledby": `${row.id}-name`,
    "aria-describedby": `${row.id}-about`,
  };
  const value = row.value;
  switch (value.kind) {
    case "bool":
      return (
        <button
          type="button"
          role="switch"
          className="switch"
          aria-checked={value.value}
          {...labels}
          onClick={() => send(setSetting(row.id, { Bool: !value.value }))}
        >
          <span className="switch-knob" />
        </button>
      );
    case "choice":
      return (
        <select
          className="setting-select"
          value={value.value}
          {...labels}
          onChange={(event) =>
            send(setSetting(row.id, { Text: event.currentTarget.value }))
          }
        >
          {value.options.map((option) => (
            <option key={option} value={option}>
              {optionLabel(row.id, option)}
            </option>
          ))}
        </select>
      );
    case "secret":
      return (
        <SecretField
          set={value.set}
          labels={labels}
          onCommit={(draft) => {
            const input = commitValue(row, draft);
            if (input) send(setSetting(row.id, input));
          }}
        />
      );
    default:
      return (
        <Field
          kind={value.kind}
          value={String(value.value)}
          labels={labels}
          onCommit={(draft) => {
            const input = commitValue(row, draft);
            if (input) send(setSetting(row.id, input));
          }}
        />
      );
  }
}

/** A text or number field; it shows the server's value whenever no edit is open. */
function Field({
  kind,
  value,
  labels,
  onCommit,
}: {
  kind: "number" | "text";
  value: string;
  labels: Record<string, string>;
  onCommit: (draft: string) => void;
}) {
  const [draft, setDraft] = useState<string | null>(null);
  const commit = () => {
    if (draft !== null) onCommit(draft);
    setDraft(null);
  };
  return (
    <input
      className={`setting-field ${kind}`}
      type={kind === "number" ? "number" : "text"}
      step={kind === "number" ? 1 : undefined}
      autoComplete="off"
      spellCheck={false}
      value={draft ?? value}
      {...labels}
      onChange={(event) => setDraft(event.currentTarget.value)}
      onBlur={commit}
      onKeyDown={(event) => {
        if (event.key === "Enter") {
          event.preventDefault();
          commit();
        } else if (event.key === "Escape" && draft !== null) {
          // A second Escape leaves the field.
          event.preventDefault();
          setDraft(null);
        }
      }}
    />
  );
}

/** A password field: it shows only whether a password is saved and empties after each commit. */
function SecretField({
  set,
  labels,
  onCommit,
}: {
  set: boolean;
  labels: Record<string, string>;
  onCommit: (draft: string) => void;
}) {
  // Uncontrolled, so the typed password never lands in a value attribute.
  const commit = (input: HTMLInputElement) => {
    onCommit(input.value);
    input.value = "";
  };
  return (
    <input
      className="setting-field text"
      type="password"
      autoComplete="new-password"
      spellCheck={false}
      placeholder={set ? "Saved" : "Not set"}
      {...labels}
      onBlur={(event) => commit(event.currentTarget)}
      onKeyDown={(event) => {
        if (event.key === "Enter") {
          event.preventDefault();
          commit(event.currentTarget);
        } else if (event.key === "Escape" && event.currentTarget.value !== "") {
          event.preventDefault();
          event.currentTarget.value = "";
        }
      }}
    />
  );
}
