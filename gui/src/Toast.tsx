import { useState } from "react";
import type { Action } from "./bindings/Action";
import type { StatusPayload } from "./bindings/StatusPayload";

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
  // The server hands an expiring error to the status line with the same text; keep it hidden.
  const [dismissed, setDismissed] = useState<string | null>(null);
  const error = status?.api_error ?? null;
  if (error && error !== dismissed)
    return (
      <div className="toast error" role="alert">
        <span>{error}</span>
        <button
          type="button"
          aria-label="Dismiss"
          onClick={() => {
            // Leaving the error frame clears the error for every frontend.
            if (route === "error") send("Back");
            setDismissed(error);
          }}
        >
          ×
        </button>
      </div>
    );
  const message = status?.message ?? null;
  if (!message || message === dismissed) return null;
  return (
    <div className={status?.is_error ? "toast error" : "toast"} role="status">
      <span>{message}</span>
    </div>
  );
}
