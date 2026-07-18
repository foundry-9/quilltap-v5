import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../../core/core-client';
import { SettingsTagsCard } from './tags-tab';
import { DEFAULT_TAG_STYLE, mergeWithDefaultTagStyle, type SettingsTag } from './tags.api';

/**
 * The global Tags card (v4 `components/settings/tags-tab.tsx`). The copy, the
 * usage/confirm pluralization, and the quick-hide authoring flow are the
 * behaviors worth pinning — plus the style merge, which has three different
 * fallback rules in one function.
 */

interface DispatchReq {
  type: string;
  tagId?: string;
  tag?: Record<string, unknown>;
}

function tag(over: Partial<SettingsTag>): SettingsTag {
  return { id: 't1', name: 'Alpha', quickHide: false, visualStyle: null, totalUsage: 0, ...over };
}

function stubClient(
  tags: SettingsTag[],
  opts: { seen?: DispatchReq[]; fail?: string } = {},
): Partial<CoreClient> {
  return {
    dispatchData: (async (req: DispatchReq) => {
      opts.seen?.push(req);
      if (opts.fail === req.type) throw new Error('boom');
      if (req.type === 'tagList') return { tags };
      if (req.type === 'tagUpdate') {
        const base = tags.find((t) => t.id === req.tagId) ?? tag({});
        return { tag: { ...base, ...(req.tag ?? {}) } };
      }
      return {};
    }) as CoreClient['dispatchData'],
  };
}

