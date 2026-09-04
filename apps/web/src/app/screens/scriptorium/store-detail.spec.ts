import { ComponentFixture, TestBed } from '@angular/core/testing';
import { provideRouter } from '@angular/router';
import { describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../core/core-client';
import { WORKSPACE_TAB_ID, WORKSPACE_TAB_VISIBLE } from '../../workspace/workspace-contract';
import { StoreDetail } from './store-detail';

/**
 * P4.75 (the `title=` census, item 5): the file-manager toggle's tooltip is
 * CONTRACTUAL COPY, byte-for-byte v4's
 * `app/scriptorium/[id]/DocumentStoreDetailView.tsx:241` at the `0b0617fee`
 * pin. v5 had shortened it to "Preview the new file manager", dropping the
 * word the sentence exists to say — which manager is being previewed. The
 * census found it as a v4 string with no v5 twin; this pins the repair at its
 * source so a future tidy cannot re-shorten it silently.
 */
describe('StoreDetail — the file-manager toggle (v4 DocumentStoreDetailView:238-244)', () => {
  async function render(): Promise<ComponentFixture<StoreDetail>> {
    const core = {
      dispatchData: vi.fn(async (req: { type: string }) => {
        if (req.type === 'mountPointGet') {
          return {
            mountPoint: {
              id: 'm1',
              name: 'Quilltap General',
              mountType: 'database',
              scanStatus: 'idle',
              lastScannedAt: null,
              totalSizeBytes: 0,
              chunkCount: 1,
              embeddedChunkCount: 1,
              fileCount: 0,
              // The toggle is gated on the store advertising capabilities.
              capabilities: { canUpload: true, canDelete: true },
            },
          };
        }
        return { files: [] };
      }),
      dispatch: vi.fn(),
      events$: { subscribe: () => ({ unsubscribe() {} }) },
    };
    TestBed.configureTestingModule({
      imports: [StoreDetail],
      providers: [
        provideRouter([]),
        { provide: CoreClient, useValue: core },
        { provide: WORKSPACE_TAB_ID, useValue: null },
        { provide: WORKSPACE_TAB_VISIBLE, useValue: () => true },
      ],
    });
    // The template `@defer`s the file manager, so the component carries
    // unresolved metadata until compiled.
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(StoreDetail);
    fixture.componentRef.setInput('storeId', 'm1');
    fixture.detectChanges();
    // The loads are plain promises (zoneless: `whenStable` does not await them),
    // so drain the microtask/macrotask queue before reading the DOM.
    await new Promise((resolve) => setTimeout(resolve, 0));
    await fixture.whenStable();
    fixture.detectChanges();
    return fixture;
  }

  it("carries v4's whole sentence, SVAR included (v4 :241)", async () => {
    const fixture = await render();
    const toggle = Array.from(
      fixture.nativeElement.querySelectorAll('button'),
    ).find((b) => (b as HTMLElement).textContent?.includes('New file manager (beta)')) as
      | HTMLElement
      | undefined;
    expect(toggle, 'the file-manager toggle should render for a store with capabilities').toBeTruthy();
    expect(toggle!.getAttribute('title')).toBe('Preview the new SVAR-powered file manager');
  });
});
