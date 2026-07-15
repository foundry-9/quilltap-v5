import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it } from 'vitest';

import { LLMInspectorEntry } from './llm-inspector-entry';
import type { LlmLogDto } from './llm-logs.api';

function log(overrides: Partial<LlmLogDto> = {}): LlmLogDto {
  return {
    id: 'log-1',
    userId: 'user-1',
    type: 'CHAT_MESSAGE',
    provider: 'openai',
    modelName: 'gpt-4o',
    request: { messageCount: 2, messages: [], toolCount: 0 },
    response: { content: 'Well met!', contentLength: 9 },
    createdAt: '2026-02-01T12:34:56.000Z',
    updatedAt: '2026-02-01T12:34:56.000Z',
    ...overrides,
  };
}

function render(l: LlmLogDto, highlighted = false): ComponentFixture<LLMInspectorEntry> {
  // Reset first so a test may render more than once (TestBed refuses to
  // reconfigure an instantiated module).
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({ imports: [LLMInspectorEntry] });
  const fixture = TestBed.createComponent(LLMInspectorEntry);
  fixture.componentRef.setInput('log', l);
  fixture.componentRef.setInput('isHighlighted', highlighted);
  fixture.detectChanges();
  return fixture;
}

/** The DOM strips inter-element whitespace differently than JSX; normalize. */
function text(el: Element | null): string {
  return (el?.textContent ?? '').replace(/\s+/g, ' ').trim();
}

/**
 * The label→value rows of the detail tabs, keyed by label. Read per-row rather
 * than from a flattened textContent: v4 renders "Provider:" and its value as
 * ADJACENT spans, so the concatenated text carries no separator ("Provider:openai")
 * and a substring assertion would be testing DOM serialization, not the port.
 */
function rows(fixture: ComponentFixture<LLMInspectorEntry>): Record<string, string> {
  const out: Record<string, string> = {};
  for (const div of Array.from(
    fixture.nativeElement.querySelectorAll('.flex.justify-between'),
  ) as HTMLElement[]) {
    const spans = div.querySelectorAll('span');
    if (spans.length < 2) continue;
    out[text(spans[0]).replace(/:$/, '')] = text(spans[1]);
  }
  return out;
}

function summaryButton(fixture: ComponentFixture<LLMInspectorEntry>): HTMLButtonElement {
  return fixture.nativeElement.querySelector('button[aria-expanded]');
}

function expand(fixture: ComponentFixture<LLMInspectorEntry>): void {
  summaryButton(fixture).click();
  fixture.detectChanges();
}

function tab(fixture: ComponentFixture<LLMInspectorEntry>, label: string): void {
  const button = (
    Array.from(fixture.nativeElement.querySelectorAll('button')) as HTMLButtonElement[]
  ).find((b) => b.textContent?.trim() === label);
  button!.click();
  fixture.detectChanges();
}

describe('LLMInspectorEntry — the scroll hook (v4 LLMInspectorPanel.tsx:78)', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('exposes data-log-id — the attribute the panel’s scroll-to querySelector uses', () => {
    const fixture = render(log({ id: 'log-abc' }));
    expect(fixture.nativeElement.querySelector('[data-log-id="log-abc"]')).not.toBeNull();
  });

  it('omits the highlight class when not highlighted (v4 :70)', () => {
    const entry = render(log(), false).nativeElement.querySelector('.qt-inspector-entry');
    expect(entry.classList.contains('qt-inspector-entry-highlight')).toBe(false);
  });

  it('adds the highlight class when highlighted — the CSS keyframes hook', () => {
    const entry = render(log(), true).nativeElement.querySelector('.qt-inspector-entry');
    expect(entry.classList.contains('qt-inspector-entry-highlight')).toBe(true);
  });
});

