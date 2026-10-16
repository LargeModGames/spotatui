import type { KeyboardEvent, MouseEvent } from "react";
import type { Action } from "./bindings/Action";
import type { ActiveAnnouncement } from "./bindings/ActiveAnnouncement";
import type { AnnouncementLevel } from "./bindings/AnnouncementLevel";
import { trapTab } from "./ConfirmDialog";
import { EN, fmt, type GuiStrings } from "./i18n";
import { Glyph, type GlyphName } from "./Icon";
import "./AnnouncementDialog.css";

const LEVEL_GLYPH: Record<AnnouncementLevel, GlyphName> = {
  info: "info",
  warning: "warning",
  critical: "alert",
};

/** The level's icon in the level's color, for the modal and the Welcome page history. */
export function AnnouncementLevelIcon({ level }: { level: AnnouncementLevel }) {
  return (
    <span className={`announcement-icon ${level}`} aria-hidden="true">
      <Glyph name={LEVEL_GLYPH[level]} size={20} />
    </span>
  );
}

/** The active spotatui announcement in a modal; Escape or Got it dismisses it. */
export function AnnouncementDialog({
  s = EN,
  active,
  send,
}: {
  s?: GuiStrings;
  active: ActiveAnnouncement | null;
  send: (action: Action) => void;
}) {
  if (!active) return null;
  const { entry, position, total } = active;
  const dismiss = () => send({ DismissAnnouncement: { id: entry.id } });
  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    event.stopPropagation();
    trapTab(event);
    if (event.key !== "Escape") return;
    event.preventDefault();
    dismiss();
  };
  // A click on the backdrop neither closes the dialog nor takes the focus out of it.
  const keepFocus = (event: MouseEvent<HTMLDivElement>) => {
    if (event.target === event.currentTarget) event.preventDefault();
  };
  return (
    <div className="dialog-scrim" onMouseDown={keepFocus}>
      <div
        className={`dialog announcement-dialog ${entry.level}`}
        role={entry.level === "critical" ? "alertdialog" : "dialog"}
        aria-modal="true"
        aria-labelledby="announcement-title"
        aria-describedby="announcement-body"
        onKeyDown={onKeyDown}
      >
        <div className="announcement-head">
          <AnnouncementLevelIcon level={entry.level} />
          <h2 id="announcement-title">{entry.title}</h2>
          {total > 1 && (
            <span className="announcement-position">
              {fmt(s.announcement_position, { position, total })}
            </span>
          )}
        </div>
        <p id="announcement-body" className="announcement-body" tabIndex={0}>
          {entry.body}
        </p>
        <div className="dialog-buttons">
          {entry.url && (
            <button
              type="button"
              className="dialog-cancel"
              onClick={() => send({ OpenAnnouncementUrl: { id: entry.id } })}
            >
              {s.announcement_open}
            </button>
          )}
          <button
            type="button"
            className="dialog-confirm"
            autoFocus
            onClick={dismiss}
          >
            {s.announcement_got_it}
          </button>
        </div>
      </div>
    </div>
  );
}
