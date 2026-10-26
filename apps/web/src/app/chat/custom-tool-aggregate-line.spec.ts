import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it } from 'vitest';

import {
  CustomToolAggregateLine,
  INVENTORY_AGGREGATE_LEAD,
  INVENTORY_AGGREGATE_TAIL,
  inventoryAggregateKeys,
} from './custom-tool-aggregate-line';
import type { CustomToolReferences } from './custom-tools.api';

/**
 * The run dialog's aggregate paragraph, proven alone — v4
 * `CustomToolRunDialog.tsx:948-966` + `:1060-1071` (`a53024d3c`).
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

async function render(references: CustomToolReferences | undefined): Promise<ComponentFixture<CustomToolAggregateLine>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({ imports: [CustomToolAggregateLine] });
  const fixture = TestBed.createComponent(CustomToolAggregateLine);
  fixture.componentRef.setInput('references', references);
  fixture.detectChanges();
  return fixture;
}

const paragraphs = (f: ComponentFixture<unknown>) => [...(f.nativeElement as HTMLElement).querySelectorAll('p')];

describe('inventoryAggregateKeys (v4 CustomToolRunDialog.tsx:949)', () => {
  it('is none for an absent reference set or an older server', () => {
    expect(inventoryAggregateKeys(undefined)).toEqual([]);
    expect(inventoryAggregateKeys(BASE)).toEqual([]);
  });

  it('returns the keys in the server’s order, verbatim', () => {
    expect(
      inventoryAggregateKeys({ ...BASE, inventoryAggregates: ['owned.titles', 'equipped.metadata.toolVersion.max'] } as never),
    ).toEqual(['owned.titles', 'equipped.metadata.toolVersion.max']);
  });
});

describe('CustomToolAggregateLine', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('renders nothing for an empty list', async () => {
    expect(paragraphs(await render(BASE))).toHaveLength(0);
    expect(paragraphs(await render(undefined))).toHaveLength(0);
    expect(paragraphs(await render({ ...BASE, inventoryAggregates: [] } as never))).toHaveLength(0);
  });

  it('renders one paragraph: v4’s lead, the values joined by ", ", v4’s tail', async () => {
    const f = await render({ ...BASE, inventoryAggregates: ['equipped.names', 'owned.count'] } as never);
    const [p] = paragraphs(f);
    expect(paragraphs(f)).toHaveLength(1);
    expect(p.textContent?.replace(/\s+/g, ' ').trim()).toBe(
      'It takes stock of everything they carry, reading equipped.names, owned.count. Run as nobody, it finds no inventory at all.',
    );
    expect(p.getAttribute('class')).toBe('text-xs qt-text-secondary mt-4 leading-relaxed');
    expect([...p.querySelectorAll('code')].map((c) => c.getAttribute('class'))).toEqual(['font-mono qt-text', 'font-mono qt-text']);
    expect(INVENTORY_AGGREGATE_LEAD).toBe('It takes stock of everything they carry, reading');
    expect(INVENTORY_AGGREGATE_TAIL).toBe('. Run as nobody, it finds no inventory at all.');
  });
});
