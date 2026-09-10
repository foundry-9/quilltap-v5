/**
 * The call sheet under an assistant avatar: order, marks, and strikes. A
 * parity transcription of v4's `__tests__/unit/components/ui/RouteTrailBadge.test.tsx`.
 */

import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it } from 'vitest';

import type { RouteAttempt } from '../core/core-contract';
import { RouteTrailBadge } from './route-trail-badge';

const PRIMARY: RouteAttempt = {
  profileId: '00000000-0000-4000-8000-00000000000a',
  profileName: 'OpenAI gpt-5',
  provider: 'openai',
  modelName: 'gpt-5',
  via: 'primary',
  outcome: 'failed',
  trigger: 'network',
  detail: 'Connection error.',
};

const REFUSED: RouteAttempt = {
  profileId: '00000000-0000-4000-8000-00000000000b',
  profileName: 'Anthropic Sonnet',
  provider: 'anthropic',
  modelName: 'claude-sonnet-5',
  via: 'understudy',
  outcome: 'refused',
  trigger: 'moderation-refusal',
  evidence: 'finish-reason',
  detail: 'finish_reason: refusal',
};

const ANSWERED: RouteAttempt = {
  profileId: '00000000-0000-4000-8000-00000000000c',
  profileName: 'DeepSeek',
  provider: 'deepseek',
  modelName: 'deepseek-v4-pro',
  via: 'tier-pick',
  outcome: 'answered',
};

function render(trail: RouteAttempt[]): ComponentFixture<RouteTrailBadge> {
  TestBed.configureTestingModule({ imports: [RouteTrailBadge] });
  const fixture = TestBed.createComponent(RouteTrailBadge);
  fixture.componentRef.setInput('routeTrail', trail);
  fixture.detectChanges();
  return fixture;
}

describe('RouteTrailBadge', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('lists every profile tried, first asked at the top', () => {
    const fixture = render([PRIMARY, REFUSED, ANSWERED]);
    const list = fixture.nativeElement.querySelector('[aria-label="Models tried for this reply"]') as HTMLElement;
    const rows = list.querySelectorAll('li');
    expect(rows).toHaveLength(3);
    expect(rows[0].textContent).toContain('gpt-5');
    expect(rows[1].textContent).toContain('claude-sonnet-5');
    expect(rows[2].textContent).toContain('deepseek-v4-pro');
  });

  it('strikes through only the rows that did not answer', () => {
    const fixture = render([PRIMARY, REFUSED, ANSWERED]);
    const struck = Array.from(fixture.nativeElement.querySelectorAll('s')) as HTMLElement[];
    expect(struck).toHaveLength(2);
    expect(struck[0].textContent).toContain('gpt-5');
    expect(struck[1].textContent).toContain('claude-sonnet-5');
  });

  it('marks a self-inflicted failure ❌ and a content refusal 🚫', () => {
    const fixture = render([PRIMARY, REFUSED, ANSWERED]);
    const el = fixture.nativeElement as HTMLElement;
    expect(el.querySelector('[aria-label="failed"]')?.textContent).toBe('❌');
    expect(el.querySelector('[aria-label="refused on content grounds"]')?.textContent).toBe('🚫');
    expect(el.querySelector('[aria-label="answered"]')).toBeNull();
  });

  it("carries the full explanation as each row's hover text", () => {
    const fixture = render([PRIMARY, ANSWERED]);
    const el = fixture.nativeElement as HTMLElement;
    expect(
      el.querySelector(
        '[title="OpenAI gpt-5 · openai: gpt-5 — first on the call sheet; fell over: network (Connection error.)"]',
      ),
    ).not.toBeNull();
    expect(
      el.querySelector('[title="DeepSeek · deepseek: deepseek-v4-pro — drafted from the company by tier; answered"]'),
    ).not.toBeNull();
  });

  it('renders a one-row trail with no mark and no strike, as the plain badge would', () => {
    const fixture = render([ANSWERED]);
    const el = fixture.nativeElement as HTMLElement;
    expect(el.querySelectorAll('li')).toHaveLength(1);
    expect(el.querySelector('s')).toBeNull();
    expect(el.textContent).not.toContain('❌');
    expect(el.textContent).not.toContain('🚫');
  });

  it('collapses the same-profile retry into one row that says it answered second time', () => {
    const retried: RouteAttempt = { ...PRIMARY, via: 'retry', outcome: 'answered', trigger: undefined, detail: undefined };
    const fixture = render([PRIMARY, retried]);
    const el = fixture.nativeElement as HTMLElement;
    expect(el.querySelectorAll('li')).toHaveLength(1);
    expect(el.querySelector('s')).toBeNull();
    expect(
      el.querySelector('[title="OpenAI gpt-5 · openai: gpt-5 — first on the call sheet; answered on the second try"]'),
    ).not.toBeNull();
  });

  // v4 `:34` — `if (rows.length === 0) return null`: no list, no aria-label,
  // nothing for a screen reader to announce. (The host element itself is
  // Angular's; the caller in `message-row.ts` never mounts this on an empty
  // array anyway.) The §3 unification review of the `78b381a96` round restored
  // v4's case from a weakened "renders an empty list" transcription.
  it('renders nothing for an empty trail', () => {
    const fixture = render([]);
    expect(fixture.nativeElement.querySelector('[aria-label="Models tried for this reply"]')).toBeNull();
    expect(fixture.nativeElement.querySelector('ul')).toBeNull();
  });
});
