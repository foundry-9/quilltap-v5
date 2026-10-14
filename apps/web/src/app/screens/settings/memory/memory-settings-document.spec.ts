import { ChangeDetectionStrategy, Component } from '@angular/core';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { describe, expect, it } from 'vitest';

import { CoreClient } from '../../../core/core-client';
import { CoreDispatchError } from '../../../core/core-contract';
import type { MemoryMaintenanceConfig } from '../../../core/core-contract';
import { fetchMaintenanceConfig, memoryKeys, setMaintenanceConfig } from '../../../memory/memory.api';
import { ToastService } from '../../../ui/toast.service';
import { injectMemorySettingsDocument } from './memory-settings-document';

/**
 * P4.D300 L-1 — the ONE settings-document helper, v4 `components/tools/hooks/
 * useMemorySettingsDocument.ts` (`b045dd986`, at `83334b9a5`). v4 shipped no
 * test for the hook; these rows are the lane's own, red-first.
 */

const DEFAULTS: MemoryMaintenanceConfig = {
  enabled: false,
  perCharacterCap: 2000,
  perCharacterCapOverrides: {},
  phases: { foldNearDuplicates: true, demote: true, retire: true },
};

@Component({
  selector: 'qt-doc-host',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `{{ doc.loading() ? 'loading' : 'ready' }}`,
})
class DocHost {
  readonly doc = injectMemorySettingsDocument<MemoryMaintenanceConfig>({
    queryKey: memoryKeys.maintenanceConfig(),
    read: fetchMaintenanceConfig,
    write: setMaintenanceConfig,
    defaults: DEFAULTS,
    noun: 'maintenance settings',
  });
}

interface Req {
  type: string;
  config?: Record<string, unknown>;
}
type Handler = (req: Req) => Record<string, unknown> | Promise<Record<string, unknown>>;

function render(
  handlers: Partial<Record<string, Handler>>,
  seen: Req[] = [],
): { fixture: ComponentFixture<DocHost>; queryClient: QueryClient } {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const client: Partial<CoreClient> = {
    dispatchData: (async (req: Req) => {
      seen.push(req);
      const h = handlers[req.type];
      return h ? h(req) : {};
    }) as CoreClient['dispatchData'],
  };
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [DocHost],
    providers: [provideTanStackQuery(queryClient), { provide: CoreClient, useValue: client }],
  });
  const fixture = TestBed.createComponent(DocHost);
  fixture.detectChanges();
  return { fixture, queryClient };
}

