import { ComponentFixture, TestBed } from '@angular/core/testing';
import { describe, expect, it } from 'vitest';

import type {
  HuggingFaceLoraFacts,
  HuggingFaceLookupResult,
} from '../../../core/core-contract';
import {
  LoraQueryResult,
  baseModelsCopy,
  failureCopy,
  gatedCopy,
  kindCopy,
  standingCopy,
} from './lora-query-result';
import shapes from './__fixtures__/lora-lookup-shapes.json';

/**
 * Parity spec for the queried-LoRA read-out. The oracle is v4's CLIENT
 * component `components/image-profiles/LoraQueryResult.tsx` at `2ece98c90`.
 *
 * The result objects it renders are NOT hand-written: `lora-lookup-shapes.json`
 * is v4's REAL `lookupHuggingFaceLora` run over v4's own mock payloads from a
 * worktree pinned at `2ece98c90` (recipe:
 * `harness/oracle/cases/lora-lookup-shapes.test.ts`). So the panel is exercised
 * on exactly the shapes the server produces, `base_model:adapter:` tag merge
 * and all, rather than on what a fixture author guessed they look like.
 */

type Shape = (typeof shapes)[number];

function shape(name: string): HuggingFaceLookupResult {
  const row = shapes.find((s: Shape) => s.name === name);
  if (!row) throw new Error(`no recorded shape named ${name}`);
  return row.result as unknown as HuggingFaceLookupResult;
}

function factsOf(name: string): HuggingFaceLoraFacts {
  const result = shape(name);
  if (!result.ok) throw new Error(`${name} is a failure shape`);
  return result.facts;
}

async function render(inputs: {
  result: HuggingFaceLookupResult;
  supportsPrivateWeightsToken?: boolean;
  currentTriggerPhrase?: string;
}): Promise<{ fixture: ComponentFixture<LoraQueryResult>; offered: string[] }> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({ imports: [LoraQueryResult] });
  const fixture = TestBed.createComponent(LoraQueryResult);
  const offered: string[] = [];
  fixture.componentRef.setInput('result', inputs.result);
  fixture.componentRef.setInput(
    'supportsPrivateWeightsToken',
    inputs.supportsPrivateWeightsToken ?? false,
  );
  fixture.componentRef.setInput('currentTriggerPhrase', inputs.currentTriggerPhrase ?? '');
  fixture.componentInstance.useTriggerPhrase.subscribe((p) => offered.push(p));
  fixture.detectChanges();
  await fixture.whenStable();
  fixture.detectChanges();
  return { fixture, offered };
}

/** The panel's visible text, whitespace-collapsed as a reader would see it. */
function text(fixture: ComponentFixture<unknown>): string {
  return ((fixture.nativeElement as HTMLElement).textContent ?? '').replace(/\s+/g, ' ').trim();
}

function el<T extends HTMLElement>(fixture: ComponentFixture<unknown>, selector: string): T | null {
  return (fixture.nativeElement as HTMLElement).querySelector<T>(selector);
}

/**
 * The seven failure sentences, byte for byte. These are the load-bearing copy
 * of the whole feature — the panel exists to say what went wrong in terms the
 * reader can act on — and only two of the seven are reachable through a
 * recorded shape, so they are pinned at the function.
 */
