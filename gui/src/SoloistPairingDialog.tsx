import {
  useEffect,
  useRef,
  type KeyboardEvent,
  type MouseEvent,
  type Ref,
} from "react";
import { holdFocus } from "./AnnouncementDialog";
import type { Action } from "./bindings/Action";
import type { SoloistPairingGuide } from "./bindings/SoloistPairingGuide";
import { trapTab } from "./ConfirmDialog";

type Props = {
  guide: SoloistPairingGuide | null;
  send: (action: Action) => void;
};

/** The Soloist pairing guide in a modal, until Soloist logs in or Close dismisses it. */
export function SoloistPairingDialog(props: Props) {
  const close = useRef<HTMLButtonElement>(null);
  const open = props.guide !== null;
  useEffect(
    () => (open ? holdFocus(close.current, document.activeElement) : undefined),
    [open],
  );
  return <SoloistPairingModal {...props} close={close} />;
}

/** The modal without state, so a test can walk it. */
export function SoloistPairingModal({
  guide,
  send,
  close,
}: Props & { close?: Ref<HTMLButtonElement> }) {
  if (!guide) return null;
  const dismiss = () => send("DismissSoloistPairing");
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
        className="dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="soloist-pairing-title"
        onKeyDown={onKeyDown}
      >
        <h2 id="soloist-pairing-title">{guide.title}</h2>
        {guide.steps.map((step) => (
          <p key={step}>{step}</p>
        ))}
        <p role="status">{guide.waiting}</p>
        <div className="dialog-buttons">
          <button
            type="button"
            ref={close}
            className="dialog-confirm"
            onClick={dismiss}
          >
            {guide.close}
          </button>
        </div>
      </div>
    </div>
  );
}
