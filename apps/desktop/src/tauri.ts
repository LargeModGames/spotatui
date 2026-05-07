export type InvokeArgs = Record<string, unknown>;
export type InvokeFunction = <T = unknown>(
  command: string,
  args?: InvokeArgs,
) => Promise<T>;

type TauriWindow = Window &
  typeof globalThis & {
    __TAURI__?: {
      core?: {
        invoke?: InvokeFunction;
      };
      tauri?: {
        invoke?: InvokeFunction;
      };
      invoke?: InvokeFunction;
    };
  };

type InvokeSuccess<T> = {
  ok: true;
  value: T;
};

type InvokeFailure = {
  ok: false;
  reason: "missing" | "failed";
  error?: unknown;
};

export type InvokeResult<T> = InvokeSuccess<T> | InvokeFailure;

let importedInvoke: InvokeFunction | null | undefined;

function getWindowInvoke(): InvokeFunction | null {
  if (typeof window === "undefined") {
    return null;
  }

  const tauriWindow = window as TauriWindow;
  return (
    tauriWindow.__TAURI__?.core?.invoke ??
    tauriWindow.__TAURI__?.tauri?.invoke ??
    tauriWindow.__TAURI__?.invoke ??
    null
  );
}

async function resolveInvoke(): Promise<InvokeFunction | null> {
  const windowInvoke = getWindowInvoke();
  if (windowInvoke) {
    return windowInvoke;
  }

  if (importedInvoke !== undefined) {
    return importedInvoke;
  }

  try {
    const tauriCore = await import("@tauri-apps/api/core");
    importedInvoke = tauriCore.invoke;
  } catch {
    importedInvoke = null;
  }

  return importedInvoke;
}

export async function invokeCommand<T = unknown>(
  command: string,
  args?: InvokeArgs,
): Promise<InvokeResult<T>> {
  const invoke = await resolveInvoke();

  if (!invoke) {
    return { ok: false, reason: "missing" };
  }

  try {
    return {
      ok: true,
      value: await invoke<T>(command, args),
    };
  } catch (error) {
    return { ok: false, reason: "failed", error };
  }
}
