import { ComponentFixture, TestBed } from '@angular/core/testing';
import { By } from '@angular/platform-browser';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../core/core-client';
import type { CharacterListItem } from '../../core/core-contract';
import { RichEditor } from '../../editor/rich-editor';
import { InsertAnnouncementDialog } from './insert-announcement-dialog';
import type { AudienceCandidate } from './post-office.api';
import { ToastService } from '../../ui/toast.service';

function audienceCandidate(over: Partial<AudienceCandidate> = {}): AudienceCandidate {
  return {
    participantId: 'p-cleo',
    name: 'Cleo',
    controlledBy: 'llm',
    avatarUrl: null,
    status: 'active',
    ...over,
  };
}

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
  audienceCandidates: AudienceCandidate[] = [],
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
  fixture.componentRef.setInput('audienceCandidates', audienceCandidates);
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

/** The toast stack this render raised, newest last. */
function toasts(): { type: string; message: string }[] {
  return TestBed.inject(ToastService)
    .toasts()
    .map((t) => ({ type: t.type, message: t.message }));
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

  /**
   * Regression (caught by the real e2e run, not this suite): the audience
   * checkbox group and the off-scene character picker both wore v4's literal
   * `max-h-40` class, so with BOTH visible at once — the common case, since
   * `audienceCandidates` renders independent of `mode` — a `.max-h-40`
   * locator (`salon-post-office-flow.spec.ts`'s pre-existing off-scene-picker
   * beat) resolved to two elements and picked up "Aria" from the audience
   * list instead of the picker it meant to scope to.
   */
  it('the audience group and the off-scene picker never share a `.max-h-40` match', async () => {
    const fixture = await mount(
      stub({ characters: [character({ id: 'c-dax', name: 'Dax' })] }),
      [],
      [audienceCandidate({ participantId: 'p-aria', name: 'Aria' })],
    );
    tab(fixture, 'Off-scene character').click();
    fixture.detectChanges();
    await settle(fixture);
    expect(fixture.nativeElement.querySelectorAll('.max-h-40')).toHaveLength(1);
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
    expect(toasts()).toEqual([
      { type: 'error', message: 'The LLM returned no content. Try again or use as-is.' },
    ]);
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
    expect(toasts().at(-1)?.message).toContain('empty content or unknown chat');
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

  describe('the "Who hears it" audience (v4 `a163862c`)', () => {
    function checkbox(fixture: ComponentFixture<unknown>, name: string): HTMLInputElement {
      const labels = [
        ...fixture.nativeElement.querySelectorAll('[role="group"] label'),
      ] as HTMLLabelElement[];
      const label = labels.find((l) => (l.textContent ?? '').includes(name))!;
      return label.querySelector('input[type="checkbox"]') as HTMLInputElement;
    }

    it('renders no audience section when there are no candidates', async () => {
      const fixture = await mount(stub({}));
      expect(fixture.nativeElement.querySelector('[role="group"]')).toBeNull();
    });

    it('shows every candidate with a (you) tag and a status tag when present', async () => {
      const fixture = await mount(stub({}), [], [
        audienceCandidate({ participantId: 'p-cleo', name: 'Cleo', controlledBy: 'user' }),
        audienceCandidate({ participantId: 'p-dax', name: 'Dax', status: 'silent' }),
        audienceCandidate({ participantId: 'p-aria', name: 'Aria', status: 'active' }),
      ]);
      expect(text(fixture)).toContain('Who hears it');
      expect(checkbox(fixture, 'Cleo').closest('label')!.textContent).toContain('(you)');
      expect(checkbox(fixture, 'Dax').closest('label')!.textContent).toContain('(silent)');
      expect(checkbox(fixture, 'Aria').closest('label')!.textContent).not.toContain('(');
    });

    it('is public by default: helper text says so, and no targetParticipantIds key changes anything', async () => {
      const fixture = await mount(stub({}), [], [audienceCandidate()]);
      expect(text(fixture)).toContain('Everyone present hears this');
      expect(primary(fixture).textContent?.trim()).toBe('Post Announcement');
    });

    it('checking a name turns it into a whisper — label flips, audience sent, distinct toast', async () => {
      const s = stub({});
      const fixture = await mount(s, [], [audienceCandidate({ participantId: 'p-cleo', name: 'Cleo' })]);
      checkbox(fixture, 'Cleo').click();
      fixture.detectChanges();
      expect(text(fixture)).toContain('Whispered to Cleo');
      expect(primary(fixture).textContent?.trim()).toBe('Post Whisper');
      await setEditor(fixture, 'A private word.');
      fixture.detectChanges();
      primary(fixture).click();
      await settle(fixture);
      expect(s.calls.find((c) => c['type'] === 'chatAnnouncementPost')).toMatchObject({
        targetParticipantIds: ['p-cleo'],
      });
      expect(toasts()).toEqual([{ type: 'success', message: 'Whispered announcement posted' }]);
    });

    it('checking two names collects both, in check order', async () => {
      const s = stub({});
      const fixture = await mount(s, [], [
        audienceCandidate({ participantId: 'p-cleo', name: 'Cleo' }),
        audienceCandidate({ participantId: 'p-dax', name: 'Dax' }),
      ]);
      checkbox(fixture, 'Dax').click();
      fixture.detectChanges();
      checkbox(fixture, 'Cleo').click();
      fixture.detectChanges();
      await setEditor(fixture, 'notice');
      fixture.detectChanges();
      primary(fixture).click();
      await settle(fixture);
      expect(s.calls.find((c) => c['type'] === 'chatAnnouncementPost')).toMatchObject({
        targetParticipantIds: ['p-dax', 'p-cleo'],
      });
    });

    it('unchecking a name removes it from the audience', async () => {
      const s = stub({});
      const fixture = await mount(s, [], [audienceCandidate({ participantId: 'p-cleo', name: 'Cleo' })]);
      checkbox(fixture, 'Cleo').click();
      fixture.detectChanges();
      checkbox(fixture, 'Cleo').click();
      fixture.detectChanges();
      expect(text(fixture)).toContain('Everyone present hears this');
      await setEditor(fixture, 'notice');
      fixture.detectChanges();
      primary(fixture).click();
      await settle(fixture);
      expect(s.calls.find((c) => c['type'] === 'chatAnnouncementPost')).toMatchObject({
        targetParticipantIds: null,
      });
    });

    it('"Make it public" clears the audience and returns to compose', async () => {
      const s = stub({
        characters: [character({ id: 'c-bram', name: 'Bram' })],
        profiles: [{ id: 'p1', name: 'Cheap', provider: 'openai', modelName: 'm', isDefault: true }],
      });
      const fixture = await mount(s, [], [audienceCandidate({ participantId: 'p-cleo', name: 'Cleo' })]);
      checkbox(fixture, 'Cleo').click();
      fixture.detectChanges();
      const link = [...fixture.nativeElement.querySelectorAll('button')].find(
        (b: HTMLButtonElement) => b.textContent?.trim() === 'Make it public',
      ) as HTMLButtonElement;
      link.click();
      fixture.detectChanges();
      expect(text(fixture)).toContain('Everyone present hears this');
      expect(primary(fixture).textContent?.trim()).toBe('Post Announcement');
    });

    it('toggling the audience while reviewing an in-character proposal invalidates it and returns to compose', async () => {
      const s = stub({
        characters: [character({ id: 'c-bram', name: 'Bram' })],
        profiles: [{ id: 'p1', name: 'Cheap', provider: 'openai', modelName: 'm', isDefault: true }],
      });
      const fixture = await mount(s, [], [audienceCandidate({ participantId: 'p-cleo', name: 'Cleo' })]);
      tab(fixture, 'Off-scene character').click();
      fixture.detectChanges();
      await settle(fixture);
      (fixture.nativeElement.querySelector('.max-h-40 button') as HTMLButtonElement).click();
      fixture.detectChanges();
      await setEditor(fixture, 'The ridge is clear.');
      fixture.detectChanges();
      primary(fixture).click();
      await settle(fixture);
      expect(text(fixture)).toContain('What Bram will say');

      checkbox(fixture, 'Cleo').click();
      fixture.detectChanges();
      expect(text(fixture)).not.toContain('What Bram will say');
      expect(primary(fixture).textContent?.trim()).toBe('Preview in character');
      expect(s.calls.some((c) => c['type'] === 'chatAnnouncementPost')).toBe(false);
    });

    it('the seed-editor label carries all four v4 combinations of willRewrite × isWhisper', async () => {
      function seedLabel(fixture: ComponentFixture<unknown>): string {
        const labels = [...fixture.nativeElement.querySelectorAll('label')] as HTMLLabelElement[];
        // The seed label is the last plain `qt-text-primary` label before the
        // editor — the audience checkbox labels don't carry that class alone,
        // and this excludes "Sender"/"Staff member"/"Who hears it" by content.
        return (
          labels.find((l) =>
            ['Announcement', 'Whisper', 'What you want the character to announce',
             'What you want the character to say privately'].includes((l.textContent ?? '').trim()),
          )?.textContent ?? ''
        ).trim();
      }

      const s = stub({
        characters: [character({ id: 'c-bram', name: 'Bram' })],
        profiles: [{ id: 'p1', name: 'Cheap', provider: 'openai', modelName: 'm', isDefault: true }],
      });
      // Staff + public → "Announcement".
      const publicStaff = await mount(stub({}), [], [audienceCandidate({ participantId: 'p-cleo', name: 'Cleo' })]);
      expect(seedLabel(publicStaff)).toBe('Announcement');

      // Staff + whisper → "Whisper".
      checkbox(publicStaff, 'Cleo').click();
      publicStaff.detectChanges();
      expect(seedLabel(publicStaff)).toBe('Whisper');

      // Character (will rewrite) + public → "What you want the character to announce".
      const fixture = await mount(s, [], [audienceCandidate({ participantId: 'p-cleo', name: 'Cleo' })]);
      tab(fixture, 'Off-scene character').click();
      fixture.detectChanges();
      await settle(fixture);
      (fixture.nativeElement.querySelector('.max-h-40 button') as HTMLButtonElement).click();
      fixture.detectChanges();
      expect(seedLabel(fixture)).toBe('What you want the character to announce');

      // Character (will rewrite) + whisper → "What you want the character to say privately".
      checkbox(fixture, 'Cleo').click();
      fixture.detectChanges();
      expect(seedLabel(fixture)).toBe('What you want the character to say privately');
    });

    it('preview-in-character carries the audience to chatAnnouncementPreview', async () => {
      const s = stub({
        characters: [character({ id: 'c-bram', name: 'Bram' })],
        profiles: [{ id: 'p1', name: 'Cheap', provider: 'openai', modelName: 'm', isDefault: true }],
      });
      const fixture = await mount(s, [], [audienceCandidate({ participantId: 'p-cleo', name: 'Cleo' })]);
      tab(fixture, 'Off-scene character').click();
      fixture.detectChanges();
      await settle(fixture);
      (fixture.nativeElement.querySelector('.max-h-40 button') as HTMLButtonElement).click();
      fixture.detectChanges();
      checkbox(fixture, 'Cleo').click();
      fixture.detectChanges();
      await setEditor(fixture, 'The ridge is clear.');
      fixture.detectChanges();
      primary(fixture).click();
      await settle(fixture);
      expect(s.calls.find((c) => c['type'] === 'chatAnnouncementPreview')).toMatchObject({
        targetParticipantIds: ['p-cleo'],
      });
    });
  });
});
