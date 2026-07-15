import { ComponentFixture, TestBed } from '@angular/core/testing';
import { By } from '@angular/platform-browser';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { afterEach, describe, expect, it } from 'vitest';

import { CoreClient } from '../../../core/core-client';
import { RichEditor } from '../../../editor/rich-editor';
import { DefaultAestheticsCard } from './default-aesthetics-card';
import { fetchSystemAesthetic, setSystemAesthetic } from './system-aesthetics.api';

interface DispatchReq {
  type: string;
  [k: string]: unknown;
}

/** Canned `{content}` per kind; every dispatch is recorded for the payload pins. */
function stubClient(
  content: Record<string, string>,
  seen: DispatchReq[],
  failSave = false,
): Partial<CoreClient> {
  return {
    dispatchData: (async (req: DispatchReq) => {
      seen.push(req);
      if (req.type === 'systemImageAestheticsGet') {
        return { content: content[req['kind'] as string] ?? '' };
      }
      if (req.type === 'systemImageAestheticsSet' && failSave) {
        throw new Error('store is not available');
      }
      return {};
    }) as CoreClient['dispatchData'],
  };
}

async function settle(fixture: ComponentFixture<unknown>): Promise<void> {
  for (let i = 0; i < 8; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

async function render(
  client: Partial<CoreClient>,
  { wait = true } = {},
): Promise<ComponentFixture<DefaultAestheticsCard>> {
  TestBed.configureTestingModule({
    imports: [DefaultAestheticsCard],
    providers: [provideTanStackQuery(new QueryClient()), { provide: CoreClient, useValue: client }],
  });
  const fixture = TestBed.createComponent(DefaultAestheticsCard);
  fixture.detectChanges();
  if (wait) await settle(fixture);
  return fixture;
}

/** The two fields in v4's order: lantern first, aurora second. */
function fields(fixture: ComponentFixture<DefaultAestheticsCard>): HTMLElement[] {
  return [...fixture.nativeElement.querySelectorAll('qt-aesthetic-editor-field')];
}

function saveButton(field: HTMLElement): HTMLButtonElement {
  return [...field.querySelectorAll('button')].find((b) =>
    b.textContent?.includes('Save'),
  ) as HTMLButtonElement;
}

/**
 * The Default Aesthetics card — v4 `components/settings/tabs/
 * ImagesTabContent.tsx:52-66`. Each assertion cites the v4 line it pins.
 */
describe('DefaultAestheticsCard', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('renders v4’s two fields, in v4’s order, with v4’s labels and copy', async () => {
    const fixture = await render(stubClient({}, []));
    const [lantern, aurora] = fields(fixture);
    expect(fields(fixture)).toHaveLength(2);

    // v4 :55-58 — the lantern field is FIRST.
    expect(lantern.querySelector('label')?.textContent?.trim()).toBe('Default Image Aesthetic');
    expect(lantern.querySelector('p')?.textContent?.trim()).toBe(
      'The overall look for scenes and backgrounds (medium, era, palette, rendering). Feeds story backgrounds and ad-hoc images.',
    );

    // v4 :60-64 — the aurora field is SECOND. The two vantage points are not
    // interchangeable: scenes vs. how people are depicted.
    expect(aurora.querySelector('label')?.textContent?.trim()).toBe('Default Character Aesthetic');
    expect(aurora.querySelector('p')?.textContent?.trim()).toBe(
      'How people and their outfits are depicted. Feeds avatars, plus the figures rendered in story backgrounds and ad-hoc images.',
    );
  });

  it('loads each kind through its own systemImageAestheticsGet', async () => {
    const seen: DispatchReq[] = [];
    await render(stubClient({ lantern: 'sepia', aurora: 'ink' }, seen));
    expect(seen.filter((r) => r.type === 'systemImageAestheticsGet')).toEqual([
      { type: 'systemImageAestheticsGet', kind: 'lantern' },
      { type: 'systemImageAestheticsGet', kind: 'aurora' },
    ]);
  });

  it('gates the editor on the load, then mounts it with the content in hand', async () => {
    // v4 :117-119. Load-bearing, not cosmetic: the field absorbs exactly one
    // emit, so an editor that mounted ahead of its content would surface the
    // load as an edit and a Save would rewrite the stored file in normalized
    // bytes. See markdown-field-load-must-not-emit.
    const pending = await render(stubClient({ lantern: '__sepia__' }, []), { wait: false });
    expect(pending.nativeElement.textContent).toContain('Loading…');
    expect(pending.nativeElement.querySelector('qt-markdown-field')).toBeNull();

    await settle(pending);
    expect(pending.nativeElement.textContent).not.toContain('Loading…');
    // The editor DISPLAYS the canonical serialization of the loaded bytes...
    expect(
      pending.debugElement.queryAll(By.directive(RichEditor))[0].componentInstance.getMarkdown(),
    ).toBe('**sepia**');
    // ...and the load left the field pristine, so the Save that would rewrite
    // those bytes is unreachable (v4 :127, `disabled={saving || !dirty}`).
    expect(saveButton(fields(pending)[0]).disabled).toBe(true);
  });

  it('sends the systemImageAestheticsSet shape verbatim on save', async () => {
    const seen: DispatchReq[] = [];
    const fixture = await render(stubClient({ lantern: 'sepia' }, seen));
    const lantern = fields(fixture)[0];

    // A genuine edit dirties the field, which is what unlocks Save (v4 :127).
    (lantern.querySelector('.qt-formatting-button-h2') as HTMLButtonElement).click();
    await settle(fixture);
    const save = saveButton(lantern);
    expect(save.disabled).toBe(false);
    save.click();
    await settle(fixture);

    expect(seen.filter((r) => r.type === 'systemImageAestheticsSet')).toEqual([
      { type: 'systemImageAestheticsSet', kind: 'lantern', content: '## sepia' },
    ]);
    // v4 :131 — the success span appears once saved and no longer dirty.
    expect(lantern.textContent).toContain('Saved');
    expect(saveButton(lantern).disabled).toBe(true);
  });

  it('surfaces a save failure and leaves the field dirty', async () => {
    // v4 :96-98 — the error span shows and `dirty` is never cleared, so the
    // user's text stays saveable rather than being silently dropped.
    const fixture = await render(stubClient({ lantern: 'sepia' }, [], true));
    const lantern = fields(fixture)[0];
    (lantern.querySelector('.qt-formatting-button-h2') as HTMLButtonElement).click();
    await settle(fixture);
    saveButton(lantern).click();
    await settle(fixture);

    expect(lantern.textContent).toContain('store is not available');
    expect(lantern.textContent).not.toContain('Saved');
    expect(saveButton(lantern).disabled).toBe(false);
  });

  it('keeps the two fields independent', async () => {
    // Separate query keys, separate drafts: editing one must not dirty or save
    // the other (v4 gives each field its own component instance and `loadUrl`).
    const seen: DispatchReq[] = [];
    const fixture = await render(stubClient({ lantern: 'sepia', aurora: 'ink' }, seen));
    const [lantern, aurora] = fields(fixture);

    (lantern.querySelector('.qt-formatting-button-h2') as HTMLButtonElement).click();
    await settle(fixture);
    expect(saveButton(lantern).disabled).toBe(false);
    expect(saveButton(aurora).disabled).toBe(true);

    saveButton(lantern).click();
    await settle(fixture);
    expect(seen.filter((r) => r.type === 'systemImageAestheticsSet').map((r) => r['kind'])).toEqual([
      'lantern',
    ]);
  });
});

/**
 * The client surface the card wires to (the Shared contract §2). The empty-save
 * case lives here rather than in the card: driving the editor to empty is a
 * ProseMirror transaction, and what the contract actually pins is that the
 * client sends whatever string it holds.
 */
describe('system-aesthetics.api', () => {
  function core(seen: DispatchReq[], content = ''): CoreClient {
    return {
      dispatchData: (async (req: DispatchReq) => {
        seen.push(req);
        return { content };
      }) as CoreClient['dispatchData'],
    } as CoreClient;
  }

  it('reads `content`, defaulting an absent body to the empty string', async () => {
    const seen: DispatchReq[] = [];
    // A fresh instance with no stored file answers `{content: ''}` by contract.
    expect(await fetchSystemAesthetic(core(seen), 'aurora')).toBe('');
    expect(seen).toEqual([{ type: 'systemImageAestheticsGet', kind: 'aurora' }]);
  });

  it('sends empty content verbatim — the server owns the delete semantics', async () => {
    // An empty save DELETES the stored file and restores the fallback (§2).
    // The client must not second-guess that by skipping the call.
    const seen: DispatchReq[] = [];
    await setSystemAesthetic(core(seen), 'lantern', '');
    expect(seen).toEqual([{ type: 'systemImageAestheticsSet', kind: 'lantern', content: '' }]);
  });
});
