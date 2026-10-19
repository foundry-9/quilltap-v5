import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it } from 'vitest';

import { CoreClient } from '../../core/core-client';
import { draftFromDefinition, newDraft, type ToolDraft } from '../../pascal/tool-draft';
import * as bench from './proving-bench';
import { ProvingBench, formatShort, stateToken } from './proving-bench';

/**
 * ProvingBench — asserted against v4 `ProvingBench.tsx`. The load-bearing claims:
 * the bench NEVER resolves a fact sheet itself (it sends one of two forms and
 * lets the server hydrate), it renders the SERVER's error strings verbatim, the
 * character list rides the DESTINATIONS response rather than a roster call, and
 * the audit's run count is whatever the server reports.
 */

interface Req {
  type: string;
  [k: string]: unknown;
}

function stubClient(
  hooks: {
    onDispatch?: (req: Req) => void;
    preview?: unknown;
    audit?: unknown;
    destinations?: unknown;
    throws?: Error;
  } = {},
): Partial<CoreClient> {
  return {
    dispatchData: (async (req: Req) => {
      hooks.onDispatch?.(req);
      if (hooks.throws) throw hooks.throws;
      if (req.type === 'customToolPreview') return hooks.preview ?? {};
      if (req.type === 'customToolAudit') return hooks.audit ?? {};
      if (req.type === 'customToolsDestinations') return hooks.destinations ?? { characters: [] };
      return {};
    }) as CoreClient['dispatchData'],
  };
}

function validDraft(): ToolDraft {
  return draftFromDefinition({
    name: 'unlock',
    description: 'Pick the lock.',
    outcomes: [
      { when: { gte: 0.5 }, message: 'Open.', state: 'success' },
      { when: true, message: 'Stuck.', state: 'failure' },
    ],
  })!;
}

async function render(
  draft: ToolDraft,
  client: Partial<CoreClient>,
  valid = true,
): Promise<ComponentFixture<ProvingBench>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [ProvingBench],
    providers: [{ provide: CoreClient, useValue: client }],
  });
  const fixture = TestBed.createComponent(ProvingBench);
  fixture.componentRef.setInput('draft', draft);
  fixture.componentRef.setInput('valid', valid);
  fixture.detectChanges();
  return fixture;
}

function text(fixture: ComponentFixture<unknown>): string {
  return (fixture.nativeElement as HTMLElement).textContent ?? '';
}

describe('formatShort (v4 ProvingBench:341-344)', () => {
  it('prints integers bare and trims float noise to four significant digits', () => {
    expect(formatShort(7)).toBe('7');
    expect(formatShort(0.123456789)).toBe('0.1235');
    expect(formatShort(1234567)).toBe('1234567');
  });
});

describe('stateToken (v4 ProvingBench:347-358)', () => {
  it('maps outcome states onto the --qt-alert-* families', () => {
    expect(stateToken('success')).toBe('success');
    expect(stateToken('partial')).toBe('warning');
    expect(stateToken('failure')).toBe('error');
    expect(stateToken('info')).toBe('info');
    expect(stateToken(undefined)).toBe('info');
  });
});

