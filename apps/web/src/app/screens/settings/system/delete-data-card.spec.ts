import { ComponentFixture, TestBed } from '@angular/core/testing';
import { describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../../core/core-client';
import { DeleteDataCard } from './delete-data-card';

const SUMMARY = {
  characters: 3,
  chats: 2,
  tags: 1,
  files: 4,
  memories: 10,
  apiKeys: 1,
  backups: 0,
  projects: 2,
  profiles: { connection: 1, image: 0, embedding: 0 },
  templates: { prompt: 0, roleplay: 0 },
};

interface Stub {
  calls: { type: string; [k: string]: unknown }[];
  client: Partial<CoreClient>;
}

function stub(): Stub {
  const calls: { type: string; [k: string]: unknown }[] = [];
  const dispatchData = vi.fn(async (req: { type: string }) => {
    calls.push(req);
    return { summary: SUMMARY };
  });
  return { calls, client: { dispatchData: dispatchData as unknown as CoreClient['dispatchData'] } };
}

async function mount(s: Stub): Promise<ComponentFixture<DeleteDataCard>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [DeleteDataCard],
    providers: [{ provide: CoreClient, useValue: s.client }],
  });
  const fixture = TestBed.createComponent(DeleteDataCard);
  fixture.detectChanges();
  return fixture;
}

function buttonWith(fixture: ComponentFixture<unknown>, label: string): HTMLButtonElement {
  return Array.from((fixture.nativeElement as HTMLElement).querySelectorAll('button')).find(
    (b) => b.textContent?.trim() === label,
  ) as HTMLButtonElement;
}

async function settle(fixture: ComponentFixture<unknown>): Promise<void> {
  for (let i = 0; i < 4; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

describe('DeleteDataCard', () => {
  it('previews on open and computes a real (non-NaN) total', async () => {
    const s = stub();
    const fixture = await mount(s);
    buttonWith(fixture, 'Delete All Data').click();
    await settle(fixture);
    expect(s.calls).toContainEqual({ type: 'systemDeleteDataPreview' });
    const text = fixture.nativeElement.textContent;
    expect(text).toContain('Projects'); // the row v4 dropped
    // 3+2+1+4+10+1+0+2 + 1+0+0 = 24 (templates 0)
    expect(text).toContain('24');
    expect(text).not.toContain('NaN');
  });

  it('gates the delete behind the typed DELETE and sends the wire token', async () => {
    const s = stub();
    const fixture = await mount(s);
    buttonWith(fixture, 'Delete All Data').click();
    await settle(fixture);
    buttonWith(fixture, 'Continue').click();
    fixture.detectChanges();

    const del = buttonWith(fixture, 'Delete Everything');
    expect(del.disabled).toBe(true);

    const input = fixture.nativeElement.querySelector('input[type="text"]') as HTMLInputElement;
    input.value = 'delete'; // lower-case — the component upper-cases it
    input.dispatchEvent(new Event('input'));
    fixture.detectChanges();
    expect(buttonWith(fixture, 'Delete Everything').disabled).toBe(false);

    buttonWith(fixture, 'Delete Everything').click();
    await settle(fixture);
    expect(s.calls).toContainEqual({ type: 'systemDeleteData', confirm: 'DELETE_ALL_MY_DATA' });
    expect(fixture.nativeElement.textContent).toContain('Successfully deleted 24 items');
  });
});