describe('LoraQueryResult failure copy (v4 `failureCopy`, byte-exact)', () => {
  const fail = (reason: string): Extract<HuggingFaceLookupResult, { ok: false }> =>
    ({ ok: false, reason, repoId: null, url: null }) as Extract<
      HuggingFaceLookupResult,
      { ok: false }
    >;

  it('not-a-repo-id', () => {
    expect(failureCopy(fail('not-a-repo-id'))).toBe(
      'That source carries no HuggingFace address, so there is no registry to consult. Weights hosted elsewhere must be taken on trust.',
    );
  });

  it('missing-or-private — and it says why the two cases are fused', () => {
    expect(failureCopy(fail('missing-or-private'))).toBe(
      'HuggingFace declines to confirm this one. Either no such repository exists, or it is private and you are not on the list — the registry answers both cases identically, and does so on purpose. Check the spelling first; if it is a private or gated repository, a HuggingFace token will settle the question.',
    );
  });

  it('not-found', () => {
    expect(failureCopy(fail('not-found'))).toBe(
      'No such repository. Your token was accepted, so this is a genuine absence rather than a door held shut.',
    );
  });

  it('rate-limited — the apostrophe is U+2019, not U+0027', () => {
    const copy = failureCopy(fail('rate-limited'));
    expect(copy).toBe(
      'HuggingFace begs a moment’s patience — too many enquiries too quickly. Try again shortly.',
    );
    expect(copy).toContain('moment’s');
    expect(copy).not.toContain("moment's");
  });

  it('timeout — and it names the ten seconds the lookup actually waits', () => {
    expect(failureCopy(fail('timeout'))).toBe(
      'HuggingFace did not answer within ten seconds. The registry may be having a trying afternoon.',
    );
  });

  it('network', () => {
    expect(failureCopy(fail('network'))).toBe(
      'HuggingFace could not be reached at all. Check that this machine can see the outside world.',
    );
  });

  it('http — the default arm, for a status with no sentence of its own', () => {
    expect(failureCopy(fail('http'))).toBe(
      'HuggingFace answered, but not in any language this establishment recognises.',
    );
    // v4's `default:` catches anything unlisted, so a reason this client does
    // not know about still reads as something rather than as blank.
    expect(failureCopy(fail('a-reason-from-the-future'))).toBe(
      'HuggingFace answered, but not in any language this establishment recognises.',
    );
  });
});

/** The three `kindCopy` sentences, byte for byte, over RECORDED shapes. */
describe('LoraQueryResult kind copy (v4 `kindCopy`, byte-exact)', () => {
  it('tagged a LoRA', () => {
    expect(kindCopy(factsOf('realism-lora'))).toBe('Tagged a LoRA adapter.');
  });

  it('tagged an adapter but not a LoRA', () => {
    expect(kindCopy(factsOf('adapter-not-lora'))).toBe(
      'Tagged an adapter, though not specifically a LoRA.',
    );
  });

  it('tagged neither', () => {
    expect(kindCopy(factsOf('not-an-adapter'))).toBe(
      'Not tagged as an adapter at all — this may be a full checkpoint rather than something to layer on top of one.',
    );
  });

  it('isLora wins over isAdapter — the recorded shapes make that reachable', () => {
    // `ambiguous-weights` records isLora true WITH isAdapter false, which is a
    // combination no hand-written fixture would think to build. It proves the
    // order of v4's two ifs rather than assuming it.
    const facts = factsOf('ambiguous-weights');
    expect(facts.isLora).toBe(true);
    expect(facts.isAdapter).toBe(false);
    expect(kindCopy(facts)).toBe('Tagged a LoRA adapter.');
  });
});

describe('LoraQueryResult rows', () => {
  it('names the base models the card declares', () => {
    expect(baseModelsCopy(factsOf('multi-base'))).toBe(
      'black-forest-labs/FLUX.1-dev, black-forest-labs/FLUX.2-dev',
    );
  });

  it('stands in for a card that names none', () => {
    expect(baseModelsCopy(factsOf('no-base-model'))).toBe(
      'The card names no base model. Whether it suits your chosen model is a matter for the model card.',
    );
  });

  it('the gated sentence cross-references the token field when the model takes one', () => {
    expect(gatedCopy(factsOf('gated'), true)).toBe(
      'This repository is gated (auto); the weights want a HuggingFace token. The selected model accepts one — see the HuggingFace Token field in the options above.',
    );
  });

  it('...and says so plainly when it does not', () => {
    expect(gatedCopy(factsOf('gated'), false)).toBe(
      'This repository is gated (auto); the weights want a HuggingFace token. The selected model has nowhere to put one, so these weights are unlikely to load.',
    );
  });

  it('the standing line joins with a middle dot and localises the numbers', () => {
    expect(standingCopy(factsOf('realism-lora'))).toBe('1,232 likes · 15,707 downloads');
  });

  it('...drops the half it does not have', () => {
    // `not-an-adapter` records downloads 42 with likes null.
    expect(standingCopy(factsOf('not-an-adapter'))).toBe('42 downloads');
  });

  it('...and is empty when it has neither', () => {
    expect(standingCopy(factsOf('trigger-phrase'))).toBe('');
  });
});