describe('ProvingBench (v4 ProvingBench.tsx)', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('starts in hand-typed mode with an empty object', async () => {
    const fixture = await render(validDraft(), stubClient());
    expect(fixture.componentInstance.sheet()).toEqual({ mode: 'manual', text: '{}' });
    expect(fixture.componentInstance.manualSheetError()).toBeNull();
  });

  it('rejects a hand-typed sheet that is not a single JSON object', async () => {
    const fixture = await render(validDraft(), stubClient());
    const bench = fixture.componentInstance;

    bench.sheet.set({ mode: 'manual', text: '{oops' });
    expect(bench.manualSheetError()).toBe('The fact sheet is not valid JSON.');

    bench.sheet.set({ mode: 'manual', text: '[1,2]' });
    expect(bench.manualSheetError()).toBe('The fact sheet must be a single JSON object.');

    bench.sheet.set({ mode: 'manual', text: '"nope"' });
    expect(bench.manualSheetError()).toBe('The fact sheet must be a single JSON object.');

    bench.sheet.set({ mode: 'manual', text: '{"strength": 12}' });
    expect(bench.manualSheetError()).toBeNull();
  });

  it('blocks the bench while the draft is invalid, or the sheet will not parse', async () => {
    const invalid = await render(validDraft(), stubClient(), false);
    expect(invalid.componentInstance.benchDisabled()).toBe(true);

    const fixture = await render(validDraft(), stubClient(), true);
    expect(fixture.componentInstance.benchDisabled()).toBe(false);
    fixture.componentInstance.sheet.set({ mode: 'manual', text: '{oops' });
    expect(fixture.componentInstance.benchDisabled()).toBe(true);
  });

  it('sends a hand-typed sheet through VERBATIM and never resolves it', async () => {
    const seen: Req[] = [];
    const fixture = await render(
      validDraft(),
      stubClient({ onDispatch: (r) => seen.push(r), preview: previewResult() }),
    );
    fixture.componentInstance.sheet.set({ mode: 'manual', text: '{"strength": 12}' });
    await fixture.componentInstance.roll();

    const req = seen.find((r) => r.type === 'customToolPreview');
    expect(req?.['metadata']).toEqual({ strength: 12 });
  });

  it('sends {characterId} for the character mode — the SERVER hydrates the sheet', async () => {
    const seen: Req[] = [];
    const fixture = await render(
      validDraft(),
      stubClient({ onDispatch: (r) => seen.push(r), preview: previewResult() }),
    );
    fixture.componentRef.setInput('vantage', { kind: 'character', characterId: 'char-1' });
    await fixture.componentInstance.roll();

    const req = seen.find((r) => r.type === 'customToolPreview');
    expect(req?.['metadata']).toEqual({ characterId: 'char-1' });
    // No roster/character read of its own — the bench does not resolve sheets.
    expect(seen.some((r) => r.type === 'characterGet' || r.type === 'characterList')).toBe(false);
  });

  it('sends no metadata when the character mode has nobody picked', async () => {
    const seen: Req[] = [];
    const fixture = await render(
      validDraft(),
      stubClient({ onDispatch: (r) => seen.push(r), preview: previewResult() }),
    );
    fixture.componentInstance.sheet.set({ mode: 'character' });
    await fixture.componentInstance.roll();

    expect(seen.find((r) => r.type === 'customToolPreview')?.['metadata']).toBeUndefined();
  });

  it('draws the character list from the DESTINATIONS response, not a roster call', async () => {
    const seen: Req[] = [];
    const fixture = await render(
      validDraft(),
      stubClient({
        onDispatch: (r) => seen.push(r),
        destinations: { characters: [{ characterId: 'c1', characterName: 'Aria' }] },
      }),
    );
    await fixture.componentInstance.pickCharacterMode();

    expect(seen.map((r) => r.type)).toEqual(['customToolsDestinations']);
    expect(fixture.componentInstance.characters()).toEqual([{ id: 'c1', name: 'Aria' }]);
  });

  it('reports the matched outcome id so the form can flash the row', async () => {
    const draft = validDraft();
    const fixture = await render(
      draft,
      stubClient({ preview: previewResult({ outcomeIndex: 1 }) }),
    );
    let matched: string | null | undefined;
    fixture.componentInstance.matched.subscribe((id) => (matched = id));

    await fixture.componentInstance.roll();
    expect(matched).toBe(draft.outcomes[1].id);
  });

  it('keeps at most ten rolls, newest first', async () => {
    const fixture = await render(validDraft(), stubClient({ preview: previewResult() }));
    for (let i = 0; i < 12; i++) await fixture.componentInstance.roll();
    expect(fixture.componentInstance.rolls()).toHaveLength(10);
  });

  it('renders the SERVER error string verbatim — never a re-derived one', async () => {
    const fixture = await render(
      validDraft(),
      stubClient({ throws: new Error('outcomes.0.when: must test something') }),
    );
    await fixture.componentInstance.roll();
    fixture.detectChanges();

    expect(fixture.componentInstance.rollError()).toBe('outcomes.0.when: must test something');
    expect(text(fixture)).toContain('outcomes.0.when: must test something');
  });

  it('renders the audit hit table off the server-reported run count', async () => {
    const fixture = await render(
      validDraft(),
      stubClient({
        audit: {
          runs: 10000,
          outcomes: [
            { index: 0, hits: 5012, share: 0.5012 },
            { index: 1, hits: 4988, share: 0.4988 },
          ],
          valueMin: 0,
          valueMax: 0.9999,
          valueMean: 0.4996,
        },
      }),
    );
    await fixture.componentInstance.runAudit();
    fixture.detectChanges();

    const body = text(fixture);
    expect(body).toContain('10,000 draws');
    expect(body).toContain('50.1%');
    expect(body).toContain('49.9%');
    expect(body).toContain('row 1');
    expect(body).toContain('otherwise');
  });

  /**
   * P4.38 migrated these meters onto the shared `qt-progress` family (v4
   * `ProvingBench.tsx:476-490`). The per-outcome colour is the interesting half:
   * it must reach the fill through the family's OWN indicator variable, not a
   * `background-color`, so a theme restyling `qt-progress` restyles these too.
   */
  it('renders the outcome meters on the shared qt-progress family', async () => {
    const fixture = await render(
      validDraft(),
      stubClient({
        audit: {
          runs: 10000,
          outcomes: [
            { index: 0, hits: 5012, share: 0.5012 },
            { index: 1, hits: 4988, share: 0.4988 },
          ],
          valueMin: 0,
          valueMax: 0.9999,
          valueMean: 0.4996,
        },
      }),
    );
    await fixture.componentInstance.runAudit();
    fixture.detectChanges();

    const host = fixture.nativeElement as HTMLElement;
    const tracks = host.querySelectorAll('.qt-progress.qt-progress-sm');
    expect(tracks.length).toBe(2);

    const fill = tracks[0].querySelector('.qt-progress-fill') as HTMLElement;
    expect(fill).not.toBeNull();
    // No raw colour utility survives, and the bespoke colour rides the variable.
    expect(fill.style.backgroundColor).toBe('');
    expect(fill.style.getPropertyValue('--qt-progress-indicator')).toContain('--qt-alert-');
    expect(fill.style.width).toBe('50.12%');
  });

  it('sends NO runs field — the count is server-fixed (§W1)', async () => {
    const seen: Req[] = [];
    const fixture = await render(
      validDraft(),
      stubClient({ onDispatch: (r) => seen.push(r), audit: auditResult() }),
    );
    await fixture.componentInstance.runAudit();

    const req = seen.find((r) => r.type === 'customToolAudit');
    expect(req).toBeDefined();
    expect('runs' in req!).toBe(false);
    // Nor does audit carry the private flag — that is preview-only.
    expect('private' in req!).toBe(false);
  });

  it('flags a non-catch-all row that never fired', async () => {
    const fixture = await render(
      validDraft(),
      stubClient({
        audit: {
          runs: 10000,
          outcomes: [
            { index: 0, hits: 0, share: 0 },
            { index: 1, hits: 10000, share: 1 },
          ],
          valueMin: 0,
          valueMax: 1,
          valueMean: 0.5,
        },
      }),
    );
    await fixture.componentInstance.runAudit();
    fixture.detectChanges();
    expect(text(fixture)).toContain('never fired in 10,000 draws');
  });

  it('hints when a metadata-testing tool is given no sheet — and not otherwise', async () => {
    const plain = await render(validDraft(), stubClient());
    expect(plain.componentInstance.noSheetHint()).toBe(false);

    const metaDraft = draftFromDefinition({
      name: 'lockpick',
      description: 'x',
      outcomes: [
        { when: { metadata: { deft: { eq: true } } }, message: 'Deft.', state: 'success' },
        { when: true, message: 'No.', state: 'failure' },
      ],
    })!;
    const fixture = await render(metaDraft, stubClient());
    expect(fixture.componentInstance.noSheetHint()).toBe(true);

    fixture.componentInstance.sheet.set({ mode: 'manual', text: '{"deft": true}' });
    expect(fixture.componentInstance.noSheetHint()).toBe(false);

    // Whitespace inside an otherwise-empty object still counts as no sheet.
    fixture.componentInstance.sheet.set({ mode: 'manual', text: '{  }' });
    expect(fixture.componentInstance.noSheetHint()).toBe(true);
  });

  it('shows the exact bytes Save would write', async () => {
    const fixture = await render(validDraft(), stubClient());
    const preview = fixture.componentInstance.jsonPreview();
    expect(preview.endsWith('}\n')).toBe(true);
    expect(preview).toContain('"$schema": "/schemas/qtap-custom-tool.schema.json"');
    expect(preview).toContain('\n  "name": "unlock"');
  });

  it('validates the mock state as a single JSON object (v4 ProvingBench:mockStateError)', async () => {
    const fixture = await render(validDraft(), stubClient());
    const bench = fixture.componentInstance;

    expect(bench.mockStateError()).toBeNull(); // '{}' default
    bench.mockStateText.set('{oops');
    expect(bench.mockStateError()).toBe('Mock state is not valid JSON.');
    bench.mockStateText.set('[1,2]');
    expect(bench.mockStateError()).toBe('Mock state must be a single JSON object.');
    bench.mockStateText.set('"nope"');
    expect(bench.mockStateError()).toBe('Mock state must be a single JSON object.');
    bench.mockStateText.set('{"game": {"diff": 5}}');
    expect(bench.mockStateError()).toBeNull();
  });

  it('blocks the bench while the mock state will not parse', async () => {
    const fixture = await render(validDraft(), stubClient(), true);
    expect(fixture.componentInstance.benchDisabled()).toBe(false);
    fixture.componentInstance.mockStateText.set('{oops');
    expect(fixture.componentInstance.benchDisabled()).toBe(true);
  });

  it('carries the mock state onto BOTH the preview and audit bodies (§B)', async () => {
    const seen: Req[] = [];
    const fixture = await render(
      validDraft(),
      stubClient({ onDispatch: (r) => seen.push(r), preview: previewResult(), audit: auditResult() }),
    );
    fixture.componentInstance.mockStateText.set('{"game": {"diff": 5}}');
    await fixture.componentInstance.roll();
    await fixture.componentInstance.runAudit();

    expect(seen.find((r) => r.type === 'customToolPreview')?.['state']).toEqual({ game: { diff: 5 } });
    expect(seen.find((r) => r.type === 'customToolAudit')?.['state']).toEqual({ game: { diff: 5 } });
  });

  it('sends undefined state when the mock will not parse to a single object', async () => {
    const seen: Req[] = [];
    const fixture = await render(
      validDraft(),
      stubClient({ onDispatch: (r) => seen.push(r), preview: previewResult() }),
    );
    // A malformed mock disables the bench, but benchState() itself is fail-soft.
    fixture.componentInstance.mockStateText.set('[1,2]');
    await fixture.componentInstance.roll();
    expect(seen.find((r) => r.type === 'customToolPreview')?.['state']).toBeUndefined();
  });

  it('detects $state use across operands, roll fields, defaults, and templates', async () => {
    const plain = await render(validDraft(), stubClient());
    expect(plain.componentInstance.testsState()).toBe(false);

    const stateOperand = draftFromDefinition({
      name: 'gate',
      description: 'x',
      outcomes: [
        { when: { gte: { $state: 'game.diff', fallback: 3 } }, message: 'a', state: 'success' },
        { when: true, message: 'f', state: 'info' },
      ],
    })!;
    expect((await render(stateOperand, stubClient())).componentInstance.testsState()).toBe(true);

    const stateRoll = draftFromDefinition({
      name: 'draw',
      description: 'x',
      roll: { min: { $state: 'game.low', fallback: 0 } },
      outcomes: [{ when: true, message: 'f', state: 'info' }],
    })!;
    expect((await render(stateRoll, stubClient())).componentInstance.testsState()).toBe(true);

    const stateDefault = draftFromDefinition({
      name: 'draw',
      description: 'x',
      parameters: { bonus: { type: 'number', default: { $state: 'player.bonus', fallback: 1 } } },
      outcomes: [{ when: true, message: 'f', state: 'info' }],
    })!;
    expect((await render(stateDefault, stubClient())).componentInstance.testsState()).toBe(true);

    const stateTemplate = draftFromDefinition({
      name: 'draw',
      description: 'x',
      outcomes: [{ when: true, message: 'It is {{ state.weather }}.', state: 'info' }],
    })!;
    expect((await render(stateTemplate, stubClient())).componentInstance.testsState()).toBe(true);

    // v4 `0506517d3` (ProvingBench.tsx:168): the probe is the placeholder
    // CLASSIFIER, not `/\{\{\s*state\./` — so a bare family prefix and an
    // unterminated brace, both of which the old regex matched, no longer count.
    const bareStatePrefix = draftFromDefinition({
      name: 'draw',
      description: 'x',
      outcomes: [{ when: true, message: 'Nothing here: {{state.}}.', state: 'info' }],
    })!;
    expect((await render(bareStatePrefix, stubClient())).componentInstance.testsState()).toBe(false);

    const unterminated = draftFromDefinition({
      name: 'draw',
      description: 'x',
      outcomes: [{ when: true, message: 'Broken {{state.weather and on.', state: 'info' }],
    })!;
    expect((await render(unterminated, stubClient())).componentInstance.testsState()).toBe(false);
  });

  it('hints that $state refs will use fallbacks when the mock is empty — and not otherwise', async () => {
    const stateOperand = draftFromDefinition({
      name: 'gate',
      description: 'x',
      outcomes: [
        { when: { gte: { $state: 'game.diff', fallback: 3 } }, message: 'a', state: 'success' },
        { when: true, message: 'f', state: 'info' },
      ],
    })!;
    const fixture = await render(stateOperand, stubClient());
    expect(fixture.componentInstance.noMockStateHint()).toBe(true);
    fixture.componentInstance.mockStateText.set('{  }');
    expect(fixture.componentInstance.noMockStateHint()).toBe(true);
    fixture.componentInstance.mockStateText.set('{"game": {"diff": 5}}');
    expect(fixture.componentInstance.noMockStateHint()).toBe(false);

    // A tool with no $state never nags about the mock.
    expect((await render(validDraft(), stubClient())).componentInstance.noMockStateHint()).toBe(false);
  });

  it('carries a declared parameter into the bench spec, bounds included', async () => {
    const draft = newDraft();
    draft.parameters = [
      {
        id: 'p1',
        name: 'bonus',
        type: 'integer',
        defaultValue: '3',
        description: 'A bonus.',
        min: '0',
        max: '9',
      },
    ];
    const fixture = await render(draft, stubClient());
    expect(fixture.componentInstance.paramSpecs()['bonus']).toEqual({
      type: 'integer',
      default: 3,
      description: 'A bonus.',
      min: 0,
      max: 9,
    });
  });

  // -- The availability gate's verdict line (v4 `GateVerdictLine`) -----------

  it('says nothing about a gate an ungated draft does not have', async () => {
    const fixture = await render(validDraft(), stubClient());
    expect(fixture.componentInstance.draftGate()).toBeNull();
    expect(text(fixture)).not.toContain('This tool is gated');
  });

  it('stays silent while the gate is still half-typed — the form already complains', async () => {
    const draft = gatedDraft();
    draft.gateConditions = [
      { id: 'g1', subject: 'metadata', key: '', comparator: 'eq', operand: { kind: 'boolean', value: true } },
    ];
    const fixture = await render(draft, stubClient());
    expect(fixture.componentInstance.draftGate()).toBeNull();
    expect(text(fixture)).not.toContain('This tool is gated');
  });

  it('reads a HAND-TYPED sheet live, both ways, without a roll', async () => {
    const fixture = await render(gatedDraft(), stubClient());
    const bench = fixture.componentInstance;

    bench.sheet.set({ mode: 'manual', text: '{"rank":4}' });
    fixture.detectChanges();
    expect(bench.gateVerdict()).toEqual({ available: true });
    expect(text(fixture)).toContain('✓ This sheet would be offered the tool.');

    bench.sheet.set({ mode: 'manual', text: '{"rank":1}' });
    fixture.detectChanges();
    expect(bench.gateVerdict()).toEqual({ available: false, withheldBy: 'availableWhen' });
    expect(text(fixture)).toContain(
      '✕ This sheet would never be offered the tool — it does not pass every “only show if” test.',
    );
    expect(text(fixture)).toContain('The roll below is the bench indulging you.');
  });

  it('names the other clause when withheldWhen is what withheld it', async () => {
    const draft = gatedDraft();
    draft.gateMode = 'withheld';
    const fixture = await render(draft, stubClient());
    fixture.componentInstance.sheet.set({ mode: 'manual', text: '{"rank":9}' });
    fixture.detectChanges();
    expect(text(fixture)).toContain('it passes the “do not show if” test.');
  });

  it('NEVER guesses at a vault: a character sheet waits for the server’s verdict', async () => {
    const fixture = await render(gatedDraft(), stubClient({ preview: previewResult() }));
    const bench = fixture.componentInstance;

    fixture.componentRef.setInput('vantage', { kind: 'character', characterId: 'char1' });
    fixture.detectChanges();
    expect(bench.gateVerdict()).toBeNull();
    expect(text(fixture)).toContain(
      'This tool is gated. Roll once to learn whether this character would be offered it.',
    );
  });

  it('shows the server’s verdict once a character roll comes back', async () => {
    const fixture = await render(
      gatedDraft(),
      stubClient({
        preview: previewResult({ gate: { available: false, withheldBy: 'availableWhen' } }),
      }),
    );
    const bench = fixture.componentInstance;
    fixture.componentRef.setInput('vantage', { kind: 'character', characterId: 'char1' });

    await bench.roll();
    fixture.detectChanges();

    expect(bench.gateVerdict()).toEqual({ available: false, withheldBy: 'availableWhen' });
    expect(text(fixture)).toContain('✕ This sheet would never be offered the tool');
    // The bench still deals — the roll is rendered either way.
    expect(text(fixture)).toContain('Open.');
  });

  it('falls back to the roll’s verdict when the hand-typed sheet is unparseable', async () => {
    const fixture = await render(
      gatedDraft(),
      stubClient({ preview: previewResult({ gate: { available: true } }) }),
    );
    const bench = fixture.componentInstance;
    bench.sheet.set({ mode: 'manual', text: '{oops' });
    fixture.detectChanges();
    // No live read of a sheet that does not parse, and no roll yet.
    expect(bench.gateVerdict()).toBeNull();
    expect(text(fixture)).toContain('This tool is gated. Roll once');
  });
});

