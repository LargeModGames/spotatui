import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import { ConfirmDialog } from "./ConfirmDialog";
import { PlaylistNameDialog } from "./PlaylistNameDialog";
import { byName, hostElements } from "./testTree";

const key = (name: string, tagName = "INPUT", isComposing = false) => ({
  key: name,
  shiftKey: false,
  nativeEvent: { isComposing },
  target: { tagName },
  stopPropagation: () => {},
  preventDefault: vi.fn(),
});

const nameDialog = (name: string) => {
  const onCreate = vi.fn();
  const onCancel = vi.fn();
  const tree = (
    <PlaylistNameDialog
      name={name}
      onName={() => {}}
      onCreate={onCreate}
      onCancel={onCancel}
    />
  );
  const dialog = hostElements(tree).find(
    (element) => element.props.role === "dialog",
  );
  if (!dialog) throw new Error("no dialog");
  const onKeyDown = dialog.props.onKeyDown as (event: unknown) => void;
  return { tree, onCreate, onCancel, onKeyDown };
};

describe("PlaylistNameDialog", () => {
  it("is a modal dialog with a labelled name field", () => {
    const html = renderToStaticMarkup(nameDialog("").tree);
    expect(html).toContain('role="dialog"');
    expect(html).toContain('aria-modal="true"');
    expect(html).toMatch(/<label[^>]*><span>Name<\/span><input/);
    expect(html).toContain(">Create playlist</h2>");
  });

  it("creates with the trimmed name on Create or Enter in the field", () => {
    const named = nameDialog("  Road trip ");
    (byName(named.tree, "Create").props.onClick as () => void)();
    named.onKeyDown(key("Enter"));
    expect(named.onCreate.mock.calls).toEqual([["Road trip"], ["Road trip"]]);
  });

  it("does not create on the Enter that ends an IME composition", () => {
    const named = nameDialog("Road trip");
    named.onKeyDown(key("Enter", "INPUT", true));
    expect(named.onCreate).not.toHaveBeenCalled();
  });

  it("cannot create a blank name", () => {
    const blank = nameDialog("   ");
    const create = byName(blank.tree, "Create");
    expect(create.props.disabled).toBe(true);
    (create.props.onClick as () => void)();
    blank.onKeyDown(key("Enter"));
    expect(blank.onCreate).not.toHaveBeenCalled();
  });

  it("cancels on Escape and Cancel, and Enter on Cancel does not create", () => {
    const named = nameDialog("Mix");
    named.onKeyDown(key("Escape"));
    (byName(named.tree, "Cancel").props.onClick as () => void)();
    named.onKeyDown(key("Enter", "BUTTON"));
    expect(named.onCancel).toHaveBeenCalledTimes(2);
    expect(named.onCreate).not.toHaveBeenCalled();
  });
});

describe("ConfirmDialog", () => {
  const confirmDialog = () => {
    const onConfirm = vi.fn();
    const onCancel = vi.fn();
    const tree = (
      <ConfirmDialog
        title="Delete from Your Library?"
        body="This will delete Road trip from Your Library."
        confirm="Delete"
        onConfirm={onConfirm}
        onCancel={onCancel}
      />
    );
    return { tree, onConfirm, onCancel };
  };

  it("is an alert dialog that names the playlist", () => {
    const html = renderToStaticMarkup(confirmDialog().tree);
    expect(html).toContain('role="alertdialog"');
    expect(html).toContain('aria-modal="true"');
    expect(html).toContain("This will delete Road trip from Your Library.");
    expect(html).toContain(">Delete</button>");
  });

  it("confirms on its button and cancels on Cancel or Escape", () => {
    const { tree, onConfirm, onCancel } = confirmDialog();
    (byName(tree, "Delete").props.onClick as () => void)();
    expect(onConfirm).toHaveBeenCalledOnce();
    (byName(tree, "Cancel").props.onClick as () => void)();
    const dialog = hostElements(tree).find(
      (element) => element.props.role === "alertdialog",
    );
    (dialog?.props.onKeyDown as (event: unknown) => void)(key("Escape"));
    expect(onCancel).toHaveBeenCalledTimes(2);
    expect(onConfirm).toHaveBeenCalledOnce();
  });
});