describe('LoraQueryResult (rendered)', () => {
  it('renders the facts panel with its heading, rows and closing sentence', async () => {
    const { fixture } = await render({ result: shape('realism-lora') });
    const body = text(fixture);
    expect(body).toContain('HuggingFace says');
    expect(body).toContain('Trained on');
    expect(body).toContain('black-forest-labs/FLUX.1-dev');
    expect(body).toContain('Nature');
    expect(body).toContain('Tagged a LoRA adapter.');
    expect(body).toContain('Pipeline');
    expect(body).toContain('text-to-image');
    expect(body).toContain('Weights');
    expect(body).toContain('lora.safetensors');
    expect(body).toContain('Standing');
    expect(body).toContain('1,232 likes · 15,707 downloads');
    // The closing sentence is the panel's whole thesis: facts, no verdict.
    expect(body).toContain(
      'This is what the registry declares, and nothing more. Whether these weights agree with your chosen model is between you and your provider — read the card if in doubt.',
    );
  });

  it('renders NO compatibility verdict anywhere in its text', async () => {
    // The client-side mirror of v4's server-side guard: if the panel ever
    // starts believing a verdict, a wrong "this will not work" is worse than
    // the silence it replaced.
    for (const name of ['realism-lora', 'gated', 'ambiguous-weights', 'not-an-adapter']) {
      const { fixture } = await render({ result: shape(name) });
      const body = text(fixture).toLowerCase();
      for (const forbidden of ['compatible', 'compatibility', 'verdict', 'will not work']) {
        expect(body).not.toContain(forbidden);
      }
    }
  });

  it('links the repo id out to the card, in a new tab, safely', async () => {
    const { fixture } = await render({ result: shape('realism-lora') });
    const link = el<HTMLAnchorElement>(fixture, 'a.qt-link');
    expect(link?.getAttribute('href')).toBe('https://huggingface.co/XLabs-AI/flux-RealismLora');
    expect(link?.getAttribute('target')).toBe('_blank');
    expect(link?.getAttribute('rel')).toBe('noopener noreferrer');
    expect(link?.textContent?.trim()).toBe('XLabs-AI/flux-RealismLora ↗');
  });

  it('omits the Pipeline row when the card declares none', async () => {
    const { fixture } = await render({ result: shape('trigger-phrase') });
    expect(text(fixture)).not.toContain('Pipeline');
  });

  it('omits the Gated row entirely when gated is false', async () => {
    const { fixture } = await render({ result: shape('realism-lora') });
    expect(text(fixture)).not.toContain('Gated');
  });

  it('omits the Standing row when the repo has neither figure', async () => {
    const { fixture } = await render({ result: shape('trigger-phrase') });
    expect(text(fixture)).not.toContain('Standing');
  });

  it('warns when no .safetensors is in the repository', async () => {
    const { fixture } = await render({ result: shape('no-base-model') });
    expect(text(fixture)).toContain(
      'No .safetensors file in the repository — the weights may live elsewhere, or under another name.',
    );
  });

  it('names every weights file, and warns that more than one is a choice', async () => {
    const { fixture } = await render({ result: shape('ambiguous-weights') });
    const body = text(fixture);
    expect(body).toContain(
      'flux-multi-angles-v2-72poses-comfy.safetensors, flux-multi-angles-v2-72poses-fal.safetensors',
    );
    expect(body).toContain(
      '— more than one, so a bare owner/name leaves the choice to your provider. Name the file directly if you have a preference.',
    );
  });

  it('names a single weights file with no warning at all', async () => {
    const { fixture } = await render({ result: shape('realism-lora') });
    expect(text(fixture)).not.toContain('more than one');
  });
});

