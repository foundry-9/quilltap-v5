import { ComponentFixture, TestBed } from '@angular/core/testing';
import { By } from '@angular/platform-browser';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../core/core-client';
import type { CharacterListItem } from '../../core/core-contract';
import { RichEditor } from '../../editor/rich-editor';
import { InsertAnnouncementDialog } from './insert-announcement-dialog';

function character(over: Partial<CharacterListItem> = {}): CharacterListItem {
  return {
    id: 'char-1',
    name: 'Bram',
    title: 'A cautious navigator',
    description: null,
    defaultImageId: null,
    defaultImage: null,
    isFavorite: false,
    controlledBy: 'llm',
    canBeCarina: false,
    defaultConnectionProfileId: null,
    defaultPartnerId: null,
    defaultPartnerName: null,
    defaultTimestampConfig: null,
    defaultScenarioId: null,
    defaultSystemPromptId: null,
    defaultImageProfileId: null,
    npc: false,
    createdAt: '2026-02-01T00:00:00.000Z',
    tags: [],
    updatedAt: '2026-02-01T00:00:00.000Z',
    systemPrompts: [],
    scenarios: [],
    _count: { chats: 0 },
    ...over,
  };
}

interface Stub {
  calls: Record<string, unknown>[];
  client: Partial<CoreClient>;
}

/**
 * `dispatchData` answers the character list, the profile list, and the two §1
 * announcement verbs. `preview` / `post` may be an Error to exercise the failure
 * arms.
 */
function stub(opts: {
  characters?: CharacterListItem[];
  profiles?: Array<Record<string, unknown>>;
  preview?: Record<string, unknown> | Error;
  post?: Record<string, unknown> | Error;
}): Stub {
  const calls: Record<string, unknown>[] = [];
  const dispatchData = vi.fn(async (req: Record<string, unknown>) => {
    calls.push(req);
    switch (req['type']) {
      case 'characterList':
        return { characters: opts.characters ?? [] };
      case 'connectionProfileList':
        return { profiles: opts.profiles ?? [] };
      case 'chatAnnouncementPreview':
        if (opts.preview instanceof Error) throw opts.preview;
        return opts.preview ?? { proposedMarkdown: 'Ahoy, the ridge is clear!' };
      case 'chatAnnouncementPost':
        if (opts.post instanceof Error) throw opts.post;
        return opts.post ?? { success: true };
      default:
        return {};
    }
  });
  return { calls, client: { dispatchData: dispatchData as unknown as CoreClient['dispatchData'] } };
}

async function settle(fixture: ComponentFixture<unknown>): Promise<void> {
  for (let i = 0; i < 8; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

async function mount(
  s: Stub,
  participantCharacterIds: string[] = [],
): Promise<ComponentFixture<InsertAnnouncementDialog>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [InsertAnnouncementDialog],
    providers: [
      provideTanStackQuery(new QueryClient()),
      { provide: CoreClient, useValue: s.client },
    ],
  });
  const fixture = TestBed.createComponent(InsertAnnouncementDialog);
  fixture.componentRef.setInput('chatId', 'chat-1');
  fixture.componentRef.setInput('participantCharacterIds', participantCharacterIds);
  fixture.detectChanges();
  await settle(fixture);
  return fixture;
}

function text(fixture: ComponentFixture<unknown>): string {
  return (fixture.nativeElement.textContent ?? '').replace(/\s+/g, ' ');
}

function tab(fixture: ComponentFixture<unknown>, label: string): HTMLButtonElement {
  const tabs = [...fixture.nativeElement.querySelectorAll('[role="tab"]')] as HTMLButtonElement[];
  return tabs.find((t) => (t.textContent ?? '').trim() === label)!;
}

function primary(fixture: ComponentFixture<unknown>): HTMLButtonElement {
  return fixture.nativeElement.querySelector(
    '[qt-modal-footer] .qt-button-primary',
  ) as HTMLButtonElement;
}

/**
 * Drive the seed editor through the real `qt-markdown-field` → `RichEditor`
 * chain (the `memory-editor.spec.ts` idiom). The seed field is the FIRST one —
 * the preview panel mounts a second once a rewrite lands.
 *
 * The settle is load-bearing: `RichEditor.replaceContent` defers its emit by a
 * microtask (so a `value`-effect write cannot re-enter change detection), so a
 * synchronous click straight after would see the pre-edit state.
 */
async function setEditor(
  fixture: ComponentFixture<InsertAnnouncementDialog>,
  value: string,
): Promise<void> {
  const editors = fixture.debugElement.queryAll(By.directive(RichEditor));
  (editors[0]!.componentInstance as RichEditor).setMarkdown(value);
  await settle(fixture);
}