async function settle(fixture: ComponentFixture<unknown>): Promise<void> {
  for (let i = 0; i < 8; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

const toasts = () =>
  TestBed.inject(ToastService)
    .toasts()
    .map((t) => `${t.type}:${t.message}`);

describe('injectMemorySettingsDocument (P4.D300 L-1, v4 useMemorySettingsDocument.ts)', () => {
  it('seeds the config from the read, over the defaults', async () => {
    const { fixture } = render({
      memoryMaintenanceConfigGet: () => ({ success: true, settings: { perCharacterCap: 3000 } }),
    });
    expect(fixture.componentInstance.doc.config()).toEqual(DEFAULTS);
    await settle(fixture);
    expect(fixture.componentInstance.doc.loading()).toBe(false);
    expect(fixture.componentInstance.doc.config()).toEqual({ ...DEFAULTS, perCharacterCap: 3000 });
  });

  it('a read without settings leaves the defaults (v4 seeds only on `loaded.settings`)', async () => {
    const { fixture } = render({ memoryMaintenanceConfigGet: () => ({ success: true }) });
    await settle(fixture);
    expect(fixture.componentInstance.doc.config()).toEqual(DEFAULTS);
  });

  it('a save posts exactly the ONE patch and shows the echo over the defaults — no client merge', async () => {
    const seen: Req[] = [];
    const { fixture, queryClient } = render(
      {
        memoryMaintenanceConfigGet: () => ({ success: true, settings: { perCharacterCap: 3000, enabled: true } }),
        // The echo carries only `enabled`: what the card shows is the echo over the
        // DEFAULTS (perCharacterCap back to 2000), never the previous config.
        memoryMaintenanceConfigSet: () => ({ success: true, settings: { enabled: false } }),
      },
      seen,
    );
    await settle(fixture);
    let invalidated: unknown = null;
    const original = queryClient.invalidateQueries.bind(queryClient);
    queryClient.invalidateQueries = ((filters: { queryKey?: unknown }) => {
      invalidated = filters?.queryKey ?? null;
      return original(filters as never);
    }) as typeof queryClient.invalidateQueries;
    const ok = await fixture.componentInstance.doc.save({ enabled: false });
    expect(ok).toBe(true);
    expect(seen.filter((r) => r.type === 'memoryMaintenanceConfigSet')).toEqual([
      { type: 'memoryMaintenanceConfigSet', config: { enabled: false } },
    ]);
    expect(fixture.componentInstance.doc.config()).toEqual(DEFAULTS);
    expect(invalidated).toEqual(['memories', 'maintenance-config']);
    expect(toasts()).toEqual(['success:Maintenance settings saved']);
  });

  it('capitalises only the first letter of the noun in the toast', async () => {
    @Component({
      selector: 'qt-doc-host-2',
      changeDetection: ChangeDetectionStrategy.OnPush,
      template: ``,
    })
    class LimitsHost {
      readonly doc = injectMemorySettingsDocument<MemoryMaintenanceConfig>({
        queryKey: memoryKeys.extractionLimitsConfig(),
        read: fetchMaintenanceConfig,
        write: setMaintenanceConfig,
        defaults: DEFAULTS,
        noun: 'extraction limits',
      });
    }
    TestBed.resetTestingModule();
    TestBed.configureTestingModule({
      imports: [LimitsHost],
      providers: [
        provideTanStackQuery(new QueryClient({ defaultOptions: { queries: { retry: false } } })),
        {
          provide: CoreClient,
          useValue: { dispatchData: async () => ({ success: true, settings: {} }) },
        },
      ],
    });
    const fixture = TestBed.createComponent(LimitsHost);
    fixture.detectChanges();
    await settle(fixture);
    await fixture.componentInstance.doc.save({ enabled: true });
    expect(toasts()).toEqual(['success:Extraction limits saved']);
  });

  it('a read failure shows readErrorText — the fallback for a server failure', async () => {
    const { fixture } = render({
      memoryMaintenanceConfigGet: () => {
        throw new CoreDispatchError({ kind: 'internal', message: 'Something private' });
      },
    });
    await settle(fixture);
    expect(fixture.componentInstance.doc.error()).toBe('Failed to load maintenance settings');
  });

  it('a read failure that is not a server failure keeps its own message', async () => {
    const { fixture } = render({
      memoryMaintenanceConfigGet: () => {
        throw new Error('Connection lost');
      },
    });
    await settle(fixture);
    expect(fixture.componentInstance.doc.error()).toBe('Connection lost');
  });

  it('a write failure: the server sentence on the error line + an error toast, false, the config kept', async () => {
    const { fixture } = render({
      memoryMaintenanceConfigGet: () => ({ success: true, settings: { perCharacterCap: 3000 } }),
      memoryMaintenanceConfigSet: () => {
        throw new CoreDispatchError({ kind: 'bad-request', message: 'perCharacterCap: too big' });
      },
    });
    await settle(fixture);
    const ok = await fixture.componentInstance.doc.save({ perCharacterCap: 900000 });
    expect(ok).toBe(false);
    expect(fixture.componentInstance.doc.error()).toBe('perCharacterCap: too big');
    expect(toasts()).toEqual(['error:perCharacterCap: too big']);
    expect(fixture.componentInstance.doc.config().perCharacterCap).toBe(3000);
    expect(fixture.componentInstance.doc.saving()).toBe(false);
  });

  it('a write failure without a server sentence falls back to `Failed to save {noun}`', async () => {
    const { fixture } = render({
      memoryMaintenanceConfigGet: () => ({ success: true, settings: {} }),
      memoryMaintenanceConfigSet: () => {
        throw new CoreDispatchError({ kind: 'internal', message: '' });
      },
    });
    await settle(fixture);
    expect(await fixture.componentInstance.doc.save({ enabled: true })).toBe(false);
    expect(fixture.componentInstance.doc.error()).toBe('Failed to save maintenance settings');
  });

  it('a save clears the previous error first; setError writes the line', async () => {
    const { fixture } = render({
      memoryMaintenanceConfigGet: () => ({ success: true, settings: {} }),
      memoryMaintenanceConfigSet: () => ({ success: true, settings: {} }),
    });
    await settle(fixture);
    fixture.componentInstance.doc.setError('A local refusal');
    expect(fixture.componentInstance.doc.error()).toBe('A local refusal');
    await fixture.componentInstance.doc.save({ enabled: true });
    expect(fixture.componentInstance.doc.error()).toBeNull();
  });
});