/** A valid draft carrying an `availableWhen` gate on `rank >= 3`. */
function gatedDraft(): ToolDraft {
  const draft = validDraft();
  draft.gateMode = 'available';
  draft.gateConditions = [
    { id: 'g1', subject: 'metadata', key: 'rank', comparator: 'gte', operand: { kind: 'number', text: '3' } },
  ];
  return draft;
}

/**
 * The `c4d4b0de` additions: the two-block miniature (heading paragraph over the
 * message's own block, chip label preferred) and the dry-run effect display.
 */
describe('the two-block bubble and the dry run (v4 c4d4b0de)', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('heads the bubble with the run\'s chip label when it rendered one', async () => {
    const fixture = await render(
      validDraft(),
      stubClient({ preview: previewResult({ chipLabel: 'Agent lambda — Jackie' }) }),
    );
    await fixture.componentInstance.roll();
    fixture.detectChanges();

    const paragraphs = Array.from(
      (fixture.nativeElement as HTMLElement).querySelectorAll('.qt-pascal-result p'),
    ).map((p) => p.textContent?.trim());
    // The heading is its own block, and the message is its own — so an outcome
    // opening with a list or a fence renders as one.
    expect(paragraphs[0]).toBe('🎲 Agent lambda — Jackie');
    expect(paragraphs[1]).toBe('Open.');
  });

  it('falls back to the display title when the run carried no label', async () => {
    const fixture = await render(validDraft(), stubClient({ preview: previewResult() }));
    await fixture.componentInstance.roll();
    fixture.detectChanges();

    const first = (fixture.nativeElement as HTMLElement).querySelector('.qt-pascal-result p');
    expect(first?.textContent?.trim()).toBe('🎲 Unlock');
  });

  it('ignores a blank chip label rather than heading the bubble with nothing', async () => {
    const fixture = await render(
      validDraft(),
      stubClient({ preview: previewResult({ chipLabel: '   ' }) }),
    );
    await fixture.componentInstance.roll();
    fixture.detectChanges();

    const first = (fixture.nativeElement as HTMLElement).querySelector('.qt-pascal-result p');
    expect(first?.textContent?.trim()).toBe('🎲 Unlock');
  });

  it('shows what each effect WOULD write, and says it never writes any of it', async () => {
    const fixture = await render(
      validDraft(),
      stubClient({
        preview: previewResult({
          effects: [
            {
              index: 0,
              target: { kind: 'state', path: ['encounter', 'count'], raw: 'state.encounter.count' },
              value: 4,
            },
            {
              index: 1,
              target: { kind: 'metadata', key: 'lockpick', raw: 'metadata.lockpick' },
              value: 'broken pick',
            },
            { index: 2, skipped: 'condition did not hold' },
          ],
        }),
      }),
    );
    await fixture.componentInstance.roll();
    fixture.detectChanges();

    const rendered = text(fixture);
    expect(rendered).toContain('→ state.encounter.count = 4');
    // Quoted the way v4's JSON.stringify renders it, so a string is visibly a
    // string rather than bare prose the reader has to guess at.
    expect(rendered).toContain('→ metadata.lockpick = "broken pick"');
    expect(rendered).toContain('(would write)');
    expect(rendered).toContain('· effect 3 skipped: condition did not hold');
    expect(rendered).toContain('The bench computes effects; it never applies them.');
  });

  it('says nothing at all about effects when the run resolved none', async () => {
    const fixture = await render(validDraft(), stubClient({ preview: previewResult() }));
    await fixture.componentInstance.roll();
    fixture.detectChanges();
    // Note the parentheses: the JSON-preview card's own hint says "what Save
    // would write", so the bare phrase is not a safe negative.
    expect(text(fixture)).not.toContain('(would write)');
    expect(text(fixture)).not.toContain('never applies them');
  });
});

