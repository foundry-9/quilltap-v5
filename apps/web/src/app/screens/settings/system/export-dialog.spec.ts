import { ComponentFixture, TestBed } from '@angular/core/testing';
import { describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../../core/core-client';
import { ExportDialog } from './export-dialog';

/**
 * P4.D47 unit 2 — the export wizard over the widened type list (contract C1).
 *
 * The picker's rendered ORDER is the thing to pin: it is what a human reads,
 * and v4's own doctrine comment exists because three types were once missing
 * from it. The contract order is transcribed as a literal (not derived from
 * `EXPORTABLE_TYPES`) so that a reordering in the module goes red here as well
 * as in `import-export.types.spec.ts`.
 *
 * The rest pins that the eight NEW types need no new wizard machinery: none of
 * them supports memories (v4 `useExportData.ts:42-43` — still characters/chats
 * only at `7189a968`), so each is a 4-step, all-or-selected flow whose second
 * step already offers Export rather than Next.
 */

const CONTRACT_ORDER: readonly string[] = [
  'characters',
  'chats',
  'roleplay-templates',
  'prompt-templates',
  'connection-profiles',
  'image-profiles',
  'embedding-profiles',
  'tags',
  'projects',
  'groups',
  'document-stores',
  'files',
  'provider-models',
  'plugin-configs',
  'instance-settings',
];

const CONTRACT_LABELS: readonly string[] = [
  'Characters',
  'Chats',
  'Roleplay Templates',
  'Prompt Templates',
  'Connection Profiles',
  'Image Profiles',
  'Embedding Profiles',
  'Tags',
  'Projects',
  'Groups',
  'Document Stores',
  'Files & Folders',
  'Provider Models',
  'Plugin Settings',
  'Instance Settings',
];

function coreStub(dispatchData?: CoreClient['dispatchData']): Partial<CoreClient> {
  return { dispatchData: dispatchData ?? ((async () => ({})) as CoreClient['dispatchData']) };
}

async function mount(client: Partial<CoreClient>): Promise<ComponentFixture<ExportDialog>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [ExportDialog],
    providers: [{ provide: CoreClient, useValue: client }],
  });
  const fixture = TestBed.createComponent(ExportDialog);
  fixture.detectChanges();
  return fixture;
}

function typeRadios(fixture: ComponentFixture<unknown>): HTMLInputElement[] {
  return Array.from(
    (fixture.nativeElement as HTMLElement).querySelectorAll<HTMLInputElement>(
      'input[name="entity-type"]',
    ),
  );
}

function chooseType(fixture: ComponentFixture<unknown>, id: string): void {
  const radio = typeRadios(fixture).find((r) => r.value === id);
  radio!.dispatchEvent(new Event('change'));
  fixture.detectChanges();
}

function buttonWith(fixture: ComponentFixture<unknown>, label: string): HTMLButtonElement {
  return Array.from((fixture.nativeElement as HTMLElement).querySelectorAll('button')).find((b) =>
    b.textContent?.trim().startsWith(label),
  ) as HTMLButtonElement;
}

async function settle(fixture: ComponentFixture<unknown>): Promise<void> {
  for (let i = 0; i < 4; i++) {
    await Promise.resolve();
    fixture.detectChanges();
  }
}

describe('ExportDialog — the fifteen-type picker (contract C1)', () => {
  it('renders the fifteen radios in the contract order', async () => {
    const fixture = await mount(coreStub());
    expect(typeRadios(fixture).map((r) => r.value)).toEqual([...CONTRACT_ORDER]);
  });

  it("renders each row's label in the contract order", async () => {
    const fixture = await mount(coreStub());
    const labels = Array.from(
      (fixture.nativeElement as HTMLElement).querySelectorAll('input[name="entity-type"]'),
    ).map((r) => r.closest('label')?.textContent?.trim());
    expect(labels).toEqual([...CONTRACT_LABELS]);
  });
});

describe('ExportDialog — the new types need no new machinery', () => {
  it.each([
    'prompt-templates',
    'projects',
    'groups',
    'document-stores',
    'files',
    'provider-models',
    'plugin-configs',
    'instance-settings',
  ])('%s is a 4-step flow with no options step', async (type) => {
    const fixture = await mount(coreStub());
    chooseType(fixture, type);
    // v4 `getTotalSteps` returns 5 only for characters/chats on steps 1-2.
    expect((fixture.nativeElement as HTMLElement).textContent).toContain('Step 1 of 4');
  });

  it('instance-settings: Next loads the listing, then step 2 offers Export', async () => {
    const dispatchData = vi.fn(async () => ({
      entities: [
        { id: 'defaultModel', name: 'defaultModel' },
        { id: 'siteTitle', name: 'siteTitle' },
      ],
    })) as unknown as CoreClient['dispatchData'];
    const fixture = await mount(coreStub(dispatchData));

    chooseType(fixture, 'instance-settings');
    buttonWith(fixture, 'Next').click();
    await settle(fixture);

    expect(dispatchData).toHaveBeenCalledWith({
      type: 'systemExportEntities',
      entityType: 'instance-settings',
    });
    expect(buttonWith(fixture, 'Export')).toBeTruthy();
    // No memories step is offered for a non-character/chat type.
    expect((fixture.nativeElement as HTMLElement).textContent).not.toContain(
      'Include associated memories',
    );
  });

  it("renders the server's entity names verbatim (contract C4 shapes)", async () => {
    // provider-models arrive pre-composed as `${provider} / ${modelId}`, and
    // instance-settings as their bare key. The wizard must not reformat either.
    const dispatchData = vi.fn(async () => ({
      entities: [
        { id: 'pm1', name: 'anthropic / claude-opus-4' },
        { id: 'pm2', name: 'openai / gpt-5' },
      ],
    })) as unknown as CoreClient['dispatchData'];
    const fixture = await mount(coreStub(dispatchData));

    chooseType(fixture, 'provider-models');
    buttonWith(fixture, 'Next').click();
    await settle(fixture);

    // The listing renders only under "Select Specific".
    const specific = Array.from(
      (fixture.nativeElement as HTMLElement).querySelectorAll<HTMLInputElement>(
        'input[name="scope"]',
      ),
    )[1];
    specific.dispatchEvent(new Event('change'));
    fixture.detectChanges();

    const text = (fixture.nativeElement as HTMLElement).textContent ?? '';
    expect(text).toContain('anthropic / claude-opus-4');
    expect(text).toContain('openai / gpt-5');
    expect(text).toContain('0 of 2 selected');
  });
});
