/**
 * The name-chip markers. The first four describes TRANSCRIBE v4
 * `__tests__/unit/lib/chat/mention-markers.test.ts:21-89` at `236c04e35`; the
 * last covers v5's post-render pass (`MENTION_MARKERS_POST_RENDER`, ruling R-2)
 * over v5's REAL renderer, with v4 `MentionChip.test.tsx:46-82`'s shapes.
 */
import { describe, expect, it } from 'vitest';

import { renderMarkdownToHtml } from '../render/markdown-renderer';
import {
  MENTION_CLOSE,
  MENTION_MID,
  MENTION_OPEN,
  assignMentionsToRuns,
  markMentionsInHtml,
  rehypeMentionMarkers,
  splitMentionMarkers,
  type HastNode,
} from './mention-markers';
import { buildNameMatcher, firstNameMatch, lastNameMatch } from './name-mentions';

const el = (tagName: string, ...children: HastNode[]): HastNode => ({
  type: 'element',
  tagName,
  children,
});
const text = (value: string): HastNode => ({ type: 'text', value });
const mark = (i: number, t: string) => `${MENTION_OPEN}${i}${MENTION_MID}${t}${MENTION_CLOSE}`;

describe('name matcher', () => {
  const m = buildNameMatcher(['Abigail', 'Abby', 'Mrs. Sebold'])!;

  it('matches whole names, case-sensitively', () => {
    expect(firstNameMatch('ask Abby now', m)).toEqual({ index: 4, text: 'Abby' });
    expect(firstNameMatch('abby', m)).toBeNull();
    expect(firstNameMatch('Abbyville', m)).toBeNull();
    expect(firstNameMatch('Ask Mrs. Sebold.', m)?.text).toBe('Mrs. Sebold');
  });

  it('finds the last match', () => {
    expect(lastNameMatch('Abby, then Abigail', m)).toEqual({ index: 11, text: 'Abigail' });
  });

  it('is null with no usable names', () => {
    expect(buildNameMatcher(['', '  ', null])).toBeNull();
  });
});

describe('rehypeMentionMarkers', () => {
  it('marks the first eligible occurrence of each mention, once', () => {
    const tree = el(
      'root',
      el('p', text('Vergil and Abby; Abby again.')),
      el('p', text('Vergil later.')),
    );
    rehypeMentionMarkers([{ text: 'Abby' }, { text: 'Vergil' }])(tree);
    expect(tree.children![0].children![0].value).toBe(
      `${mark(1, 'Vergil')} and ${mark(0, 'Abby')}; Abby again.`,
    );
    expect(tree.children![1].children![0].value).toBe('Vergil later.');
  });

  it('skips code, emphasis and other non-block parents, taking the next eligible one', () => {
    const tree = el(
      'root',
      el('p', el('code', text('Abby')), el('em', text('Abby')), text(' then Abby')),
    );
    rehypeMentionMarkers([{ text: 'Abby' }])(tree);
    const p = tree.children![0];
    expect(p.children![0].children![0].value).toBe('Abby');
    expect(p.children![1].children![0].value).toBe('Abby');
    expect(p.children![2].value).toBe(` then ${mark(0, 'Abby')}`);
  });

  it('marks inside list items and blockquotes', () => {
    const tree = el('root', el('ul', el('li', text('Greg came'))));
    rehypeMentionMarkers([{ text: 'Greg' }])(tree);
    expect(tree.children![0].children![0].children![0].value).toBe(`${mark(0, 'Greg')} came`);
  });
});

describe('splitMentionMarkers', () => {
  it('returns plain text unchanged', () => {
    expect(splitMentionMarkers('no marks')).toEqual(['no marks']);
  });

  it('splits marked runs out', () => {
    expect(splitMentionMarkers(`"When ${mark(2, 'Abby')} scolds."`)).toEqual([
      '"When ',
      { index: 2, text: 'Abby' },
      ' scolds."',
    ]);
  });
});

