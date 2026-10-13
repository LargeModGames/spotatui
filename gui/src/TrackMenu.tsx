import {
  createContext,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type KeyboardEvent,
} from "react";
import { createPortal } from "react-dom";
import type { Action } from "./bindings/Action";
import type { PlaylistInfo } from "./bindings/PlaylistInfo";
import { Glyph } from "./Icon";
import type { OpenView } from "./nav";
import type { MenuEntry } from "./trackMenuModel";
import "./TrackMenu.css";

/** What a track row's heart and menu act with; absent, rows show neither. */
export interface TrackActions {
  send: (action: Action) => void;
  open: OpenView;
  /** The liked Spotify track ids among the rows on screen. */
  liked: ReadonlySet<string>;
  /** The Spotify playlists the user can add tracks to. */
  playlists: PlaylistInfo[];
}

export const TrackActionsContext = createContext<TrackActions | null>(null);

/** A track's menu at a window point, drawn over the page. */
export function TrackMenu({
  label,
  groups,
  at,
  actions,
  onClose,
}: {
  label: string;
  groups: MenuEntry[][];
  at: { x: number; y: number };
  actions: TrackActions;
  onClose: () => void;
}) {
  const run = (entry: MenuEntry) => {
    if (entry.kind === "action") actions.send(entry.action);
    else if (entry.kind === "open") actions.open(entry.view);
    else if (entry.kind === "copy") copy(entry.text, entry.done, actions.send);
    if (entry.kind !== "submenu") onClose();
  };
  return createPortal(
    <MenuList
      label={label}
      groups={groups}
      at={at}
      run={run}
      onClose={onClose}
      root
    />,
    document.body,
  );
}

export function MenuList({
  label,
  groups,
  at,
  run,
  onClose,
  root = false,
}: {
  label: string;
  groups: MenuEntry[][];
  at: { x: number; y: number };
  run: (entry: MenuEntry) => void;
  onClose: () => void;
  root?: boolean;
}) {
  const menu = useRef<HTMLDivElement>(null);
  const [place, setPlace] = useState(at);
  const [sub, setSub] = useState<{
    key: string;
    entries: MenuEntry[];
    at: { x: number; y: number };
  } | null>(null);

  // The menu stays inside the window, and the first entry takes the keyboard.
  useLayoutEffect(() => {
    const box = menu.current?.getBoundingClientRect();
    if (!box) return;
    setPlace({
      x: Math.max(8, Math.min(at.x, window.innerWidth - box.width - 8)),
      y: Math.max(8, Math.min(at.y, window.innerHeight - box.height - 8)),
    });
    menu.current?.querySelector<HTMLElement>('[role="menuitem"]')?.focus();
  }, [at]);

  useEffect(() => {
    if (!root) return;
    const away = (event: PointerEvent) => {
      if (!(
        event.target instanceof Element && event.target.closest(".track-menu")
      ))
        onClose();
    };
    const blur = () => onClose();
    document.addEventListener("pointerdown", away);
    window.addEventListener("blur", blur);
    window.addEventListener("resize", blur);
    return () => {
      document.removeEventListener("pointerdown", away);
      window.removeEventListener("blur", blur);
      window.removeEventListener("resize", blur);
    };
  }, [root, onClose]);

  const openSub = (entry: MenuEntry, button: HTMLElement) => {
    if (entry.kind !== "submenu") return;
    const box = button.getBoundingClientRect();
    setSub({
      key: entry.label,
      entries: entry.entries,
      at: { x: box.right - 4, y: box.top - 4 },
    });
  };

  // Portal events bubble through the React tree: the row must not see them.
  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    event.stopPropagation();
    const items = [
      ...(menu.current?.querySelectorAll<HTMLElement>(
        ':scope > [role="menuitem"], :scope > .menu-group > [role="menuitem"]',
      ) ?? []),
    ];
    const at = items.indexOf(document.activeElement as HTMLElement);
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      const step = event.key === "ArrowDown" ? 1 : -1;
      items[(at + step + items.length) % items.length]?.focus();
    } else if (event.key === "Escape" || (!root && event.key === "ArrowLeft")) {
      onClose();
    } else if (event.key === "Tab") {
      onClose();
    } else return;
    event.preventDefault();
  };

  return (
    <>
      <div
        ref={menu}
        className="track-menu"
        role="menu"
        aria-label={label}
        style={{ left: place.x, top: place.y }}
        onKeyDown={onKeyDown}
        onClick={(event) => event.stopPropagation()}
        onDoubleClick={(event) => event.stopPropagation()}
        onContextMenu={(event) => {
          event.preventDefault();
          event.stopPropagation();
        }}
      >
        {groups.map((group, index) => (
          <div key={index} className="menu-group">
            {group.map((entry) => (
              <button
                key={entry.label}
                type="button"
                role="menuitem"
                aria-haspopup={entry.kind === "submenu" ? "menu" : undefined}
                aria-expanded={
                  entry.kind === "submenu"
                    ? sub?.key === entry.label
                    : undefined
                }
                onClick={(event) => {
                  if (entry.kind === "submenu")
                    openSub(entry, event.currentTarget);
                  else run(entry);
                }}
                onPointerEnter={(event) => {
                  if (entry.kind === "submenu")
                    openSub(entry, event.currentTarget);
                  else setSub(null);
                }}
                onKeyDown={(event) => {
                  if (entry.kind === "submenu" && event.key === "ArrowRight") {
                    openSub(entry, event.currentTarget);
                    event.preventDefault();
                  }
                }}
              >
                <span>{entry.label}</span>
                {entry.kind === "submenu" && <Glyph name="forward" size={14} />}
              </button>
            ))}
          </div>
        ))}
      </div>
      {sub && (
        <MenuList
          key={sub.key}
          label={sub.key}
          groups={[sub.entries]}
          at={sub.at}
          run={run}
          onClose={() => {
            setSub(null);
            menu.current
              ?.querySelector<HTMLElement>('[aria-expanded="true"]')
              ?.focus();
          }}
        />
      )}
    </>
  );
}

function copy(text: string, done: string, send: (action: Action) => void) {
  const failed = () => send({ NotifyError: ["Could not copy the link.", 4] });
  const write = navigator.clipboard?.writeText(text);
  if (!write) return failed();
  write.then(() => send({ Notify: [done, 3] }), failed);
}
