import { ComponentFixture, TestBed } from '@angular/core/testing';
import { describe, expect, it } from 'vitest';

import {
  CustomToolInventoryLines,
  INVENTORY_LINE_TAIL,
  INVENTORY_READS_TAIL,
  inventoryReferenceLines,
} from './custom-tool-inventory-lines';
import type { CustomToolReferences } from './custom-tools.api';

/**
 * The run dialog's inventory sentences, proven alone — v4
 * `CustomToolRunDialog.tsx:948-955` + `:1046-1083` (`e1872c94f`, pinned at
 * `322a5488e`). The popup mount is P4.D334's (a HANDOFF); this is the pure
 * function and the component it will mount.
 */

const BASE: CustomToolReferences = {
  value: false,
  roll: false,
  dice: false,
  llm: false,
  params: [],
  metadata: [],
  state: [],
};

async function render(
  references: CustomToolReferences | undefined,
): Promise<ComponentFixture<CustomToolInventoryLines>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({ imports: [CustomToolInventoryLines] });
  const fixture = TestBed.createComponent(CustomToolInventoryLines);
  fixture.componentRef.setInput('references', references);
  fixture.detectChanges();
  return fixture;
}

const paragraphs = (f: ComponentFixture<unknown>) => [...(f.nativeElement as HTMLElement).querySelectorAll('p')];
const said = (p: HTMLElement) => p.textContent?.replace(/\s+/g, ' ').trim();

describe('inventoryReferenceLines (v4 CustomToolRunDialog.tsx:948-955)', () => {
  it('reads first, then equips → unequips → picksUp, each in v4’s words', () => {
    expect(
      inventoryReferenceLines({
        ...BASE,
        picksUp: ['spyglass'],
        unequips: ['lantern'],
        equips: ['Excalibur'],
        inventoryReads: ['sword', 'ring'],
      }),
    ).toEqual([
      { key: 'inventoryReads', lead: 'It reads the inventory for', values: ['sword', 'ring'], tail: INVENTORY_READS_TAIL },
      { key: 'equips', lead: 'It may equip the running character with', values: ['Excalibur'], tail: INVENTORY_LINE_TAIL },
      { key: 'unequips', lead: 'It may unequip them of', values: ['lantern'], tail: INVENTORY_LINE_TAIL },
      { key: 'picksUp', lead: 'It may pick up for them', values: ['spyglass'], tail: INVENTORY_LINE_TAIL },
    ]);
  });

  it('pins the two tails byte for byte', () => {
    expect(INVENTORY_READS_TAIL).toBe('. Run as nobody, it finds no inventory at all.');
    expect(INVENTORY_LINE_TAIL).toBe('. Only when you run it as a character — a run made as nobody carries nothing.');
  });

  it('drops a line with no values, and answers nothing for an older server', () => {
    expect(inventoryReferenceLines({ ...BASE, equips: [], unequips: ['Hat'] }).map((l) => l.key)).toEqual(['unequips']);
    expect(inventoryReferenceLines(BASE)).toEqual([]);
    expect(inventoryReferenceLines(undefined)).toEqual([]);
  });
});

describe('qt-custom-tool-inventory-lines', () => {
  it('renders one paragraph per line, the values as <code> joined by ", "', async () => {
    const fixture = await render({ ...BASE, inventoryReads: ['sword', 'ring'], picksUp: ['spyglass'] });
    const ps = paragraphs(fixture);
    expect(ps).toHaveLength(2);
    expect(said(ps[0])).toBe('It reads the inventory for sword, ring. Run as nobody, it finds no inventory at all.');
    expect([...ps[0].querySelectorAll('code')].map((c) => c.textContent)).toEqual(['sword', 'ring']);
    expect(ps[0].className).toBe('text-xs qt-text-secondary mt-4 leading-relaxed');
    expect(said(ps[1])).toBe(
      'It may pick up for them spyglass. Only when you run it as a character — a run made as nobody carries nothing.',
    );
  });

  it('renders nothing for empty lists or no references', async () => {
    expect(paragraphs(await render(BASE))).toHaveLength(0);
    expect(paragraphs(await render(undefined))).toHaveLength(0);
  });
});