describe('assignMentionsToRuns', () => {
  it('gives each mention to the first prose run that names it', () => {
    const out = assignMentionsToRuns(
      ['Hello.', null, 'Abby came. Greg too.', 'Abby again.'],
      [{ text: 'Abby' }, { text: 'Greg' }],
    );
    expect(out).toEqual([undefined, undefined, [{ text: 'Abby' }, { text: 'Greg' }], undefined]);
  });
});

describe('markMentionsInHtml — the post-render pass over v5’s real renderer', () => {
  const abby = { text: 'Abby', card: { name: 'Abigail' } };
  const host = (html: string): HTMLElement => {
    const div = document.createElement('div');
    div.innerHTML = html;
    return div;
  };

  it('chips the first mention inside the dialogue styling, and only once (v4 MentionChip.test.tsx:47-58)', () => {
    const html = renderMarkdownToHtml('"When Abby is not scolding Vergil." Abby grins.');
    const out = host(markMentionsInHtml(html, [abby]));
    const chips = out.querySelectorAll('.qt-mention-chip');
    expect(chips).toHaveLength(1);
    expect(chips[0].textContent).toBe('Abby');
    // The quote is still one styled dialogue run, with the chip inside it.
    const dialogue = chips[0].closest('.qt-chat-dialogue');
    expect(dialogue?.textContent).toBe('"When Abby is not scolding Vergil."');
    expect(out.textContent).not.toMatch(/[-]/);
  });

  it('emits v4’s chip bytes, the name HTML-escaped into the label', () => {
    const out = host(
      markMentionsInHtml(renderMarkdownToHtml('Ask Abby.'), [
        { text: 'Abby', card: { name: 'Abby "<Doc>"' } },
      ]),
    );
    const chip = out.querySelector('.qt-mention-chip')!;
    expect(chip.getAttribute('tabindex')).toBe('0');
    expect(chip.getAttribute('role')).toBe('button');
    expect(chip.getAttribute('aria-label')).toBe('Abby "<Doc>", not in this conversation');
    expect(chip.getAttribute('data-mention')).toBe('0');
  });

  it('skips a name inside code, emphasis and math, taking the next eligible one', () => {
    const html = renderMarkdownToHtml('`Abby` and *Abby*\n\n$$\n\\text{Abby}\n$$\n\nthen Abby.');
    const out = host(markMentionsInHtml(html, [abby]));
    expect(out.querySelector('.katex')).not.toBeNull();
    const chips = out.querySelectorAll('.qt-mention-chip');
    expect(chips).toHaveLength(1);
    expect(chips[0].closest('code, em, .katex, .katex-display')).toBeNull();
    expect(chips[0].closest('p')?.textContent).toBe('then Abby.');
  });

  it('a name in a roleplay span nested in emphasis is skipped on both sides (the R-2 divergence shape)', () => {
    // v4: the text's parent is `em`, never an eligible parent. v5: an `em` on
    // the path makes the node ineligible. Same verdict.
    const html = renderMarkdownToHtml('*"Abby, come here."* Abby came.');
    const out = host(markMentionsInHtml(html, [abby]));
    const chips = out.querySelectorAll('.qt-mention-chip');
    expect(chips).toHaveLength(1);
    expect(chips[0].closest('em')).toBeNull();
  });

  it('marks inside a list item', () => {
    const out = host(
      markMentionsInHtml(renderMarkdownToHtml('- Greg came\n- Greg left'), [
        { text: 'Greg', card: { name: 'Gregory' } },
      ]),
    );
    const chips = out.querySelectorAll('li .qt-mention-chip');
    expect(chips).toHaveLength(1);
    expect(chips[0].closest('li')?.textContent).toBe('Greg came');
  });

  it('with no mentions returns the very same string (no parse)', () => {
    const html = renderMarkdownToHtml('Abby came by.');
    expect(markMentionsInHtml(html, [])).toBe(html);
  });

  it('re-marking is idempotent — a previous pass’s chips are unwrapped first', () => {
    const once = markMentionsInHtml(renderMarkdownToHtml('Abby came by.'), [abby]);
    const twice = markMentionsInHtml(once, [abby]);
    expect(host(twice).querySelectorAll('.qt-mention-chip')).toHaveLength(1);
  });
});
