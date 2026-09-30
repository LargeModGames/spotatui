import type { StatusPayload } from "./bindings/StatusPayload";

/** What the toast shows. */
export interface ToastView {
  /** The dismissed text while the server still carries it; null once it is gone. */
  held: string | null;
  error: string | null;
  message: string | null;
}

/**
 * The api error first, else the status message, minus a dismissed text. The server hands an
 * expiring error to the status line with the same text, so the dismissal holds until neither
 * carries it; the same failure later shows again.
 */
export function toastView(
  status: StatusPayload | null,
  dismissed: string | null,
): ToastView {
  const apiError = status?.api_error ?? null;
  const statusMessage = status?.message ?? null;
  const held =
    dismissed !== null &&
    (apiError === dismissed || statusMessage === dismissed)
      ? dismissed
      : null;
  const error = apiError && apiError !== held ? apiError : null;
  const message =
    !error && statusMessage && statusMessage !== held ? statusMessage : null;
  return { held, error, message };
}
