export const REDUCED_MOTION = "(prefers-reduced-motion: reduce)";

/** "auto" (jump) when the OS asks for less motion, "smooth" otherwise. */
export function scrollBehavior(): ScrollBehavior {
  return window.matchMedia(REDUCED_MOTION).matches ? "auto" : "smooth";
}
