import {
  useEffect,
  useRef,
  type KeyboardEvent,
  type MouseEvent,
  type Ref,
} from "react";
import { holdFocus } from "./AnnouncementDialog";
import type { Action } from "./bindings/Action";
import type { SoloistOffer } from "./bindings/SoloistOffer";
import { trapTab } from "./ConfirmDialog";

type Props = {
  offer: SoloistOffer | null;
  send: (action: Action) => void;
  /** Open the Settings page, where the Soloist rows are. */
  onSetUp: () => void;
};

/** The offer to set up Soloist in a modal, after librespot failed several tracks in a row. */
export function SoloistOfferDialog(props: Props) {
  const setUp = useRef<HTMLButtonElement>(null);
  const open = props.offer !== null;
  useEffect(
    () => (open ? holdFocus(setUp.current, document.activeElement) : undefined),
    [open],
  );
  return <SoloistOfferModal {...props} setUp={setUp} />;
}

/** The modal without state, so a test can walk it. */
export function SoloistOfferModal({
  offer,
  send,
  onSetUp,
  setUp,
}: Props & { setUp?: Ref<HTMLButtonElement> }) {
  if (!offer) return null;
  const answer = (set_up: boolean) => {
    send({ AnswerSoloistOffer: { set_up } });
    if (set_up) onSetUp();
  };
  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    event.stopPropagation();
    trapTab(event);
    if (event.key !== "Escape") return;
    event.preventDefault();
    answer(false);
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
        aria-labelledby="soloist-offer-title"
        onKeyDown={onKeyDown}
      >
        <h2 id="soloist-offer-title">{offer.title}</h2>
        <p>{offer.body}</p>
        {offer.steps.map((step) => (
          <p key={step}>{step}</p>
        ))}
        <div className="dialog-buttons">
          <button
            type="button"
            className="dialog-cancel"
            onClick={() => answer(false)}
          >
            {offer.not_now}
          </button>
          <button
            type="button"
            ref={setUp}
            className="dialog-confirm"
            onClick={() => answer(true)}
          >
            {offer.set_up}
          </button>
        </div>
      </div>
    </div>
  );
}
