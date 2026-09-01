import { ComponentFixture, TestBed } from '@angular/core/testing';
import { describe, expect, it } from 'vitest';

import { CoreClient } from '../../../core/core-client';
import type { ImageLoraSpec, ImageLoraSupport } from '../../../core/core-contract';
import { DEFAULT_SCALE, LoraListEditor, scaleBounds, sourceHint } from './lora-list-editor';
import shapes from './__fixtures__/lora-lookup-shapes.json';

/**
 * Parity spec for the LoRA list editor. The oracle is v4's CLIENT component
 * `components/image-profiles/LoraListEditor.tsx` at `2ece98c90` (the post-query
 * state — `84f33ce94` created it, `2ece98c90` added the Query button and the
 * trigger-phrase adoption).
 *
 * ⚠ The wire is STUBBED here on purpose: `imageProfileLoraMetadata` is
 * P4.D138's, running in a parallel lane, so the query cases drive a scripted
 * `CoreClient` rather than a live server. The shapes it answers with are the
 * RECORDED ones from `lora-lookup-shapes.json` (v4's real lookup at the pin),
 * so the editor is exercised on what the server will actually send.
 */

const SUPPORT: ImageLoraSupport = {
  maxLoras: 2,
  sourceKinds: ['url', 'hf-repo'],
  supportsPrivateWeightsToken: true,
};

interface Dispatched {
  type: string;
  source?: string;
  hfToken?: string;
}

function recorded(name: string): Record<string, unknown> {
  const row = shapes.find((s) => s.name === name);
  if (!row) throw new Error(`no recorded shape named ${name}`);
  return row.result as unknown as Record<string, unknown>;
}

/** A CoreClient whose `imageProfileLoraMetadata` answer is scripted. */
function stubClient(
  answer: Record<string, unknown> | 'reject',
  seen: Dispatched[] = [],
): Partial<CoreClient> {
  return {
    dispatchData: (async (req: Dispatched) => {
      seen.push(req);
      if (req.type === 'imageProfileLoraMetadata') {
        if (answer === 'reject') throw new Error('boom');
        return answer;
      }
      return {};
    }) as unknown as CoreClient['dispatchData'],
  };
}

interface Rendered {
  fixture: ComponentFixture<LoraListEditor>;
  /** Every list the editor has emitted, newest last. */
  emitted: ImageLoraSpec[][];
  /** Re-feed the newest emission back as the input, as the host does. */
  settle: () => void;
}

function render(inputs: {
  support?: ImageLoraSupport | null;
  loras?: ImageLoraSpec[];
  hfToken?: string;
  client?: Partial<CoreClient>;
}): Rendered {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [LoraListEditor],
    providers: [{ provide: CoreClient, useValue: inputs.client ?? stubClient({}) }],
  });
  const fixture = TestBed.createComponent(LoraListEditor);
  const emitted: ImageLoraSpec[][] = [];
  fixture.componentRef.setInput('support', inputs.support === undefined ? SUPPORT : inputs.support);
  fixture.componentRef.setInput('loras', inputs.loras ?? []);
  fixture.componentRef.setInput('hfToken', inputs.hfToken);
  fixture.componentInstance.lorasChange.subscribe((l) => emitted.push(l));
  fixture.detectChanges();
  return {
    fixture,
    emitted,
    settle: () => {
      if (emitted.length > 0) {
        fixture.componentRef.setInput('loras', emitted[emitted.length - 1]);
      }
      fixture.detectChanges();
    },
  };
}

function text(fixture: ComponentFixture<unknown>): string {
  return ((fixture.nativeElement as HTMLElement).textContent ?? '').replace(/\s+/g, ' ').trim();
}

function all<T extends HTMLElement>(fixture: ComponentFixture<unknown>, selector: string): T[] {
  return Array.from((fixture.nativeElement as HTMLElement).querySelectorAll<T>(selector));
}

function el<T extends HTMLElement>(fixture: ComponentFixture<unknown>, selector: string): T | null {
  return (fixture.nativeElement as HTMLElement).querySelector<T>(selector);
}

/**
 * The i-th adapter row's own container. Asserting on the whole panel's text
 * cannot tell WHICH row an answer is sitting under, and "under the wrong
 * address" is the exact failure the re-index exists to prevent — so the
 * re-index cases scope every assertion to a row.
 */