// v4 `f0db8cbb8` (`ProvingBench.tsx:683-716`): the bench's THIRD branch — a
// wardrobe gesture it "would attempt" — and `describeBenchGesture`'s words.
// Read structurally so the file compiles (red) against a bench without it.
const describeBenchGesture = (bench as unknown as {
  describeBenchGesture?: (gesture: Record<string, unknown>) => string;
}).describeBenchGesture;

describe('the bench\u2019s wardrobe gestures (v4 f0db8cbb8)', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('describes the five gesture shapes in the bench\u2019s words, curly quotes and all', () => {
    expect(typeof describeBenchGesture).toBe('function');
    expect(describeBenchGesture?.({ action: 'wear', items: ['Hat'] })).toBe('wear \u201cHat\u201d');
    expect(describeBenchGesture?.({ action: 'wear', items: ['Ribbon'], mode: 'add_to_slot', slot: 'hair' })).toBe(
      'wear \u201cRibbon\u201d (add to slot hair)',
    );
    expect(describeBenchGesture?.({ action: 'wear', items: ['Cloak', 'Hat'], mode: 'replace' })).toBe(
      'wear \u201cCloak\u201d, \u201cHat\u201d (replace)',
    );
    expect(describeBenchGesture?.({ action: 'take_off', items: ['Sundress'], slot: 'top' })).toBe(
      'take off \u201cSundress\u201d from top',
    );
    expect(describeBenchGesture?.({ action: 'clear_slot', items: [], slot: 'hair' })).toBe('clear hair');
    expect(describeBenchGesture?.({ action: 'clear_slot', items: [] })).toBe('clear a slot');
    // The default mode adds nothing, even when a slot rides along.
    expect(describeBenchGesture?.({ action: 'wear', items: ['Hat'], mode: 'wear' })).toBe('wear \u201cHat\u201d');
  });

  it('renders a gesture as "would attempt" between the write and skipped arms', async () => {
    const fixture = await render(
      validDraft(),
      stubClient({
        preview: previewResult({
          effects: [
            { index: 0, target: { kind: 'state', path: ['x'], raw: 'state.x' }, value: 1 },
            { index: 1, wardrobe: { action: 'wear', items: ['Cloak of Night'] } },
            { index: 2, skipped: 'every item reference rendered empty' },
          ],
        }),
      }),
    );
    await fixture.componentInstance.roll();
    fixture.detectChanges();

    const lines = Array.from(
      (fixture.nativeElement as HTMLElement).querySelectorAll('.font-mono p'),
    ).map((p) => p.textContent?.replace(/\s+/g, ' ').trim());
    expect(lines).toContain('\u2192 state.x = 1 (would write)');
    expect(lines).toContain('\u2192 wardrobe: wear \u201cCloak of Night\u201d (would attempt)');
    expect(lines).toContain('\u00b7 effect 3 skipped: every item reference rendered empty');
    // A gesture never reads as a write.
    expect(lines.filter((l) => l?.includes('(would write)'))).toHaveLength(1);
  });
});