describe('LoraQueryResult trigger phrase', () => {
  it('offers the declared phrase, and hands it back on click', async () => {
    const { fixture, offered } = await render({ result: shape('trigger-phrase') });
    const body = text(fixture);
    expect(body).toContain('Declared trigger phrase: shou_xin, pencil sketch');
    const button = el<HTMLButtonElement>(fixture, 'button');
    expect(button?.textContent?.trim()).toBe('Use it');
    button!.click();
    expect(offered).toEqual(['shou_xin, pencil sketch']);
  });

  it('says so instead when the row already holds it (v4 `:88-89`)', async () => {
    const { fixture } = await render({
      result: shape('trigger-phrase'),
      currentTriggerPhrase: 'shou_xin, pencil sketch',
    });
    expect(text(fixture)).toContain('— already in place.');
    expect(el(fixture, 'button')).toBeNull();
  });

  it('compares against the TRIMMED current value', async () => {
    // A row whose phrase differs only by surrounding whitespace is the same
    // phrase; re-offering it would be a button that does nothing visible.
    const { fixture } = await render({
      result: shape('trigger-phrase'),
      currentTriggerPhrase: '   shou_xin, pencil sketch  ',
    });
    expect(text(fixture)).toContain('— already in place.');
    expect(el(fixture, 'button')).toBeNull();
  });

  it('offers it again once the row holds something else', async () => {
    const { fixture } = await render({
      result: shape('trigger-phrase'),
      currentTriggerPhrase: 'something else entirely',
    });
    expect(el<HTMLButtonElement>(fixture, 'button')?.textContent?.trim()).toBe('Use it');
  });

  it('draws no trigger row at all when the card declares no phrase', async () => {
    const { fixture } = await render({ result: shape('realism-lora') });
    expect(text(fixture)).not.toContain('Declared trigger phrase');
  });
});

describe('LoraQueryResult failure panel (rendered)', () => {
  it('heads the panel with the repo it tried, and keeps the card link open', async () => {
    const { fixture } = await render({ result: shape('unauthorized') });
    const body = text(fixture);
    expect(body).toContain('HuggingFace — nobody/nothing-at-all');
    expect(body).toContain(
      'HuggingFace declines to confirm this one. Either no such repository exists, or it is private and you are not on the list',
    );
    const link = el<HTMLAnchorElement>(fixture, 'a.qt-link');
    expect(link?.textContent?.trim()).toBe('Try the page yourself ↗');
    expect(link?.getAttribute('href')).toBe('https://huggingface.co/nobody/nothing-at-all');
    expect(link?.getAttribute('rel')).toBe('noopener noreferrer');
  });

  it('falls back to the bare name, and offers no link, when there was no repo id', async () => {
    const { fixture } = await render({ result: shape('not-a-repo-id') });
    const body = text(fixture);
    expect(body).toContain('HuggingFace');
    expect(body).not.toContain('HuggingFace —');
    expect(body).toContain('That source carries no HuggingFace address');
    expect(el(fixture, 'a')).toBeNull();
  });

  it('renders none of the facts furniture on a failure', async () => {
    const { fixture } = await render({ result: shape('unauthorized') });
    const body = text(fixture);
    expect(body).not.toContain('HuggingFace says');
    expect(body).not.toContain('Trained on');
    expect(body).not.toContain('Declared trigger phrase');
  });
});

describe('the recorded corpus itself', () => {
  it('covers every arm the panel can draw', () => {
    // A guard on the fixture, so a regeneration that dropped a shape cannot
    // pass as coverage — each of these is the only row driving its branch.
    const names = shapes.map((s: Shape) => s.name);
    for (const required of [
      'realism-lora',
      'trigger-phrase',
      'multi-base',
      'gated',
      'ambiguous-weights',
      'unauthorized',
      'adapter-not-lora',
      'not-an-adapter',
      'no-base-model',
      'not-a-repo-id',
    ]) {
      expect(names).toContain(required);
    }
    // Both outcomes, and the three weight-count arms.
    const ok = shapes.filter((s: Shape) => s.result.ok);
    expect(ok.length).toBeGreaterThanOrEqual(7);
    expect(shapes.length - ok.length).toBeGreaterThanOrEqual(2);
  });
});
