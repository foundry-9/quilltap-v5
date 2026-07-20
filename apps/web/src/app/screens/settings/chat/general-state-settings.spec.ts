import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it } from 'vitest';

import { CoreClient } from '../../../core/core-client';
import { GeneralStateSettings } from './general-state-settings';

/**
 * The General State settings card (v4 `GeneralStateSettings.tsx`): an intro +
 * an "Edit General State" button that opens the shared editor in `general`
 * mode. The instance-wide tier takes NO entity id.
 */

interface Req {
  type: string;
  [k: string]: unknown;
}

function stubClient(onDispatch?: (req: Req) => void): Partial<CoreClient> {
  return {
    dispatchData: (async (req: Req) => {
      onDispatch?.(req);
      if (req.type === 'generalStateReset') return { success: true, previousState: {} };
      if (req.type === 'generalStateSet') return { success: true, state: req['state'] };
      return { success: true, state: {} };
    }) as CoreClient['dispatchData'],
  };
}

async function render(client: Partial<CoreClient>): Promise<ComponentFixture<GeneralStateSettings>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [GeneralStateSettings],
    providers: [{ provide: CoreClient, useValue: client }],
  });
  const fixture = TestBed.createComponent(GeneralStateSettings);
  fixture.detectChanges();
  return fixture;
}

async function settle(fixture: ComponentFixture<unknown>): Promise<void> {
  for (let i = 0; i < 6; i++) {
    await new Promise((r) => setTimeout(r));
    fixture.detectChanges();
  }
}

describe('GeneralStateSettings (v4 GeneralStateSettings.tsx)', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('renders the intro and the Edit button, with the modal closed', async () => {
    const seen: Req[] = [];
    const fixture = await render(stubClient((r) => seen.push(r)));
    const body = (fixture.nativeElement as HTMLElement).textContent ?? '';
    expect(body).toContain('instance-wide foundation of the state cascade');
    expect(body).toContain('Edit General State');
    // Nothing is fetched until the modal opens.
    expect(seen).toHaveLength(0);
    expect((fixture.nativeElement as HTMLElement).querySelector('qt-state-editor-modal')).toBeNull();
  });

  it('opens the shared editor in general mode (generalStateGet, no id)', async () => {
    const seen: Req[] = [];
    const fixture = await render(stubClient((r) => seen.push(r)));
    const button = [...(fixture.nativeElement as HTMLElement).querySelectorAll('button')].find(
      (b) => b.textContent?.trim() === 'Edit General State',
    ) as HTMLButtonElement;
    button.click();
    await settle(fixture);

    expect(seen[0]).toEqual({ type: 'generalStateGet' });
    const body = (fixture.nativeElement as HTMLElement).textContent ?? '';
    expect(body).toContain('General State');
    // The instance-wide tier never shows the chat cascade note.
    expect(body).not.toContain('narrower tiers win');
  });
});
