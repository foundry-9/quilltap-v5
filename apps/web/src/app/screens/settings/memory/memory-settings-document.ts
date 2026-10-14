import { Signal, WritableSignal, computed, effect, inject, signal } from '@angular/core';
import { injectQuery, injectQueryClient } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../../../core/core-client';
import { ToastService } from '../../../ui/toast.service';
import { readErrorText, writeErrorText } from './api-error-text';

/**
 * One instance-wide memory settings document — v4 `components/tools/hooks/
 * useMemorySettingsDocument.ts` (`b045dd986`, at `83334b9a5`): read through
 * TanStack Query, held as local form state, and saved one patch at a time.
 * The server merges each patch over the stored value (v4 bug 244), so a
 * section sends only what it changed — and shows the server's echo over the
 * defaults, never a client merge (P4.D276 R-F's mechanics, L-2).
 *
 * v4's hook is one module used three times (the Maintenance card's two
 * documents and the extraction limits); this is its one twin (P4.D300 L-1).
 * An injection-context factory over the existing `injectQuery` idiom — call it
 * from a component's field initialiser. No realtime subscription (P4.D276 R-J):
 * the global topic map invalidates `['memories', …]` keys (R-AB).
 */
export interface MemorySettingsDocument<T> {
  config: WritableSignal<T>;
  loading: Signal<boolean>;
  saving: Signal<boolean>;
  error: Signal<string | null>;
  setError: (message: string | null) => void;
  /** POST one patch; resolves true when it saved. */
  save: (patch: Partial<T>) => Promise<boolean>;
}

export function injectMemorySettingsDocument<T extends object>(options: {
  queryKey: readonly unknown[];
  read: (core: CoreClient) => Promise<Partial<T> | null>;
  write: (core: CoreClient, patch: Partial<T>) => Promise<Partial<T> | null>;
  /** A module constant. */
  defaults: T;
  /** "consolidation settings", "maintenance settings" — used in the toasts and errors. */
  noun: string;
}): MemorySettingsDocument<T> {
  const { queryKey, read, write, defaults, noun } = options;
  const core = inject(CoreClient);
  const toasts = inject(ToastService);
  const queryClient = injectQueryClient();
  const config = signal<T>(defaults);
  const saving = signal(false);
  const error = signal<string | null>(null);

  const query = injectQuery(() => ({
    queryKey,
    queryFn: () => read(core),
  }));

  // TanStack's signal proxy maps a GENERIC data type through `Function & T`,
  // which TypeScript cannot call — name the signal's real type once.
  const data = query.data as Signal<Partial<T> | null | undefined>;

  // v4: seed the form from the query, only when `settings` came back.
  effect(() => {
    const loaded = data();
    if (loaded) config.set({ ...defaults, ...loaded });
  });
  // v4: surface the read failure in the shared error line.
  effect(() => {
    const loadError = query.error();
    if (loadError) error.set(readErrorText(loadError, `Failed to load ${noun}`));
  });

  const save = async (patch: Partial<T>): Promise<boolean> => {
    saving.set(true);
    error.set(null);
    try {
      const settings = await write(core, patch);
      config.set({ ...defaults, ...(settings ?? {}) });
      void queryClient.invalidateQueries({ queryKey });
      toasts.showSuccess(`${noun.charAt(0).toUpperCase()}${noun.slice(1)} saved`);
      return true;
    } catch (err) {
      const msg = writeErrorText(err, `Failed to save ${noun}`);
      error.set(msg);
      toasts.showError(msg);
      return false;
    } finally {
      saving.set(false);
    }
  };

  return {
    config,
    loading: computed(() => query.isLoading()),
    saving: saving.asReadonly(),
    error: error.asReadonly(),
    setError: (message) => error.set(message),
    save,
  };
}
