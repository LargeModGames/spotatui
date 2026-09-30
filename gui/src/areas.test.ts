import { describe, expect, it } from "vitest";
import { shellKey, type ShellKey } from "./areas";

const press = (key: string, patch: Partial<ShellKey> = {}): ShellKey => ({
  key,
  modified: false,
  composing: false,
  handled: false,
  typing: false,
  interactive: false,
  overlay: false,
  ...patch,
});

describe("shellKey", () => {
  it("a digit switches to its area", () => {
    expect(shellKey(press("1"))).toEqual({ area: "library" });
    expect(shellKey(press("5"))).toEqual({ area: "stats" });
    expect(shellKey(press("7"))).toBeNull();
  });

  it("a digit typed into a text field does not switch the area", () => {
    expect(shellKey(press("2", { typing: true }))).toBeNull();
  });

  it("a key held with ctrl, meta or alt passes through to the browser", () => {
    expect(shellKey(press("1", { modified: true }))).toBeNull();
  });

  it("a key a screen already handled does nothing more", () => {
    expect(shellKey(press(" ", { handled: true }))).toBeNull();
  });

  it("a key during IME composition does nothing", () => {
    expect(shellKey(press("1", { composing: true }))).toBeNull();
  });

  it("space toggles playback only when no button, link or slider has focus", () => {
    expect(shellKey(press(" "))).toBe("toggle");
    expect(shellKey(press(" ", { interactive: true }))).toBeNull();
  });

  it("colon opens command mode", () => {
    expect(shellKey(press(":"))).toBe("command");
  });

  it("slash opens the search field", () => {
    expect(shellKey(press("/"))).toBe("search");
    expect(shellKey(press("/", { typing: true }))).toBeNull();
  });

  it("capital Q opens the queue and plain q is left to the screen", () => {
    expect(shellKey(press("Q"))).toBe("queue");
    expect(shellKey(press("q"))).toBeNull();
  });

  it("an open overlay swallows the area keys but not escape", () => {
    expect(shellKey(press("1", { overlay: true }))).toBeNull();
    expect(shellKey(press("Escape", { overlay: true }))).toBe("escape");
    expect(shellKey(press("Escape", { typing: true }))).toBe("escape");
  });
});
