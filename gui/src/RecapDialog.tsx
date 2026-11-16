import {
  useEffect,
  useRef,
  type KeyboardEvent,
  type MouseEvent,
  type Ref,
} from "react";
import { holdFocus } from "./AnnouncementDialog";
import type { Action } from "./bindings/Action";
import type { RecapPrompt } from "./bindings/RecapPrompt";
import { trapTab } from "./ConfirmDialog";
import "./RecapDialog.css";

type Props = {
  prompt: RecapPrompt | null;
  send: (action: Action) => void;
};

/** The monthly or year recap that is ready, in a modal; Escape answers Later. */
export function RecapDialog(props: Props) {
  const openButton = useRef<HTMLButtonElement>(null);
  const open = props.prompt !== null;
  useEffect(
    () =>
      open ? holdFocus(openButton.current, document.activeElement) : undefined,
    [open],
  );
  return <RecapModal {...props} openButton={openButton} />;
}

/** The modal without state, so a test can walk it. */
export function RecapModal({
  prompt,
  send,
  openButton,
}: Props & { openButton?: Ref<HTMLButtonElement> }) {
  if (!prompt) return null;
  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    event.stopPropagation();
    trapTab(event);
    if (event.key !== "Escape") return;
    event.preventDefault();
    send("DismissRecapPrompt");
  };
  // A click on the backdrop neither closes the dialog nor takes the focus out of it.
  const keepFocus = (event: MouseEvent<HTMLDivElement>) => {
    if (event.target === event.currentTarget) event.preventDefault();
  };
  return (
    <div className="dialog-scrim" onMouseDown={keepFocus}>
      <div
        className="dialog recap-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="recap-title"
        onKeyDown={onKeyDown}
      >
        <h2 id="recap-title">{prompt.title}</h2>
        <p className="recap-ready">{prompt.ready}</p>
        <p>{prompt.listens}</p>
        <p>{prompt.browser}</p>
        <div className="dialog-buttons">
          <button
            type="button"
            className="dialog-cancel recap-disable"
            onClick={() => send("DisableRecapPrompt")}
          >
            {prompt.disable}
          </button>
          <button
            type="button"
            className="dialog-cancel"
            onClick={() => send("DismissRecapPrompt")}
          >
            {prompt.later}
          </button>
          <button
            type="button"
            ref={openButton}
            className="dialog-confirm"
            onClick={() => send("OpenRecap")}
          >
            {prompt.open}
          </button>
        </div>
      </div>
    </div>
  );
}