describe('LLMInspectorEntry — the type badge (v4 :14-42, :64-65, :85-87)', () => {
  afterEach(() => TestBed.resetTestingModule());

  function badge(l: LlmLogDto): HTMLElement {
    return render(l).nativeElement.querySelectorAll('button[aria-expanded] > span')[1];
  }

  it.each([
    ['CHAT_MESSAGE', 'Chat', 'qt-bg-primary/15'],
    ['TOOL_CONTINUATION', 'Tool', 'qt-bg-primary/15'],
    ['MEMORY_EXTRACTION', 'Memory', 'qt-bg-info/15'],
    ['TITLE_GENERATION', 'Title', 'qt-bg-secondary/50'],
    ['SUMMARIZATION', 'Summary', 'qt-bg-secondary/50'],
    ['CONTEXT_COMPRESSION', 'Compress', 'qt-bg-secondary/50'],
    ['IMAGE_PROMPT_CRAFTING', 'Img Prompt', 'qt-bg-warning/15'],
    ['IMAGE_DESCRIPTION', 'Img Desc', 'qt-bg-warning/15'],
    ['APPEARANCE_RESOLUTION', 'Appearance', 'qt-bg-warning/15'],
    ['DANGER_CLASSIFICATION', 'Safety', 'qt-bg-destructive/15'],
    ['CHARACTER_WIZARD', 'Wizard', 'qt-bg-success/15'],
    ['AI_IMPORT', 'Import', 'qt-bg-success/15'],
  ])('labels and tints %s as "%s"', (type, label, tint) => {
    const el = badge(log({ type }));
    expect(text(el)).toBe(label);
    expect(el.className).toContain(tint);
  });

  it('falls back to the RAW type string + muted tint for a type outside the table', () => {
    // v4's tables cover 12 of the 19 LLMLogTypeEnum members; the rest land here
    // BY DESIGN (v4 :64-65) — this is ported behavior, not a gap to fill in.
    const el = badge(log({ type: 'SCENE_STATE_TRACKING' }));
    expect(text(el)).toBe('SCENE_STATE_TRACKING');
    expect(el.className).toContain('qt-bg-muted');
    expect(el.className).toContain('qt-text-secondary');
  });
});

describe('LLMInspectorEntry — the collapsed row (v4 :73-127)', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('renders provider/modelName', () => {
    const fixture = render(log({ provider: 'anthropic', modelName: 'claude-opus' }));
    expect(text(summaryButton(fixture))).toContain('anthropic/claude-opus');
  });

  it('renders the token summary with the → arrow, both counts localized (v4 :44-47)', () => {
    const fixture = render(
      log({ usage: { promptTokens: 12000, completionTokens: 3400, totalTokens: 15400 } }),
    );
    expect(text(summaryButton(fixture))).toContain('12,000 → 3,400');
  });

  it('omits the token summary when usage is absent', () => {
    expect(text(summaryButton(render(log())))).not.toContain('→');
  });

  it('renders the duration with ONE decimal (v4 :49-52)', () => {
    expect(text(summaryButton(render(log({ durationMs: 1234 }))))).toContain('1.2s');
  });

  it('omits the duration when durationMs is null (v4 :105 `!= null`)', () => {
    expect(text(summaryButton(render(log({ durationMs: null }))))).not.toContain('s');
  });

  it('renders a zero duration — 0 is data, and v4’s gate is `!= null`, not truthiness', () => {
    expect(text(summaryButton(render(log({ durationMs: 0 }))))).toContain('0.0s');
  });

  it('renders the message-link icon with v4’s title when messageId is set (v4 :112-116)', () => {
    const fixture = render(log({ messageId: 'm-1' }));
    const linked = fixture.nativeElement.querySelector('span[title="Linked to a message"]');
    expect(linked).not.toBeNull();
    expect(linked.querySelector('qt-icon').getAttribute('name')).toBe('chat');
  });

  it('omits the message-link icon for a chat-level log', () => {
    expect(
      render(log({ messageId: null })).nativeElement.querySelector(
        'span[title="Linked to a message"]',
      ),
    ).toBeNull();
  });

  it('titles the error icon with the error TEXT itself (v4 :120)', () => {
    const fixture = render(
      log({ response: { content: '', contentLength: 0, error: 'Rate limited' } }),
    );
    const errorSpan = fixture.nativeElement.querySelector('span[title="Rate limited"]');
    expect(errorSpan).not.toBeNull();
    expect(errorSpan.querySelector('qt-icon').getAttribute('name')).toBe('alert-circle');
  });

  it('rotates the chevron only when expanded (v4 :126)', () => {
    const fixture = render(log());
    const chevron = fixture.nativeElement.querySelector('qt-icon[name="chevron-down"]');
    expect(chevron.className).not.toContain('rotate-180');
    expand(fixture);
    expect(chevron.className).toContain('rotate-180');
  });
});

describe('LLMInspectorEntry — expansion is lazy (v4 :130)', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('renders no tabs until expanded, and reports state via aria-expanded', () => {
    const fixture = render(log());
    expect(summaryButton(fixture).getAttribute('aria-expanded')).toBe('false');
    expect(fixture.nativeElement.textContent).not.toContain('Request');

    expand(fixture);
    expect(summaryButton(fixture).getAttribute('aria-expanded')).toBe('true');
    expect(fixture.nativeElement.textContent).toContain('Request');
  });

  it('capitalizes the three tab labels (v4 :134, :145)', () => {
    const fixture = render(log());
    expand(fixture);
    const labels = (
      Array.from(fixture.nativeElement.querySelectorAll('.border-b-2')) as HTMLElement[]
    ).map((b) => b.textContent!.trim());
    expect(labels).toEqual(['Request', 'Response', 'Usage']);
  });

  it('opens on the Request tab', () => {
    const fixture = render(log());
    expand(fixture);
    expect(fixture.nativeElement.textContent).toContain('Provider:');
  });
});

