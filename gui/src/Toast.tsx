import { useState } from "react";
import type { Action } from "./bindings/Action";
import type { StatusPayload } from "./bindings/StatusPayload";
import { toastView } from "./toastModel";

/** The api error first, else the status message. Both expire on the server. */
export function Toast({
  status,
  route,
  send,
}: {
  status: StatusPayload | null;
  route: string | null;
  send: (action: Action) => void;
}) {
  const [dismissed, setDismissed] = useState<string | null>(null);
  const view = toastView(status, dismissed);
  if (view.held !== dismissed) setDismissed(view.held);
  const shownError = view.error;
  const shownMessage = view.message;
  return (
    <>
      {/* Live regions stay mounted; a region inserted with its text is not announced. */}
      <div className="sr-only" role="alert">
        {shownError}
      </div>
      <div className="sr-only" role="status">
        {shownMessage}
      </div>
      {shownError ? (
        <div className="toast error">
          <span>{shownError}</span>
          <button
            type="button"
            aria-label="Dismiss"
            onClick={() => {
              // Leaving the error frame clears the error for every frontend.
              if (route === "error") send("Back");
              setDismissed(shownError);
            }}
          >
            ×
          </button>
        </div>
      ) : (
        shownMessage && (
          <div className={status?.is_error ? "toast error" : "toast"}>
            <span>{shownMessage}</span>
          </div>
        )
      )}
    </>
  );
}
