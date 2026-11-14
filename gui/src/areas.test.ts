import { describe, expect, it } from "vitest";
import type { PaneKeys } from "./bindings/PaneKeys";
import { modified, paneKey, shellKey, type ShellKey } from "./areas";

const press = (key: string, patch: Partial<ShellKey> = {}): ShellKey => ({
  key,
  modified: false,
  command: false,
  composing: false,
  handled: false,
  typing: false,
  interactive: false,
  overlay: false,
  code: "",
  alt: false,
  ctrl: false,
  shift: false,
  ...patch,
});

describe("shellKey", () => {
  it("a digit switches to its area", () => {
    expect(shellKey(press("1"))).toEqual({ area: "home" });
    expect(shellKey(press("5"))).toEqual({ area: "stats" });
    expect(shellKey(press("7"))).toEqual({ area: "visualizer" });
    expect(shellKey(press("8"))).toBeNull();
  });

  it("a digit typed into a text field does not switch the area", () => {
    expect(shellKey(press("2", { typing: true }))).toBeNull();
  });

  it("a key held with ctrl, meta or alt passes through to the browser", () => {
    expect(shellKey(press("1", { modified: true }))).toBeNull();
  });

  it("the command key with a comma opens Settings, even from a text field", () => {
    const comma = { modified: true, command: true };
    expect(shellKey(press(",", comma))).toBe("settings");
    expect(shellKey(press(",", { ...comma, typing: true }))).toBe("settings");
    expect(shellKey(press(",", { ...comma, overlay: true }))).toBeNull();
    expect(shellKey(press(",", { modified: true }))).toBeNull();
    expect(shellKey(press(","))).toBeNull();
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

  it("r opens the Room and l its lyrics", () => {
    expect(shellKey(press("r"))).toBe("room");
    expect(shellKey(press("l"))).toBe("lyrics");
    expect(shellKey(press("r", { typing: true }))).toBeNull();
  });

  it("colon opens command mode", () => {
    expect(shellKey(press(":"))).toBe("command");
  });

  it("slash opens the search field", () => {
    expect(shellKey(press("/"))).toBe("search");
    expect(shellKey(press("/", { typing: true }))).toBeNull();
  });

  it("capital T switches the mini player, but not from a text field", () => {
    expect(shellKey(press("T"))).toBe("mini");
    expect(shellKey(press("T", { typing: true }))).toBeNull();
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

const alt = (code: string) => ({ code, alt: true, ctrl: false, shift: false });
const KEYS: PaneKeys = {
  detach: alt("KeyD"),
  close: alt("KeyX"),
  focus_left: alt("KeyH"),
  focus_down: alt("KeyJ"),
  focus_up: alt("KeyK"),
  focus_right: alt("KeyL"),
  reset: alt("Digit0"),
};

describe("paneKey", () => {
  it("matches a configured chord by its key place, so macOS Option works", () => {
    const optionD = press("∂", { modified: true, code: "KeyD", alt: true });
    expect(paneKey(optionD, KEYS)).toBe("detach");
    expect(paneKey(press("0", { code: "Digit0", alt: true }), KEYS)).toBe(
      "reset",
    );
    expect(paneKey(press("l", { code: "KeyL", alt: true }), KEYS)).toEqual({
      focus: "right",
    });
  });

  it("an unbound or unmodified key passes through", () => {
    expect(paneKey(press("q", { code: "KeyQ", alt: true }), KEYS)).toBeNull();
    expect(paneKey(press("d", { code: "KeyD" }), KEYS)).toBeNull();
    expect(paneKey(press("d", { code: "KeyD", alt: true }), null)).toBeNull();
  });

  it("does nothing while typing or under an overlay", () => {
    const chord = { code: "KeyX", alt: true };
    expect(paneKey(press("x", { ...chord, typing: true }), KEYS)).toBeNull();
    expect(paneKey(press("x", { ...chord, overlay: true }), KEYS)).toBeNull();
  });
});

describe("modified", () => {
  it("is true while alt, ctrl, meta or shift is held, so a grip or divider lets the press go on", () => {
    const keys = {
      altKey: false,
      ctrlKey: false,
      metaKey: false,
      shiftKey: false,
    };
    expect(modified(keys)).toBe(false);
    for (const key of ["altKey", "ctrlKey", "metaKey", "shiftKey"] as const)
      expect(modified({ ...keys, [key]: true })).toBe(true);
  });
});
