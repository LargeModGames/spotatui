import { useEffect, useLayoutEffect, useRef } from "react";
import type { Action } from "./bindings/Action";
import type { DeviceInfo } from "./bindings/DeviceInfo";
import { Glyph } from "./Icon";

/** The device picker above the devices button. Clicks inside its parent, the button's box, are not outside clicks. */
export function DevicesPopover({
  devices,
  current,
  send,
  onClose,
}: {
  devices: DeviceInfo[];
  /** The name of the device that plays now. */
  current: string | null;
  send: (action: Action) => void;
  onClose: () => void;
}) {
  const root = useRef<HTMLDivElement>(null);
  useEffect(() => {
    send("RefreshDevices");
  }, [send]);
  useLayoutEffect(() => {
    root.current?.querySelector<HTMLElement>("button:not(:disabled)")?.focus();
  }, []);
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      event.preventDefault();
      onClose();
    };
    const onPointerDown = (event: PointerEvent) => {
      const box = root.current?.parentElement ?? root.current;
      if (event.target instanceof Node && box?.contains(event.target)) return;
      onClose();
    };
    document.addEventListener("keydown", onKeyDown);
    document.addEventListener("pointerdown", onPointerDown);
    return () => {
      document.removeEventListener("keydown", onKeyDown);
      document.removeEventListener("pointerdown", onPointerDown);
    };
  }, [onClose]);

  const active = devices.find((device) => device.is_active)?.name ?? current;
  const others = devices.filter(
    (device) => !device.is_active && device.name !== active,
  );
  return (
    <div
      ref={root}
      className="devices-popover"
      role="dialog"
      aria-label="Devices"
    >
      <h2>Connect to a device</h2>
      {active && (
        <div className="device-current">
          <Glyph name="devices" size={28} />
          <span>
            <strong>Current device</strong>
            <span>{active}</span>
          </span>
        </div>
      )}
      <h3>Select another device</h3>
      {others.length === 0 ? (
        <p className="device-empty">No other devices found.</p>
      ) : (
        <ul>
          {others.map((device) => (
            <li key={device.id ?? device.name}>
              <button
                type="button"
                disabled={device.id === null}
                onClick={() => {
                  if (device.id === null) return;
                  send({
                    TransferPlayback: { device_id: device.id, persist: false },
                  });
                  onClose();
                }}
              >
                <Glyph name="devices" size={20} />
                <span>
                  <strong>{device.name}</strong>
                  <span>{device.kind}</span>
                </span>
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
