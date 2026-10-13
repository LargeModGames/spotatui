import type { KeyboardEvent } from "react";
import { trapTab } from "./ConfirmDialog";
import "./Dialog.css";

/** The "Create playlist" dialog: a name field, Create and Cancel. The caller holds the name. */
export function PlaylistNameDialog({
  name,
  onName,
  onCreate,
  onCancel,
}: {
  name: string;
  onName: (name: string) => void;
  onCreate: (name: string) => void;
  onCancel: () => void;
}) {
  const blank = name.trim() === "";
  const create = () => {
    if (!blank) onCreate(name.trim());
  };
  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    event.stopPropagation();
    if (event.nativeEvent.isComposing) return;
    trapTab(event);
    if (event.key === "Escape") onCancel();
    else if (
      event.key === "Enter" &&
      (event.target as HTMLElement).tagName === "INPUT"
    )
      create();
    else return;
    event.preventDefault();
  };
  return (
    <div className="dialog-scrim" onClick={onCancel}>
      <div
        className="dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="playlist-name-title"
        onClick={(event) => event.stopPropagation()}
        onKeyDown={onKeyDown}
      >
        <h2 id="playlist-name-title">Create playlist</h2>
        <label className="dialog-field">
          <span>Name</span>
          <input
            type="text"
            autoFocus
            value={name}
            placeholder="My playlist"
            onChange={(event) => onName(event.target.value)}
          />
        </label>
        <div className="dialog-buttons">
          <button type="button" className="dialog-cancel" onClick={onCancel}>
            Cancel
          </button>
          <button
            type="button"
            className="dialog-confirm"
            disabled={blank}
            onClick={create}
          >
            Create
          </button>
        </div>
      </div>
    </div>
  );
}
