import { createContext, FC, ReactNode, useContext, useEffect } from "react";
import { ConfirmDialog, type ConfirmOptions } from "../components/ui/ConfirmDialog";
import { confirm, dismissConfirm, usePendingConfirm } from "../app/confirm";

interface ConfirmContextValue {
  confirm: (opts: ConfirmOptions) => Promise<boolean>;
}

const ConfirmContext = createContext<ConfirmContextValue | null>(null);

const VALUE: ConfirmContextValue = { confirm };

const DEFAULT_OPTS: ConfirmOptions = { title: "" };

/**
 * Draws the question `app/confirm`'s store holds, and hands components
 * `confirm` through the context they already read it from. The question
 * itself lives in the store so the native menu, outside React, can ask it.
 */
export const ConfirmProvider: FC<{ children: ReactNode }> = ({ children }) => {
  const pending = usePendingConfirm();

  // If the provider ever tears down (e.g. app unmount), answer any open
  // question `false` so callers don't leak unresolved promises.
  useEffect(() => dismissConfirm, []);

  // A move through history while the question is open answers it `false`:
  // the screen it was asked on is going, and so is its answer's meaning.
  useEffect(() => {
    window.addEventListener("popstate", dismissConfirm);
    return () => window.removeEventListener("popstate", dismissConfirm);
  }, []);

  return (
    <ConfirmContext.Provider value={VALUE}>
      {children}
      <ConfirmDialog
        {...(pending?.opts ?? DEFAULT_OPTS)}
        open={pending !== null}
        onConfirm={() => pending?.answer(true)}
        onCancel={() => pending?.answer(false)}
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