describe('LLMInspectorEntry — the Request tab (v4 :162-213)', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('renders the provider/model/type and count blocks', () => {
    const fixture = render(
      log({
        type: 'CHAT_MESSAGE',
        provider: 'openai',
        modelName: 'gpt-4o',
        request: { messageCount: 3, messages: [], toolCount: 5, temperature: 0.7, maxTokens: 2048 },
      }),
    );
    expand(fixture);
    expect(rows(fixture)).toMatchObject({
      Provider: 'openai',
      Model: 'gpt-4o',
      Type: 'CHAT_MESSAGE',
      Messages: '3',
      Temperature: '0.7',
      'Max Tokens': '2048',
      Tools: '5',
    });
  });

  it('shows "default" for a null temperature and maxTokens (v4 :188, :194)', () => {
    const fixture = render(
      log({
        request: {
          messageCount: 1,
          messages: [],
          toolCount: 0,
          temperature: null,
          maxTokens: null,
        },
      }),
    );
    expand(fixture);
    expect(rows(fixture)['Temperature']).toBe('default');
    expect(rows(fixture)['Max Tokens']).toBe('default');
  });

  it('renders a temperature of 0 as 0, not "default" — v4’s gate is `!= null`', () => {
    const fixture = render(
      log({ request: { messageCount: 1, messages: [], toolCount: 0, temperature: 0 } }),
    );
    expand(fixture);
    expect(rows(fixture)['Temperature']).toBe('0');
  });

  it('renders each prompt message with role, char count and the attachments marker (v4 :277-284)', () => {
    const fixture = render(
      log({
        request: {
          messageCount: 2,
          toolCount: 0,
          messages: [
            {
              role: 'system',
              content: 'You are a pilot.',
              contentLength: 16,
              hasAttachments: false,
            },
            { role: 'user', content: 'Hello', contentLength: 5, hasAttachments: true },
          ],
        },
      }),
    );
    expand(fixture);
    const blocks = fixture.nativeElement.querySelectorAll('qt-llm-inspector-message-block');
    expect(blocks.length).toBe(2);
    expect(text(blocks[0])).toContain('system');
    expect(text(blocks[0])).toContain('16 chars');
    expect(text(blocks[0])).not.toContain('(attachments)');
    expect(text(blocks[1])).toContain('5 chars (attachments)');
    expect(text(blocks[1])).toContain('Hello');
  });

  it('falls back content → contentPreview for a message body (v4 :207)', () => {
    const fixture = render(
      log({
        request: {
          messageCount: 1,
          toolCount: 0,
          messages: [
            {
              role: 'user',
              content: '',
              contentPreview: 'old preview',
              contentLength: 11,
              hasAttachments: false,
            },
          ],
        },
      }),
    );
    expand(fixture);
    expect(text(fixture.nativeElement.querySelector('qt-llm-inspector-message-block'))).toContain(
      'old preview',
    );
  });
});

describe('LLMInspectorEntry — the Response tab (v4 :215-240)', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('shows the success banner and the char-count heading', () => {
    const fixture = render(log({ response: { content: 'Well met!', contentLength: 9 } }));
    expand(fixture);
    tab(fixture, 'Response');
    expect(text(fixture.nativeElement)).toContain('Request completed successfully');
    expect(text(fixture.nativeElement)).toContain('Response (9 chars)');
  });

  it('shows the error block INSTEAD of the success banner when the log errored', () => {
    const fixture = render(
      log({ response: { content: '', contentLength: 0, error: 'Context length exceeded' } }),
    );
    expand(fixture);
    tab(fixture, 'Response');
    const body = text(fixture.nativeElement);
    expect(body).toContain('Error');
    expect(body).toContain('Context length exceeded');
    expect(body).not.toContain('Request completed successfully');
  });

  it('falls back content → fullContent → contentPreview (v4 :216-217, backward compat)', () => {
    // Old log entries predate the full-content field; dropping the chain would
    // blank the body on every historical entry.
    const fixture = render(
      log({ response: { content: '', fullContent: 'from fullContent', contentLength: 16 } }),
    );
    expand(fixture);
    tab(fixture, 'Response');
    expect(text(fixture.nativeElement.querySelector('pre'))).toBe('from fullContent');

    const older = render(
      log({
        response: {
          content: '',
          fullContent: null,
          contentPreview: 'from preview',
          contentLength: 12,
        },
      }),
    );
    expand(older);
    tab(older, 'Response');
    expect(text(older.nativeElement.querySelector('pre'))).toBe('from preview');
  });
});