/**
 * The wardrobe lens on the bench (v4 `5cb0e0190`, `ProvingBench.tsx`): with a
 * character in the chair each gesture's items say "would apply" or "would
 * skip" over C3 §1's `wardrobeResolution` (stubbed here with C3's bytes — the
 * server lane lands it), and without one the bench still "would attempt".
 */
describe('the bench\u2019s wardrobe lens (v4 5cb0e0190)', () => {
  afterEach(() => TestBed.resetTestingModule());

  const SUNDRESS = {
    ref: 'Sundress',
    found: {
      id: 'dress',
      title: 'Sundress',
      types: ['top', 'bottom'],
      origin: { scope: 'general', id: null, name: 'Quilltap General' },
      imageFileId: null,
    },
  };
  const OWN_SHIRT = {
    ref: 'linen shirt',
    found: {
      id: 'shirt',
      title: 'Linen Shirt',
      types: ['top'],
      origin: { scope: 'character', id: 'char-imogen', name: 'Imogen' },
      imageFileId: null,
    },
  };
  const MISSING = {
    ref: 'Cloak of Nite',
    found: null,
    reason: 'not_found',
    message: 'Wardrobe item not found with title "Cloak of Nite"',
  };
  const ARCHIVED = {
    ref: 'Old Coat',
    found: null,
    reason: 'archived',
    message: 'Item "Old Coat" is archived and cannot be worn',
  };
  const WRONG_SLOT = {
    ref: 'Sundress',
    found: null,
    reason: 'slot',
    message: 'Item "Sundress" (types: top, bottom) cannot be added to the "footwear" slot',
  };

  async function rollWith(
    effects: unknown[],
    wardrobeResolution: unknown[] | undefined,
    seat = true,
  ): Promise<{ lines: (string | undefined)[]; seen: Req[] }> {
    const seen: Req[] = [];
    const fixture = await render(
      validDraft(),
      stubClient({
        onDispatch: (r) => seen.push(r),
        preview: previewResult({ effects, ...(wardrobeResolution ? { wardrobeResolution } : {}) }),
        destinations: { characters: [{ characterId: 'char-imogen', characterName: 'Imogen' }] },
      }),
    );
    if (seat) {
      fixture.componentRef.setInput('vantage', { kind: 'character', characterId: 'char-imogen' });
      fixture.detectChanges();
      await new Promise((r) => setTimeout(r, 0));
    }
    await fixture.componentInstance.roll();
    fixture.detectChanges();
    const lines = Array.from(
      (fixture.nativeElement as HTMLElement).querySelectorAll('.font-mono p'),
    ).map((p) => p.textContent?.replace(/\s+/g, ' ').trim());
    return { lines, seen };
  }

  // The detail is `formatSlotLabels` + `wardrobeOriginLabel` (v4 `:781-823`),
  // so a General item reads "Shared · Quilltap General" — the order's prose
  // ("Top, Bottom · Quilltap General") dropped the label's prefix; the code
  // is what ports.
  it('prints a found item with its slots and origin, "would apply"', async () => {
    const { lines } = await rollWith(
      [{ index: 0, wardrobe: { action: 'wear', items: ['Sundress'] } }],
      [{ index: 0, items: [SUNDRESS] }],
    );
    expect(lines).toContain('\u2192 wardrobe: wear Sundress (Top, Bottom \u00b7 Shared \u00b7 Quilltap General) (would apply)');
  });

  it('prints an own item with its slots alone — an own garment has no origin label', async () => {
    const { lines } = await rollWith(
      [{ index: 0, wardrobe: { action: 'take_off', items: ['linen shirt'], slot: 'top' } }],
      [{ index: 0, items: [OWN_SHIRT] }],
    );
    expect(lines).toContain('\u2192 wardrobe: take off Linen Shirt (Top) from top (would apply)');
  });

  it('names the chair in a not-found skip', async () => {
    const { lines } = await rollWith(
      [{ index: 0, wardrobe: { action: 'wear', items: ['Cloak of Nite'] } }],
      [{ index: 0, items: [MISSING] }],
    );
    expect(lines).toContain(
      '\u2192 wardrobe: wear \u201cCloak of Nite\u201d \u2014 not in Imogen\u2019s reach (would skip)',
    );
  });

  it('says "the chair\u2019s reach" when the roll knew no chair name', () => {
    const lines = bench.benchWardrobeLines(
      { action: 'wear', items: ['Cloak of Nite'] },
      { index: 0, items: [MISSING] as never },
      null,
    );
    expect(lines).toEqual([
      { kind: 'skip', text: 'wear \u201cCloak of Nite\u201d \u2014 not in the chair\u2019s reach' },
    ]);
  });

  it('prints the server\u2019s archived and slot sentences verbatim', async () => {
    const { lines } = await rollWith(
      [{ index: 0, wardrobe: { action: 'wear', items: ['Old Coat', 'Sundress'], mode: 'add_to_slot', slot: 'footwear' } }],
      [{ index: 0, items: [ARCHIVED, WRONG_SLOT] }],
    );
    expect(lines).toContain(
      '\u2192 wardrobe: wear \u201cOld Coat\u201d (add to slot footwear) \u2014 Item "Old Coat" is archived and cannot be worn (would skip)',
    );
    expect(lines).toContain(
      '\u2192 wardrobe: wear \u201cSundress\u201d (add to slot footwear) \u2014 Item "Sundress" (types: top, bottom) cannot be added to the "footwear" slot (would skip)',
    );
  });

  it('keeps a clear_slot beside a wear on its own index', async () => {
    const { lines } = await rollWith(
      [
        { index: 0, wardrobe: { action: 'clear_slot', items: [], slot: 'hair' } },
        { index: 1, wardrobe: { action: 'wear', items: ['Sundress'] } },
      ],
      [{ index: 1, items: [SUNDRESS] }],
    );
    expect(lines).toContain('\u2192 wardrobe: clear hair (would attempt)');
    expect(lines).toContain('\u2192 wardrobe: wear Sundress (Top, Bottom \u00b7 Shared \u00b7 Quilltap General) (would apply)');
  });

  it('still "would attempt" when the roll carries no resolution', async () => {
    const { lines } = await rollWith(
      [{ index: 0, wardrobe: { action: 'wear', items: ['Sundress'] } }],
      undefined,
      false,
    );
    expect(lines).toContain('\u2192 wardrobe: wear \u201cSundress\u201d (would attempt)');
  });

  it('seats the vantage\u2019s character and sends it as the sheet', async () => {
    const { seen } = await rollWith([], undefined);
    const req = seen.find((r) => r.type === 'customToolPreview');
    expect(req?.['metadata']).toEqual({ characterId: 'char-imogen' });
  });

  it('opens on character mode when the editor seats a chair, and writes the vantage from its select', async () => {
    const fixture = await render(
      validDraft(),
      stubClient({
        destinations: { characters: [{ characterId: 'char-imogen', characterName: 'Imogen' }] },
      }),
    );
    const changes: unknown[] = [];
    fixture.componentInstance.vantageChange.subscribe((v) => changes.push(v));
    fixture.componentRef.setInput('vantage', { kind: 'character', characterId: 'char-imogen' });
    fixture.detectChanges();
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
    expect(fixture.componentInstance.sheet()).toEqual({ mode: 'character' });
    const select = (fixture.nativeElement as HTMLElement).querySelector<HTMLSelectElement>(
      'select[aria-label="Character whose fact sheet to use"]',
    )!;
    expect(select.options[0].textContent).toBe('\u2014 nobody in particular \u2014');
    expect(select.value).toBe('char-imogen');
    // The fact-sheet textarea is gone in character mode (v4 :116).
    expect(
      (fixture.nativeElement as HTMLElement).querySelector('textarea[aria-label^="Hand-typed fact sheet"]'),
    ).toBeNull();

    select.value = '';
    select.dispatchEvent(new Event('change'));
    expect(changes).toEqual([{ kind: 'none' }]);
  });

  it('leaves a shared vantage alone when nobody in particular is chosen', async () => {
    const fixture = await render(validDraft(), stubClient());
    const changes: unknown[] = [];
    fixture.componentInstance.vantageChange.subscribe((v) => changes.push(v));
    fixture.componentRef.setInput('vantage', { kind: 'container', container: { scope: 'general', id: null } });
    fixture.componentInstance.sheet.set({ mode: 'character' });
    fixture.detectChanges();
    const select = (fixture.nativeElement as HTMLElement).querySelector<HTMLSelectElement>(
      'select[aria-label="Character whose fact sheet to use"]',
    )!;
    select.value = '';
    select.dispatchEvent(new Event('change'));
    expect(changes).toEqual([]);
  });

  it('restores the hand-typed sheet with {} (v4 :180)', async () => {
    const fixture = await render(validDraft(), stubClient());
    fixture.componentRef.setInput('vantage', { kind: 'character', characterId: 'c' });
    fixture.detectChanges();
    const handTyped = Array.from(
      (fixture.nativeElement as HTMLElement).querySelectorAll<HTMLButtonElement>('button[role="radio"]'),
    ).find((b) => b.textContent?.trim() === 'Hand-typed sheet')!;
    handTyped.click();
    fixture.detectChanges();
    expect(fixture.componentInstance.sheet()).toEqual({ mode: 'manual', text: '{}' });
    expect(
      (fixture.nativeElement as HTMLElement).querySelector<HTMLTextAreaElement>(
        'textarea[aria-label^="Hand-typed fact sheet"]',
      )!.value,
    ).toBe('{}');
  });

  it('holds its own vantage when the editor binds none', async () => {
    const fixture = await render(
      validDraft(),
      stubClient({ destinations: { characters: [{ characterId: 'c1', characterName: 'Aria' }] } }),
    );
    fixture.componentInstance.sheet.set({ mode: 'character' });
    fixture.detectChanges();
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
    const select = (fixture.nativeElement as HTMLElement).querySelector<HTMLSelectElement>(
      'select[aria-label="Character whose fact sheet to use"]',
    )!;
    select.value = 'c1';
    select.dispatchEvent(new Event('change'));
    expect(fixture.componentInstance.chairCharacterId()).toBe('c1');
  });
});

function previewResult(over: Record<string, unknown> = {}) {
  return {
    tool: 'unlock',
    params: {},
    rollForm: 'range',
    raw: 0.75,
    value: 0.75,
    state: 'success',
    outcomeIndex: 0,
    message: 'Open.',
    diceBreakdown: '',
    visibility: 'public',
    ...over,
  };
}

function auditResult() {
  return {
    runs: 10000,
    outcomes: [{ index: 0, hits: 10000, share: 1 }],
    valueMin: 0,
    valueMax: 1,
    valueMean: 0.5,
  };
}
