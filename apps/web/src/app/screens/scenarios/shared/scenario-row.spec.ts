import { ComponentFixture, TestBed } from '@angular/core/testing';
import { describe, expect, it } from 'vitest';

import type { ScenarioDto } from '../../../core/core-contract';
import { ScenarioRow } from './scenario-row';

/**
 * P4.D121 unit 4 — the archive affordance on a scenario row (v4
 * `components/scenarios/__tests__/ScenarioRow.test.tsx` at `d25dacc1`,
 * transcribed onto the Angular row).
 *
 * The row goes three → four actions in BOTH layouts, the default radio is
 * disabled for an archived scenario (it can never win default resolution
 * server-side, so offering the control would be a lie), and the badge appears
 * only when the file carries the flag.
 */

function scenario(over: Partial<ScenarioDto> = {}): ScenarioDto {
  return {
    path: 'Scenarios/tavern.md',
    filename: 'tavern',
    name: 'Tavern',
    description: 'A cozy inn at the crossroads.',
    isDefault: false,
    rawIsDefault: false,
    archived: false,
    body: 'Once upon a time…',
    lastModified: '2026-01-01T00:00:00.000Z',
    createdAt: '2026-01-01T00:00:00.000Z',
    updatedAt: '2026-01-01T00:00:00.000Z',
    ...over,
  };
}

interface Rendered {
  fixture: ComponentFixture<ScenarioRow>;
  host: HTMLElement;
  toggled: ScenarioDto[];
}

function render(dto: ScenarioDto): Rendered {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({ imports: [ScenarioRow] });
  const fixture = TestBed.createComponent(ScenarioRow);
  fixture.componentRef.setInput('scenario', dto);
  fixture.componentRef.setInput('scopeLabel', 'project');
  const toggled: ScenarioDto[] = [];
  fixture.componentInstance.toggleArchived.subscribe((s: ScenarioDto) => toggled.push(s));
  fixture.detectChanges();
  return { fixture, host: fixture.nativeElement as HTMLElement, toggled };
}

/** The inline (wide-container) action buttons, in DOM order. */
function inlineLabels(host: HTMLElement): string[] {
  const wide = host.querySelector('.\\@lg\\:flex') as HTMLElement;
  return Array.from(wide.querySelectorAll('button')).map((b) => (b.textContent ?? '').trim());
}

function kebabLabels(rendered: Rendered): string[] {
  const kebab = Array.from(rendered.host.querySelectorAll('button')).find((b) =>
    (b.getAttribute('aria-label') ?? '').startsWith('More actions for'),
  ) as HTMLButtonElement;
  kebab.click();
  rendered.fixture.detectChanges();
  return Array.from(rendered.host.querySelectorAll('[role="menu"] button')).map((b) =>
    (b.textContent ?? '').trim(),
  );
}

function button(host: HTMLElement, label: string): HTMLButtonElement | undefined {
  return Array.from(host.querySelectorAll('button')).find(
    (b) => (b.textContent ?? '').trim() === label,
  ) as HTMLButtonElement | undefined;
}

describe('ScenarioRow — the archive affordance (v4 d25dacc1)', () => {
  it('badges an archived scenario and leaves an active one unbadged', () => {
    expect(render(scenario({ archived: true })).host.textContent).toContain('Archived');
    expect(render(scenario()).host.textContent).not.toContain('Archived');
  });

  it('disables the default radio for an archived scenario, with v4’s title', () => {
    const active = render(scenario());
    const activeRadio = active.host.querySelector('input[type="radio"]') as HTMLInputElement;
    expect(activeRadio.disabled).toBe(false);
    expect(activeRadio.title).toBe('Set as project default');

    const archived = render(scenario({ archived: true }));
    const radio = archived.host.querySelector('input[type="radio"]') as HTMLInputElement;
    expect(radio.disabled).toBe(true);
    expect(radio.title).toBe('Archived scenarios cannot be the default');
  });

  it('offers four inline actions, Archive between Rename and Delete', () => {
    expect(inlineLabels(render(scenario()).host)).toEqual([
      'Edit',
      'Rename',
      'Archive',
      'Delete',
    ]);
  });

  it('flips the inline label to Restore for an archived scenario, with v4’s titles', () => {
    const active = render(scenario());
    expect(button(active.host, 'Archive')!.title).toBe('Hide this scenario from the pickers');
    button(active.host, 'Archive')!.click();
    expect(active.toggled).toHaveLength(1);

    const archived = render(scenario({ archived: true }));
    expect(inlineLabels(archived.host)).toEqual(['Edit', 'Rename', 'Restore', 'Delete']);
    expect(button(archived.host, 'Restore')!.title).toBe(
      'Restore this scenario to the pickers',
    );
    button(archived.host, 'Restore')!.click();
    expect(archived.toggled).toHaveLength(1);
  });

  it('holds the same four actions in the narrow kebab, and closes on pick', () => {
    const rendered = render(scenario());
    expect(kebabLabels(rendered)).toEqual(['Edit', 'Rename', 'Archive', 'Delete']);
    const entry = Array.from(rendered.host.querySelectorAll('[role="menu"] button')).find(
      (b) => (b.textContent ?? '').trim() === 'Archive',
    ) as HTMLButtonElement;
    entry.click();
    rendered.fixture.detectChanges();
    expect(rendered.toggled).toHaveLength(1);
    expect(rendered.host.querySelector('[role="menu"]')).toBeNull();
  });

  it('flips the kebab label too', () => {
    expect(kebabLabels(render(scenario({ archived: true })))).toEqual([
      'Edit',
      'Rename',
      'Restore',
      'Delete',
    ]);
  });
});