describe('LLMInspectorEntry — the 500-char truncation (v4 :243-299)', () => {
  afterEach(() => TestBed.resetTestingModule());

  const long = 'x'.repeat(1200);

  it('leaves a body at or under 500 chars untouched, with no toggle', () => {
    const fixture = render(log({ response: { content: 'y'.repeat(500), contentLength: 500 } }));
    expand(fixture);
    tab(fixture, 'Response');
    expect(text(fixture.nativeElement.querySelector('pre')).length).toBe(500);
    expect(fixture.nativeElement.textContent).not.toContain('Show all');
  });

  it('truncates a longer response to 500 chars + an ellipsis and offers "Show all N chars"', () => {
    const fixture = render(log({ response: { content: long, contentLength: 1200 } }));
    expand(fixture);
    tab(fixture, 'Response');
    expect(text(fixture.nativeElement.querySelector('pre'))).toBe('x'.repeat(500) + '...');
    expect(fixture.nativeElement.textContent).toContain('Show all 1,200 chars');
  });

  it('shows the whole body after Show all, then collapses again on Show less', () => {
    const fixture = render(log({ response: { content: long, contentLength: 1200 } }));
    expand(fixture);
    tab(fixture, 'Response');
    tab(fixture, 'Show all 1,200 chars');
    expect(text(fixture.nativeElement.querySelector('pre')).length).toBe(1200);
    expect(fixture.nativeElement.textContent).toContain('Show less');
    tab(fixture, 'Show less');
    expect(text(fixture.nativeElement.querySelector('pre')).length).toBe(503);
  });

  it('truncates message blocks on the SAME threshold', () => {
    const fixture = render(
      log({
        request: {
          messageCount: 1,
          toolCount: 0,
          messages: [{ role: 'user', content: long, contentLength: 1200, hasAttachments: false }],
        },
      }),
    );
    expand(fixture);
    expect(fixture.nativeElement.textContent).toContain('Show all 1,200 chars');
  });

  it('counts the LOGGED contentLength in a message block’s copy, not the rendered body (v4 :294)', () => {
    // v4's block reports the prop while the response body reports the rendered
    // string's own length; they disagree when the body fell back to a shorter
    // contentPreview. Carried as written.
    const fixture = render(
      log({
        request: {
          messageCount: 1,
          toolCount: 0,
          messages: [{ role: 'user', content: long, contentLength: 99999, hasAttachments: false }],
        },
      }),
    );
    expand(fixture);
    expect(fixture.nativeElement.textContent).toContain('Show all 99,999 chars');
  });
});

describe('LLMInspectorEntry — the Usage tab (v4 :301-354)', () => {
  afterEach(() => TestBed.resetTestingModule());

  function usage(l: LlmLogDto): ComponentFixture<LLMInspectorEntry> {
    const fixture = render(l);
    expand(fixture);
    tab(fixture, 'Usage');
    return fixture;
  }

  function usageBody(l: LlmLogDto): string {
    return text(usage(l).nativeElement);
  }

  it('renders the three-column token grid, localized', () => {
    const fixture = usage(
      log({ usage: { promptTokens: 12000, completionTokens: 3400, totalTokens: 15400 } }),
    );
    const cells = (
      Array.from(fixture.nativeElement.querySelectorAll('.text-center')) as HTMLElement[]
    ).map((c) => Array.from(c.querySelectorAll('p')).map((p) => text(p)));
    expect(cells).toEqual([
      ['12,000', 'Prompt'],
      ['3,400', 'Completion'],
      ['15,400', 'Total'],
    ]);
  });

  it('renders the duration with TWO decimals here — the collapsed row uses one (v4 :51 vs :342)', () => {
    expect(rows(usage(log({ durationMs: 1234 })))['Duration']).toBe('1.23s');
  });

  it('renders each cacheUsage row only when its key is DEFINED (v4 :323, :330)', () => {
    const r = rows(usage(log({ cacheUsage: { cacheReadInputTokens: 2048 } })));
    expect(r['Cache Read']).toBe('2,048 tokens');
    expect(r['Cache Creation']).toBeUndefined();
  });

  it('renders a cache row whose value is 0 — the gate is `!== undefined`, not truthiness', () => {
    expect(
      rows(usage(log({ cacheUsage: { cacheCreationInputTokens: 0 } })))['Cache Creation'],
    ).toBe('0 tokens');
  });

  it('shows the fallback copy only when usage, cacheUsage and durationMs are ALL absent (v4 :347)', () => {
    expect(usageBody(log())).toContain('No usage data available for this log');
  });

  it('suppresses the fallback when any one of the three is present', () => {
    expect(usageBody(log({ durationMs: 500 }))).not.toContain('No usage data available');
    expect(
      usageBody(log({ usage: { promptTokens: 1, completionTokens: 1, totalTokens: 2 } })),
    ).not.toContain('No usage data available');
  });
});
