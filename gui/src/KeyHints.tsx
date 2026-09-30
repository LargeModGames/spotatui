import type { ReactNode } from "react";

/** The mono key-hint footer of a screen. */
export function KeyHints({
  hints,
  children,
}: {
  hints: string[];
  children?: ReactNode;
}) {
  return (
    <footer className="keys">
      {hints.map((hint) => (
        <kbd key={hint}>{hint}</kbd>
      ))}
      {children}
    </footer>
  );
}