async function render(client: Partial<CoreClient>): Promise<ComponentFixture<SettingsTagsCard>> {
  TestBed.configureTestingModule({
    imports: [SettingsTagsCard],
    providers: [
      provideTanStackQuery(new QueryClient()),
      { provide: CoreClient, useValue: client },
    ],
  });
  const fixture = TestBed.createComponent(SettingsTagsCard);
  fixture.detectChanges();
  for (let i = 0; i < 6; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
  return fixture;
}

function text(fixture: ComponentFixture<unknown>): string {
  return (fixture.nativeElement as HTMLElement).textContent ?? '';
}

function buttonByText(fixture: ComponentFixture<unknown>, label: string): HTMLButtonElement {
  const found = Array.from(
    (fixture.nativeElement as HTMLElement).querySelectorAll('button'),
  ).find((b) => (b.textContent ?? '').trim().startsWith(label)) as HTMLButtonElement | undefined;
  if (!found) throw new Error(`no button "${label}"`);
  return found;
}

// ---------------------------------------------------------------------------

describe('mergeWithDefaultTagStyle (v4 lib/tags/styles.ts:13-27)', () => {
  it('returns the defaults for a nullish style (v4 :14-16)', () => {
    expect(mergeWithDefaultTagStyle(null)).toEqual(DEFAULT_TAG_STYLE);
    expect(mergeWithDefaultTagStyle(undefined)).toEqual(DEFAULT_TAG_STYLE);
  });

  it('keeps emoji only when a NON-EMPTY string (v4 :19)', () => {
    expect(mergeWithDefaultTagStyle({ emoji: '🎩' }).emoji).toBe('🎩');
    expect(mergeWithDefaultTagStyle({ emoji: '' }).emoji).toBeNull();
    expect(mergeWithDefaultTagStyle({ emoji: null }).emoji).toBeNull();
  });

  it('falls back colors with || so an empty string reverts (v4 :20-21)', () => {
    expect(mergeWithDefaultTagStyle({ foregroundColor: '' }).foregroundColor).toBe(
      DEFAULT_TAG_STYLE.foregroundColor,
    );
    expect(mergeWithDefaultTagStyle({ backgroundColor: '#000000' }).backgroundColor).toBe('#000000');
  });

  it('falls back flags with ?? so an explicit false is KEPT (v4 :22-25)', () => {
    // The distinction that matters: `false ?? default` is false, `false || default`
    // would wrongly become the default.
    expect(mergeWithDefaultTagStyle({ bold: false }).bold).toBe(false);
    expect(mergeWithDefaultTagStyle({ bold: true }).bold).toBe(true);
    expect(mergeWithDefaultTagStyle({}).bold).toBe(false);
  });
});

describe('SettingsTagsCard', () => {
  beforeEach(() => TestBed.resetTestingModule());
  afterEach(() => vi.restoreAllMocks());

  it('renders both section headings and blurbs verbatim (v4 :256-259,:430-433)', async () => {
    const fixture = await render(stubClient([tag({})]));
    const t = text(fixture);
    expect(t).toContain('Tag Appearance');
    expect(t).toContain(
      'Map tags to custom emojis and colors. Tags without a custom style use the default gray border/background and show only the tag name.',
    );
    expect(t).toContain('Tag Management');
    expect(t).toContain(
      'View all tags and their usage across your workspace. Delete tags you no longer need — they will be removed from all entities that use them.',
    );
  });

  it('shows the no-styles empty state (v4 :420-424)', async () => {
    const fixture = await render(stubClient([tag({})]));
    expect(text(fixture)).toContain(
      'No custom tag styles yet. Select a tag above to add an emoji and colors.',
    );
  });

  it('shows the no-tags empty state (v4 :471-475)', async () => {
    const fixture = await render(stubClient([]));
    expect(text(fixture)).toContain(
      'No tags exist yet. Tags are created when you assign them to characters, profiles, or other content.',
    );
  });

  it('pluralizes usage (v4 :444-446)', async () => {
    const fixture = await render(
      stubClient([
        tag({ id: 'a', name: 'Once', totalUsage: 1 }),
        tag({ id: 'b', name: 'Thrice', totalUsage: 3 }),
        tag({ id: 'c', name: 'Never', totalUsage: 0 }),
      ]),
    );
    const t = text(fixture);
    expect(t).toContain('Used 1 time');
    expect(t).toContain('Used 3 times');
    expect(t).toContain('Unused');
  });

  it('sorts the Management rows by name while the picker keeps server order (v4 :437)', async () => {
    const fixture = await render(
      stubClient([tag({ id: 'z', name: 'Zeta' }), tag({ id: 'a', name: 'Alpha' })]),
    );
    const rows = Array.from(
      (fixture.nativeElement as HTMLElement).querySelectorAll('.divide-y > div'),
    );
    expect(rows[0]?.textContent).toContain('Alpha');
    expect(rows[1]?.textContent).toContain('Zeta');
  });

  it('partitions styled vs unstyled tags (v4 :234-243)', async () => {
    const fixture = await render(
      stubClient([
        tag({ id: 'styled', name: 'Styled', visualStyle: { bold: true } }),
        tag({ id: 'plain', name: 'Plain', visualStyle: null }),
      ]),
    );
    // The unstyled one is offered in the picker...
    const options = Array.from(
      (fixture.nativeElement as HTMLElement).querySelectorAll('option'),
    ).map((o) => o.textContent?.trim());
    expect(options).toContain('Plain');
    expect(options).not.toContain('Styled');
    // ...and the styled one has a card (its Remove Style button exists).
    expect(text(fixture)).toContain('Remove Style');
  });

  describe('the quick-hide authoring checkbox (v4 :368-382)', () => {
    function quickHideBox(fixture: ComponentFixture<unknown>): HTMLInputElement {
      const label = Array.from(
        (fixture.nativeElement as HTMLElement).querySelectorAll('label'),
      ).find((l) => (l.textContent ?? '').includes('Enable quick-hide button'));
      return label?.querySelector('input[type="checkbox"]') as HTMLInputElement;
    }

    it('renders v4’s label and subtext verbatim (v4 :377,:380)', async () => {
      const fixture = await render(
        stubClient([tag({ visualStyle: { bold: false } })]),
      );
      const t = text(fixture);
      expect(t).toContain('Enable quick-hide button');
      expect(t).toContain('Adds this tag to the navbar quick-hide controls.');
    });

    it('sends the quickHide patch NESTED under `tag` (the engine shape)', async () => {
      const seen: DispatchReq[] = [];
      const fixture = await render(
        stubClient([tag({ id: 't1', visualStyle: { bold: false } })], { seen }),
      );
      quickHideBox(fixture).click();
      await new Promise((r) => setTimeout(r, 0));
      fixture.detectChanges();

      const update = seen.find((r) => r.type === 'tagUpdate');
      expect(update).toBeDefined();
      expect(update?.tagId).toBe('t1');
      // NOT flat: the engine variant is `TagUpdate { tag_id, tag: Value }`.
      expect(update?.tag).toEqual({ quickHide: true });
    });

    it('refreshes the quick-hide list after a successful save (v4 :188)', async () => {
      const seen: DispatchReq[] = [];
      const fixture = await render(
        stubClient([tag({ id: 't1', visualStyle: { bold: false } })], { seen }),
      );
      seen.length = 0;
      quickHideBox(fixture).click();
      await new Promise((r) => setTimeout(r, 0));
      fixture.detectChanges();

      // The service's refresh re-reads the tag list so the menu updates live.
      expect(seen.filter((r) => r.type === 'tagList').length).toBeGreaterThan(0);
    });

    it('surfaces a failed save (v4 toasts; v5 alerts)', async () => {
      const fixture = await render(
        stubClient([tag({ id: 't1', visualStyle: { bold: false } })], { fail: 'tagUpdate' }),
      );
      quickHideBox(fixture).click();
      await new Promise((r) => setTimeout(r, 0));
      fixture.detectChanges();
      expect(text(fixture)).toContain('boom');
    });
  });

  describe('delete (v4 :200-223,:451-465)', () => {
    it('asks for confirmation naming the blast radius (v4 :459-462)', async () => {
      const fixture = await render(stubClient([tag({ id: 't1', name: 'Alpha', totalUsage: 2 })]));
      buttonByText(fixture, 'Delete').click();
      fixture.detectChanges();
      expect(text(fixture)).toContain('Delete "Alpha"? It will be removed from 2 items.');
    });

    it('omits the blast radius when unused (v4 :462)', async () => {
      const fixture = await render(stubClient([tag({ id: 't1', name: 'Alpha', totalUsage: 0 })]));
      buttonByText(fixture, 'Delete').click();
      fixture.detectChanges();
      const t = text(fixture);
      expect(t).toContain('Delete "Alpha"?');
      expect(t).not.toContain('It will be removed from');
    });

    it('Cancel closes the confirm without deleting (v4 :47)', async () => {
      const seen: DispatchReq[] = [];
      const fixture = await render(stubClient([tag({ id: 't1', name: 'Alpha' })], { seen }));
      buttonByText(fixture, 'Delete').click();
      fixture.detectChanges();
      buttonByText(fixture, 'Cancel').click();
      fixture.detectChanges();

      expect(text(fixture)).not.toContain('Delete "Alpha"?');
      expect(seen.some((r) => r.type === 'tagDelete')).toBe(false);
    });

    it('confirming dispatches tagDelete', async () => {
      const seen: DispatchReq[] = [];
      const fixture = await render(stubClient([tag({ id: 't1', name: 'Alpha' })], { seen }));
      buttonByText(fixture, 'Delete').click();
      fixture.detectChanges();
      // The confirm's own Delete is the second one now on screen.
      const confirm = Array.from(
        (fixture.nativeElement as HTMLElement).querySelectorAll('button'),
      ).find((b) => b.classList.contains('qt-button-destructive')) as HTMLButtonElement;
      confirm.click();
      await new Promise((r) => setTimeout(r, 0));
      fixture.detectChanges();

      expect(seen.find((r) => r.type === 'tagDelete')?.tagId).toBe('t1');
    });
  });

  it('Add Style writes the DEFAULT style for the picked tag (v4 :160-166)', async () => {
    const seen: DispatchReq[] = [];
    const fixture = await render(stubClient([tag({ id: 'plain', name: 'Plain' })], { seen }));

    const select = (fixture.nativeElement as HTMLElement).querySelector(
      'select',
    ) as HTMLSelectElement;
    select.value = 'plain';
    select.dispatchEvent(new Event('change'));
    fixture.detectChanges();

    buttonByText(fixture, 'Add Style').click();
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();

    const update = seen.find((r) => r.type === 'tagUpdate');
    expect(update?.tagId).toBe('plain');
    expect(update?.tag).toEqual({ visualStyle: DEFAULT_TAG_STYLE });
  });
});
