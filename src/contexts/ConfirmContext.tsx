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

  const confirm = useCallback((opts: ConfirmOptions): Promise<boolean> => {
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