describe('InsertAnnouncementDialog (v4 components/chat/InsertAnnouncementDialog.tsx)', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('opens on the Staff arm with v4’s ten staff, "The Host" first and selected', async () => {
    const fixture = await mount(stub({}));
    const options = [
      ...fixture.nativeElement.querySelectorAll('#announce-staff option'),
    ] as HTMLOptionElement[];
    expect(options).toHaveLength(10);
    expect(options[0]!.textContent?.trim()).toBe('The Host');
    expect(options[0]!.selected).toBe(true);
    expect(options.map((o) => o.value)).toContain('commonplaceBook');
  });

  it('does NOT load characters until the off-scene arm is chosen (v4’s lazy load)', async () => {
    const s = stub({ characters: [character()] });
    const fixture = await mount(s);
    expect(s.calls.some((c) => c['type'] === 'characterList')).toBe(false);
    tab(fixture, 'Off-scene character').click();
    fixture.detectChanges();
    await settle(fixture);
    expect(s.calls.some((c) => c['type'] === 'characterList')).toBe(true);
  });

  it('loads the connection profiles eagerly on open (v4’s once-per-open effect)', async () => {
    const s = stub({});
    await mount(s);
    expect(s.calls.filter((c) => c['type'] === 'connectionProfileList')).toHaveLength(1);
  });

  it('filters chat participants out of the off-scene picker and sorts by name', async () => {
    const s = stub({
      characters: [
        character({ id: 'c-dax', name: 'Dax' }),
        character({ id: 'c-aria', name: 'Aria' }),
        character({ id: 'c-bram', name: 'Bram' }),
      ],
    });
    const fixture = await mount(s, ['c-aria']);
    tab(fixture, 'Off-scene character').click();
    fixture.detectChanges();
    await settle(fixture);
    const names = [...fixture.nativeElement.querySelectorAll('.font-medium.truncate')].map((n) =>
      (n as HTMLElement).textContent?.trim(),
    );
    expect(names).toEqual(['Bram', 'Dax']);
  });

  it('the primary button posts verbatim for Staff — no preview round-trip', async () => {
    const s = stub({});
    const fixture = await mount(s);
    await setEditor(fixture, '  The airship departs at dawn.  ');
    fixture.detectChanges();
    expect(primary(fixture).textContent?.trim()).toBe('Post Announcement');
    primary(fixture).click();
    await settle(fixture);
    const post = s.calls.find((c) => c['type'] === 'chatAnnouncementPost');
    expect(post).toMatchObject({
      chatId: 'chat-1',
      contentMarkdown: 'The airship departs at dawn.',
      sender: { kind: 'staff', staffId: 'host' },
    });
    expect(s.calls.some((c) => c['type'] === 'chatAnnouncementPreview')).toBe(false);
  });

  it('an LLM character with a default profile previews first, then posts the APPROVED text', async () => {
    const s = stub({
      characters: [character({ id: 'c-bram', name: 'Bram' })],
      profiles: [{ id: 'p1', name: 'Cheap', provider: 'openai', modelName: 'm', isDefault: true }],
    });
    const fixture = await mount(s);
    tab(fixture, 'Off-scene character').click();
    fixture.detectChanges();
    await settle(fixture);
    (fixture.nativeElement.querySelector('.max-h-40 button') as HTMLButtonElement).click();
    fixture.detectChanges();
    await setEditor(fixture, 'The ridge is clear.');
    fixture.detectChanges();
    await settle(fixture);

    // The label flips because a profile (not as-is) resolved by default.
    expect(primary(fixture).textContent?.trim()).toBe('Preview in character');
    primary(fixture).click();
    await settle(fixture);

    const preview = s.calls.find((c) => c['type'] === 'chatAnnouncementPreview');
    expect(preview).toMatchObject({
      chatId: 'chat-1',
      seedMarkdown: 'The ridge is clear.',
      characterId: 'c-bram',
      connectionProfileId: 'p1',
    });
    // Nothing has been posted yet — the operator must approve.
    expect(s.calls.some((c) => c['type'] === 'chatAnnouncementPost')).toBe(false);
    expect(text(fixture)).toContain('What Bram will say');
    expect(fixture.nativeElement.querySelector('[qt-modal-footer]').textContent).toContain(
      'Regenerate',
    );

    primary(fixture).click();
    await settle(fixture);
    expect(s.calls.find((c) => c['type'] === 'chatAnnouncementPost')).toMatchObject({
      contentMarkdown: 'Ahoy, the ridge is clear!',
      sender: { kind: 'character', characterId: 'c-bram' },
    });
  });

  it('a user-controlled character defaults to as-is, so it posts what was typed', async () => {
    const s = stub({
      characters: [character({ id: 'c-cleo', name: 'Cleo', controlledBy: 'user' })],
      profiles: [{ id: 'p1', name: 'Cheap', provider: 'openai', modelName: 'm', isDefault: true }],
    });
    const fixture = await mount(s);
    tab(fixture, 'Off-scene character').click();
    fixture.detectChanges();
    await settle(fixture);
    (fixture.nativeElement.querySelector('.max-h-40 button') as HTMLButtonElement).click();
    fixture.detectChanges();
    await settle(fixture);
    expect(text(fixture)).toContain('Cleo is user-controlled');
    await setEditor(fixture, 'A short notice.');
    fixture.detectChanges();
    expect(primary(fixture).textContent?.trim()).toBe('Post Announcement');
    primary(fixture).click();
    await settle(fixture);
    expect(s.calls.some((c) => c['type'] === 'chatAnnouncementPreview')).toBe(false);
    expect(s.calls.find((c) => c['type'] === 'chatAnnouncementPost')).toMatchObject({
      contentMarkdown: 'A short notice.',
    });
  });

  it('a blank rewrite bounces back to compose with v4’s copy, posting nothing', async () => {
    const s = stub({
      characters: [character({ id: 'c-bram' })],
      profiles: [{ id: 'p1', name: 'Cheap', provider: 'openai', modelName: 'm', isDefault: true }],
      preview: { proposedMarkdown: '   ' },
    });
    const fixture = await mount(s);
    tab(fixture, 'Off-scene character').click();
    fixture.detectChanges();
    await settle(fixture);
    (fixture.nativeElement.querySelector('.max-h-40 button') as HTMLButtonElement).click();
    fixture.detectChanges();
    await setEditor(fixture, 'seed');
    fixture.detectChanges();
    primary(fixture).click();
    await settle(fixture);
    expect(text(fixture)).toContain('The LLM returned no content. Try again or use as-is.');
    expect(primary(fixture).textContent?.trim()).toBe('Preview in character');
    expect(s.calls.some((c) => c['type'] === 'chatAnnouncementPost')).toBe(false);
  });

  it('the custom arm requires a display name and sends it trimmed', async () => {
    const s = stub({});
    const fixture = await mount(s);
    tab(fixture, 'Custom').click();
    fixture.detectChanges();
    await setEditor(fixture, 'A voice from the rafters.');
    fixture.detectChanges();
    // Name still blank → the primary is disabled (v4 canSubmit).
    expect(primary(fixture).disabled).toBe(true);
    const name = fixture.nativeElement.querySelector('#announce-custom-name') as HTMLInputElement;
    name.value = '  The Narrator  ';
    name.dispatchEvent(new Event('input'));
    fixture.detectChanges();
    expect(primary(fixture).disabled).toBe(false);
    primary(fixture).click();
    await settle(fixture);
    expect(s.calls.find((c) => c['type'] === 'chatAnnouncementPost')).toMatchObject({
      sender: { kind: 'custom', displayName: 'The Narrator' },
    });
    // v4 caps the display name at 120 characters in the input itself.
    expect(name.getAttribute('maxlength')).toBe('120');
  });

  it('a failed post surfaces the server’s message and leaves the dialog open', async () => {
    const s = stub({ post: new Error('Failed to post announcement (empty content or unknown chat).') });
    const fixture = await mount(s);
    const closed = vi.fn();
    fixture.componentInstance.close.subscribe(closed);
    await setEditor(fixture, 'notice');
    fixture.detectChanges();
    primary(fixture).click();
    await settle(fixture);
    expect(text(fixture)).toContain('empty content or unknown chat');
    expect(closed).not.toHaveBeenCalled();
  });

  it('emits posted then close on success (v4 onPosted → onClose)', async () => {
    const s = stub({});
    const fixture = await mount(s);
    const seen: string[] = [];
    fixture.componentInstance.posted.subscribe(() => seen.push('posted'));
    fixture.componentInstance.close.subscribe(() => seen.push('close'));
    await setEditor(fixture, 'notice');
    fixture.detectChanges();
    primary(fixture).click();
    await settle(fixture);
    expect(seen).toEqual(['posted', 'close']);
  });

  it('hides the system-prompt picker unless the character has MORE THAN ONE', async () => {
    const one = character({
      id: 'c-bram',
      systemPrompts: [{ id: 's1', name: 'Solo', isDefault: true }],
    });
    const s = stub({
      characters: [one],
      profiles: [{ id: 'p1', name: 'Cheap', provider: 'openai', modelName: 'm', isDefault: true }],
    });
    const fixture = await mount(s);
    tab(fixture, 'Off-scene character').click();
    fixture.detectChanges();
    await settle(fixture);
    (fixture.nativeElement.querySelector('.max-h-40 button') as HTMLButtonElement).click();
    fixture.detectChanges();
    await settle(fixture);
    expect(fixture.nativeElement.querySelector('#announce-prompt')).toBeNull();
  });
});