function row(fixture: ComponentFixture<unknown>, index: number): HTMLElement {
  const rows = all<HTMLElement>(fixture, 'div.rounded.border.p-3');
  const found = rows[index];
  if (!found) throw new Error(`no row at index ${index}`);
  return found;
}

/** Whether row `index` is showing a query answer, and for which repository. */
function answerIn(fixture: ComponentFixture<unknown>, index: number): string | null {
  const panel = row(fixture, index).querySelector('qt-lora-query-result');
  return panel ? (panel.textContent ?? '').replace(/\s+/g, ' ').trim() : null;
}

/** Type into a text input the way a person does. */
function type(input: HTMLInputElement, value: string): void {
  input.value = value;
  input.dispatchEvent(new Event('input'));
}

async function flush(fixture: ComponentFixture<unknown>): Promise<void> {
  for (let i = 0; i < 4; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

describe('sourceHint (v4 `:66-75`)', () => {
  const withKinds = (kinds: ImageLoraSupport['sourceKinds']): ImageLoraSupport => ({
    maxLoras: 1,
    sourceKinds: kinds,
  });

  it('capitalises a lone kind, and only a lone kind', () => {
    expect(sourceHint(withKinds(['url']))).toBe('A .safetensors URL.');
    expect(sourceHint(withKinds(['hf-repo']))).toBe('A HuggingFace owner/model-name.');
    expect(sourceHint(withKinds(['provider-id']))).toBe(
      "An identifier from the provider's own catalogue.",
    );
  });

  it('joins two with "or", leaving the first lowercase', () => {
    expect(sourceHint(withKinds(['url', 'hf-repo']))).toBe(
      'a .safetensors URL or a HuggingFace owner/model-name.',
    );
  });

  it('joins three with commas and a final "or" — no Oxford comma', () => {
    expect(sourceHint(withKinds(['url', 'hf-repo', 'provider-id']))).toBe(
      "a .safetensors URL, a HuggingFace owner/model-name or an identifier from the provider's own catalogue.",
    );
  });

  it('renders in the declaration order v4 hard-codes, not the array order', () => {
    // v4 tests the three kinds in a fixed sequence, so a provider declaring
    // them backwards still reads the same way.
    expect(sourceHint(withKinds(['provider-id', 'url']))).toBe(
      "a .safetensors URL or an identifier from the provider's own catalogue.",
    );
  });

  it('stands in when a provider names no kind at all', () => {
    expect(sourceHint(withKinds([]))).toBe('Whatever identifier this provider accepts.');
  });
});

describe('scaleBounds (v4 `:53-63`)', () => {
  it('falls back whole when the provider declares no scale', () => {
    expect(scaleBounds({ maxLoras: 1, sourceKinds: [] })).toEqual(DEFAULT_SCALE);
  });

  it('takes a declared scale but defaults only the step', () => {
    expect(
      scaleBounds({ maxLoras: 1, sourceKinds: [], scale: { min: 0.1, max: 1.5, default: 0.8 } }),
    ).toEqual({ min: 0.1, max: 1.5, default: 0.8, step: 0.05 });
  });

  it('honours a declared step', () => {
    expect(
      scaleBounds({
        maxLoras: 1,
        sourceKinds: [],
        scale: { min: 0, max: 1, default: 0.5, step: 0.25 },
      }),
    ).toEqual({ min: 0, max: 1, default: 0.5, step: 0.25 });
  });

  it('the default mirrors the server constant', () => {
    expect(DEFAULT_SCALE).toEqual({ min: 0, max: 2, default: 1, step: 0.05 });
  });
});

describe('LoraListEditor visibility', () => {
  it('renders NOTHING at all when the model resolves no support (v4 `:81`)', () => {
    const { fixture } = render({ support: null, loras: [{ source: 'a/b' }] });
    expect(text(fixture)).toBe('');
  });

  it('renders the heading and the capacity sentence when it does', () => {
    const { fixture } = render({});
    expect(text(fixture)).toContain('LoRA Adapters (Optional)');
    expect(text(fixture)).toContain(
      'Adapters are applied in the order listed. a .safetensors URL or a HuggingFace owner/model-name. This model accepts up to 2 adapters.',
    );
  });

  it('says "a single adapter" rather than "up to 1 adapters"', () => {
    const { fixture } = render({ support: { maxLoras: 1, sourceKinds: ['hf-repo'] } });
    expect(text(fixture)).toContain('This model accepts a single adapter.');
    expect(text(fixture)).not.toContain('up to 1');
  });

  it('names the empty state', () => {
    const { fixture } = render({});
    expect(text(fixture)).toContain(
      'No adapters attached — the model generates in its own native manner.',
    );
  });

  it('...and drops it once a row exists', () => {
    const { fixture } = render({ loras: [{ source: 'a/b' }] });
    expect(text(fixture)).not.toContain('No adapters attached');
  });
});

describe('LoraListEditor rows', () => {
  it('numbers adapters from 1 and appends a label when there is one', () => {
    const { fixture } = render({ loras: [{ source: 'a/b' }, { source: 'c/d', label: 'Realism' }] });
    const body = text(fixture);
    expect(body).toContain('Adapter 1');
    expect(body).toContain('Adapter 2 — Realism');
  });

  it('shows the scale to two places, defaulting to the model’s own', () => {
    const { fixture } = render({ loras: [{ source: 'a/b' }, { source: 'c/d', scale: 0.7 }] });
    const body = text(fixture);
    expect(body).toContain('Strength — 1.00');
    expect(body).toContain('Strength — 0.70');
  });

  it('names the bounds beneath the slider', () => {
    const { fixture } = render({ loras: [{ source: 'a/b' }] });
    expect(text(fixture)).toContain("0 to 2; this model's own default is 1.");
  });

  it('the slider carries only §B’s shared class — this editor ships no slider CSS', () => {
    const { fixture } = render({ loras: [{ source: 'a/b' }] });
    const slider = el<HTMLInputElement>(fixture, 'input[type=range]');
    expect(slider?.getAttribute('class')).toBe('qt-range w-full');
  });

  it('carries the two help sentences under Source and Trigger Phrase', () => {
    const { fixture } = render({ loras: [{ source: 'a/b' }] });
    const body = text(fixture);
    expect(body).toContain(
      'Querying asks HuggingFace what it declares about this adapter — its base model, its weights, its magic word. It settles nothing about whether the two of you will get along.',
    );
    expect(body).toContain(
      'Many adapters answer only to a magic word. Whatever you put here is woven into the prompt on every generation that uses this profile.',
    );
  });

  it('tallies the rows against the cap', () => {
    const { fixture } = render({ loras: [{ source: 'a/b' }] });
    expect(text(fixture)).toContain('1 of 2');
  });
});

describe('LoraListEditor over-cap rows', () => {
  const OVER: ImageLoraSpec[] = [{ source: 'a/b' }, { source: 'c/d' }, { source: 'e/f' }];

  it('warns on the row beyond the limit, byte for byte (v4 `:177-182`)', () => {
    const { fixture } = render({ support: { maxLoras: 2, sourceKinds: [] }, loras: OVER });
    expect(text(fixture)).toContain(
      "Beyond this model's limit of 2 — kept on the profile, but left behind on every request until you remove an earlier adapter or return to a model that takes more.",
    );
  });

  it('warns on exactly the rows past the cap, and no others', () => {
    const { fixture } = render({ support: { maxLoras: 2, sourceKinds: [] }, loras: OVER });
    expect(all(fixture, 'p.qt-text-warning')).toHaveLength(1);
    // The row is flagged, never deleted: switching the model back must lose
    // nothing, and the request builder caps the list again at generation time.
    expect(all(fixture, 'input[type=range]')).toHaveLength(3);
  });

  it('marks the over-cap row’s border and leaves the others alone', () => {
    const { fixture } = render({ support: { maxLoras: 2, sourceKinds: [] }, loras: OVER });
    expect(all(fixture, 'div.qt-border-warning')).toHaveLength(1);
  });

  it('disables Add at the cap, and says why', () => {
    const { fixture } = render({
      support: { maxLoras: 2, sourceKinds: [] },
      loras: [{ source: 'a/b' }, { source: 'c/d' }],
    });
    const add = all<HTMLButtonElement>(fixture, 'button').find(
      (b) => b.textContent?.trim() === 'Add LoRA',
    );
    expect(add?.disabled).toBe(true);
    expect(add?.getAttribute('title')).toBe('This model accepts at most 2');
  });

  it('...and offers it below the cap', () => {
    const { fixture } = render({ loras: [{ source: 'a/b' }] });
    const add = all<HTMLButtonElement>(fixture, 'button').find(
      (b) => b.textContent?.trim() === 'Add LoRA',
    );
    expect(add?.disabled).toBe(false);
    expect(add?.getAttribute('title')).toBe('Attach another adapter');
  });
});

describe('LoraListEditor editing', () => {
  it('adds a blank row at the model’s own default scale (v4 `:117-119`)', () => {
    const { emitted, fixture } = render({ loras: [] });
    all<HTMLButtonElement>(fixture, 'button')
      .find((b) => b.textContent?.trim() === 'Add LoRA')!
      .click();
    expect(emitted.at(-1)).toEqual([{ source: '', scale: 1 }]);
  });

  it('...using a DECLARED default when the provider names one', () => {
    const { emitted, fixture } = render({
      support: { maxLoras: 3, sourceKinds: [], scale: { min: 0, max: 1, default: 0.6 } },
      loras: [],
    });
    all<HTMLButtonElement>(fixture, 'button')
      .find((b) => b.textContent?.trim() === 'Add LoRA')!
      .click();
    expect(emitted.at(-1)).toEqual([{ source: '', scale: 0.6 }]);
  });

  it('patches one row and leaves its siblings identical', () => {
    const { emitted, fixture } = render({
      loras: [{ source: 'a/b', scale: 0.5 }, { source: 'c/d' }],
    });
    type(all<HTMLInputElement>(fixture, 'input[type=text]')[0], 'x/y');
    expect(emitted.at(-1)).toEqual([{ source: 'x/y', scale: 0.5 }, { source: 'c/d' }]);
  });

  it('an emptied Trigger Phrase becomes undefined, not "" (v4 `:242`)', () => {
    const { emitted, fixture } = render({ loras: [{ source: 'a/b', triggerPhrase: 'ohwx' }] });
    const triggers = all<HTMLInputElement>(fixture, 'input[type=text]');
    type(triggers[1], '');
    expect(emitted.at(-1)?.[0].triggerPhrase).toBeUndefined();
  });

  it('removes the row it was asked to and no other', () => {
    const { emitted, fixture } = render({
      loras: [{ source: 'a/b' }, { source: 'c/d' }, { source: 'e/f' }],
    });
    all<HTMLButtonElement>(fixture, 'button')
      .filter((b) => b.textContent?.trim() === 'Remove')[1]
      .click();
    expect(emitted.at(-1)).toEqual([{ source: 'a/b' }, { source: 'e/f' }]);
  });

  it('emits an EMPTY list when the last row goes — the host deletes the key', () => {
    const { emitted, fixture } = render({ loras: [{ source: 'a/b' }] });
    all<HTMLButtonElement>(fixture, 'button')
      .find((b) => b.textContent?.trim() === 'Remove')!
      .click();
    expect(emitted.at(-1)).toEqual([]);
  });
});

describe('LoraListEditor query button', () => {
  it('is offered only when a repository can be read out of the source', () => {
    const { fixture } = render({
      loras: [{ source: 'owner/name' }, { source: 'https://cdn.example.com/w.safetensors' }],
    });
    const buttons = all<HTMLButtonElement>(fixture, 'button').filter(
      (b) => b.textContent?.trim() === 'Query',
    );
    expect(buttons).toHaveLength(2);
    expect(buttons[0].disabled).toBe(false);
    expect(buttons[0].getAttribute('title')).toBe('Ask HuggingFace about owner/name');
    expect(buttons[1].disabled).toBe(true);
    expect(buttons[1].getAttribute('title')).toBe(
      'Only a HuggingFace owner/model-name (or a huggingface.co address) can be looked up',
    );
  });

  it('sends the source in the request BODY, with no token when there is none', async () => {
    const seen: Dispatched[] = [];
    const r = render({
      loras: [{ source: 'owner/name' }],
      client: stubClient(recorded('realism-lora'), seen),
    });
    all<HTMLButtonElement>(r.fixture, 'button')
      .find((b) => b.textContent?.trim() === 'Query')!
      .click();
    await flush(r.fixture);
    expect(seen).toEqual([{ type: 'imageProfileLoraMetadata', source: 'owner/name' }]);
  });

  it('carries the profile’s hf_api_token when it has one', async () => {
    const seen: Dispatched[] = [];
    const r = render({
      loras: [{ source: 'owner/name' }],
      hfToken: 'hf_secret',
      client: stubClient(recorded('realism-lora'), seen),
    });
    all<HTMLButtonElement>(r.fixture, 'button')
      .find((b) => b.textContent?.trim() === 'Query')!
      .click();
    await flush(r.fixture);
    expect(seen).toEqual([
      { type: 'imageProfileLoraMetadata', source: 'owner/name', hfToken: 'hf_secret' },
    ]);
  });

  it('shows the result panel once the answer lands', async () => {
    const r = render({
      loras: [{ source: 'XLabs-AI/flux-RealismLora' }],
      client: stubClient(recorded('realism-lora')),
    });
    all<HTMLButtonElement>(r.fixture, 'button')
      .find((b) => b.textContent?.trim() === 'Query')!
      .click();
    await flush(r.fixture);
    const body = text(r.fixture);
    expect(body).toContain('HuggingFace says');
    expect(body).toContain('Tagged a LoRA adapter.');
  });

  it('collapses a failed REQUEST into the same panel as a failed lookup (v4 `:134-141`)', async () => {
    const r = render({ loras: [{ source: 'owner/name' }], client: stubClient('reject') });
    all<HTMLButtonElement>(r.fixture, 'button')
      .find((b) => b.textContent?.trim() === 'Query')!
      .click();
    await flush(r.fixture);
    // From the reader's chair they are the same disappointment.
    expect(text(r.fixture)).toContain(
      'HuggingFace could not be reached at all. Check that this machine can see the outside world.',
    );
  });

  it('adopts the declared trigger phrase into THIS row when offered', async () => {
    const r = render({
      loras: [{ source: 'Datou1111/shou_xin' }, { source: 'other/one' }],
      client: stubClient(recorded('trigger-phrase')),
    });
    all<HTMLButtonElement>(r.fixture, 'button')
      .filter((b) => b.textContent?.trim() === 'Query')[0]
      .click();
    await flush(r.fixture);
    all<HTMLButtonElement>(r.fixture, 'button')
      .find((b) => b.textContent?.trim() === 'Use it')!
      .click();
    expect(r.emitted.at(-1)).toEqual([
      { source: 'Datou1111/shou_xin', triggerPhrase: 'shou_xin, pencil sketch' },
      { source: 'other/one' },
    ]);
  });

  it('passes the model’s token capability down to the panel', async () => {
    const r = render({
      support: { maxLoras: 2, sourceKinds: [], supportsPrivateWeightsToken: true },
      loras: [{ source: 'owner/name' }],
      client: stubClient(recorded('gated')),
    });
    all<HTMLButtonElement>(r.fixture, 'button')
      .find((b) => b.textContent?.trim() === 'Query')!
      .click();
    await flush(r.fixture);
    expect(text(r.fixture)).toContain(
      'The selected model accepts one — see the HuggingFace Token field in the options above.',
    );
  });

  it('...and says so when it has none (an absent flag is not a yes)', async () => {
    const r = render({
      support: { maxLoras: 2, sourceKinds: [] },
      loras: [{ source: 'owner/name' }],
      client: stubClient(recorded('gated')),
    });
    all<HTMLButtonElement>(r.fixture, 'button')
      .find((b) => b.textContent?.trim() === 'Query')!
      .click();
    await flush(r.fixture);
    expect(text(r.fixture)).toContain(
      'The selected model has nowhere to put one, so these weights are unlikely to load.',
    );
  });
});

/**
 * The two stale-answer mechanics. Both exist because rows are keyed by
 * POSITION, and a fact sitting beside the wrong address is worse than no fact.
 */
describe('LoraListEditor stale-answer mechanics', () => {
  async function queryRow(r: Rendered, which: number): Promise<void> {
    all<HTMLButtonElement>(r.fixture, 'button')
      .filter((b) => b.textContent?.trim() === 'Query')[which]
      .click();
    await flush(r.fixture);
  }

  it('editing a row’s Source DISCARDS its answer (v4 `:88-97`)', async () => {
    const r = render({
      loras: [{ source: 'XLabs-AI/flux-RealismLora' }],
      client: stubClient(recorded('realism-lora')),
    });
    await queryRow(r, 0);
    expect(text(r.fixture)).toContain('HuggingFace says');

    type(all<HTMLInputElement>(r.fixture, 'input[type=text]')[0], 'someone/else');
    r.settle();
    expect(text(r.fixture)).not.toContain('HuggingFace says');
  });

  it('...but editing the scale or the trigger phrase does NOT', async () => {
    const r = render({
      loras: [{ source: 'XLabs-AI/flux-RealismLora' }],
      client: stubClient(recorded('realism-lora')),
    });
    await queryRow(r, 0);

    const slider = el<HTMLInputElement>(r.fixture, 'input[type=range]')!;
    slider.value = '1.5';
    slider.dispatchEvent(new Event('input'));
    r.settle();
    expect(text(r.fixture)).toContain('HuggingFace says');

    type(all<HTMLInputElement>(r.fixture, 'input[type=text]')[1], 'ohwx');
    r.settle();
    expect(text(r.fixture)).toContain('HuggingFace says');
  });

  it('removing a row RE-INDEXES the answers below it (v4 `:101-115`)', async () => {
    // Row 1 is the only one queried. Removing row 0 must carry its findings
    // DOWN to index 0 with it — a naive delete would leave them stranded at
    // key 1, where the (now shorter) list never reads them, and a naive
    // shift-up would surface them under the wrong address.
    const r = render({
      loras: [{ source: 'first/one' }, { source: 'Datou1111/shou_xin' }],
      client: stubClient(recorded('trigger-phrase')),
    });
    await queryRow(r, 1);
    expect(text(r.fixture)).toContain('shou_xin, pencil sketch');

    all<HTMLButtonElement>(r.fixture, 'button')
      .filter((b) => b.textContent?.trim() === 'Remove')[0]
      .click();
    r.settle();

    // One row left, and it is the queried one — with its answer still beside
    // it, and beside the right Source.
    expect(all(r.fixture, 'input[type=range]')).toHaveLength(1);
    expect(all<HTMLInputElement>(r.fixture, 'input[type=text]')[0].value).toBe(
      'Datou1111/shou_xin',
    );
    expect(answerIn(r.fixture, 0)).toContain('shou_xin, pencil sketch');
  });

  it('removing a queried row takes its answer with it, sparing the row above', async () => {
    const r = render({
      loras: [{ source: 'first/one' }, { source: 'Datou1111/shou_xin' }],
      client: stubClient(recorded('trigger-phrase')),
    });
    await queryRow(r, 1);

    all<HTMLButtonElement>(r.fixture, 'button')
      .filter((b) => b.textContent?.trim() === 'Remove')[1]
      .click();
    r.settle();

    expect(all(r.fixture, 'input[type=range]')).toHaveLength(1);
    expect(answerIn(r.fixture, 0)).toBeNull();
    expect(text(r.fixture)).not.toContain('shou_xin, pencil sketch');
  });

  it('an answer above the removed row keeps its own index', async () => {
    const r = render({
      loras: [{ source: 'Datou1111/shou_xin' }, { source: 'second/one' }, { source: 'third/one' }],
      support: { maxLoras: 3, sourceKinds: [] },
      client: stubClient(recorded('trigger-phrase')),
    });
    await queryRow(r, 0);

    all<HTMLButtonElement>(r.fixture, 'button')
      .filter((b) => b.textContent?.trim() === 'Remove')[2]
      .click();
    r.settle();

    expect(all<HTMLInputElement>(r.fixture, 'input[type=text]')[0].value).toBe(
      'Datou1111/shou_xin',
    );
    // Scoped, not page-wide: a shifted answer would still put this phrase
    // SOMEWHERE on the page, under the wrong Source. That is the bug.
    expect(answerIn(r.fixture, 0)).toContain('shou_xin, pencil sketch');
    expect(answerIn(r.fixture, 1)).toBeNull();
  });
});
