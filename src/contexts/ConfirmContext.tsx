import { createContext, FC, ReactNode, useCallback, useContext, useEffect, useRef, useState } from "react";
import { ConfirmDialog, type ConfirmOptions } from "../components/ui/ConfirmDialog";

interface ConfirmContextValue {
  confirm: (opts: ConfirmOptions) => Promise<boolean>;
}

const ConfirmContext = createContext<ConfirmContextValue | null>(null);

interface ConfirmState {
  open: boolean;
  opts: ConfirmOptions;
}

const DEFAULT_OPTS: ConfirmOptions = { title: "" };

export const ConfirmProvider: FC<{ children: ReactNode }> = ({ children }) => {
  const [state, setState] = useState<ConfirmState>({ open: false, opts: DEFAULT_OPTS });
  // Holds the resolve function of the currently pending Promise.
  const resolveRef = useRef<((value: boolean) => void) | null>(null);

  // If the provider ever tears down (e.g. app unmount), resolve any pending promise as false
  // so callers don't leak unresolved promises.
  useEffect(() => {
    return () => {
      resolveRef.current?.(false);
    };
  }, []);

  // Guard against browser back/forward navigation while dialog is open.
  // The app has no URL router (navigation is state-based), so the back button
  // would navigate away entirely — but popstate also fires for programmatic
  // history.back() calls and any future URL routing added to the app.
  useEffect(() => {
    const handlePopState = () => {
      if (resolveRef.current !== null) {
        resolveRef.current(false);
        resolveRef.current = null;
        setState((s) => ({ ...s, open: false }));
      }
    };
    window.addEventListener("popstate", handlePopState);
    return () => window.removeEventListener("popstate", handlePopState);
  }, []);

  const confirm = useCallback((opts: ConfirmOptions): Promise<boolean> => {
    // Guard: if a dialog is already open, reject the new call rather than
    // silently orphaning the pending promise.
    if (resolveRef.current !== null) {
      return Promise.resolve(false);
    }
    return new Promise<boolean>((resolve) => {
      resolveRef.current = resolve;
      setState({ open: true, opts });
    });
  }, []);

  const handleConfirm = useCallback(() => {
    setState((s) => ({ ...s, open: false }));
    resolveRef.current?.(true);
    resolveRef.current = null;
  }, []);

  const handleCancel = useCallback(() => {
    setState((s) => ({ ...s, open: false }));
    resolveRef.current?.(false);
    resolveRef.current = null;
  }, []);

  return (
    <ConfirmContext.Provider value={{ confirm }}>
      {children}
      <ConfirmDialog
        {...state.opts}
        open={state.open}
        onConfirm={handleConfirm}
        onCancel={handleCancel}
      />
    </ConfirmContext.Provider>
  );
};

/**
 * Returns a `confirm` function that shows a styled confirmation dialog and resolves to a boolean.
 * A direct, accessible replacement for `window.confirm()`.
 *
 * @example
 * const { confirm } = useConfirmContext();
 * const ok = await confirm({ title: 'Remove "foo"?', variant: 'danger' });
 * if (!ok) return;
 */
export function useConfirmContext(): ConfirmContextValue {
  const ctx = useContext(ConfirmContext);
  if (!ctx) throw new Error("useConfirmContext must be used within a ConfirmProvider");
  return ctx;
}
