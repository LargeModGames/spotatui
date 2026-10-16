import { EN, type GuiStrings } from "./i18n";
import type { KeyboardEvent } from "react";
import "./Dialog.css";

/** Keeps Tab inside a modal dialog: the last control wraps to the first and back. */
export function trapTab(event: KeyboardEvent<HTMLElement>) {
  if (event.key !== "Tab") return;
  const controls = [
    ...event.currentTarget.querySelectorAll<HTMLElement>(
      "input, button:not(:disabled), [tabindex='0']",
    ),
  ];
  const first = controls[0];
  const last = controls.at(-1);
  const wrapTo = event.shiftKey
    ? document.activeElement === first && last
    : document.activeElement === last && first;
  if (!wrapTo) return;
  event.preventDefault();
  wrapTo.focus();
}

/** A yes-or-no question before a destructive step; Cancel takes the focus. */
export function ConfirmDialog({
  s = EN,
  title,
  body,
  confirm,
  onConfirm,
  onCancel,
}: {
  s?: GuiStrings;
  title: string;
  body: string;
  /** The confirm button's label. */
  confirm: string;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    event.stopPropagation();
    trapTab(event);
    if (event.key !== "Escape") return;
    event.preventDefault();
    onCancel();
  };
  return (
    <div className="dialog-scrim" onClick={onCancel}>
      <div
        className="dialog"
        role="alertdialog"
        aria-modal="true"
        aria-labelledby="confirm-title"
        aria-describedby="confirm-body"
        onClick={(event) => event.stopPropagation()}
        onKeyDown={onKeyDown}
      >
        <h2 id="confirm-title">{title}</h2>
        <p id="confirm-body">{body}</p>
        <div className="dialog-buttons">
          <button
            type="button"
            className="dialog-cancel"
            autoFocus
            onClick={onCancel}
          >
            {s.cancel}
          </button>
          <button type="button" className="dialog-confirm" onClick={onConfirm}>
            {confirm}
          </button>
        </div>
      </div>
    </div>
  );
}
