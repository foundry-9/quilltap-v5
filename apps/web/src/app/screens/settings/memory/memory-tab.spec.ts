import { CUSTOM_ELEMENTS_SCHEMA } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { ActivatedRoute, convertToParamMap } from '@angular/router';
import { of } from 'rxjs';
import { describe, expect, it } from 'vitest';

import { CollapsibleCard } from '../../../ui/collapsible-card';
import { MemoryTab } from './memory-tab';

/**
 * P4.D300 — the Memory tab's seven cards in v4's order, titles, descriptions and
 * `sectionId`s from v4 `components/settings/tabs/MemorySearchTabContent.tsx:33-61`
 * at `83334b9a5` (Phase G replaced Housekeeping, Consolidation and Memory
 * Deduplication with ONE Maintenance card), and v4's `RETIRED_SECTIONS`
 * (`:18-22`) mapping the three retired anchors onto `memory-maintenance`. The
 * card bodies are stubbed out (each has its own spec).
 */

function mount(section: string | null) {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [MemoryTab],
    providers: [
      {
        provide: ActivatedRoute,
        useValue: { queryParamMap: of(convertToParamMap(section ? { section } : {})) },
      },
    ],
  });
  TestBed.overrideComponent(MemoryTab, {
    set: { imports: [CollapsibleCard], schemas: [CUSTOM_ELEMENTS_SCHEMA] },
  });
  const fixture = TestBed.createComponent(MemoryTab);
  fixture.detectChanges();
  return fixture;
}

/** The ids of the cards that render open. */
function openCards(section: string | null): string[] {
  const fixture = mount(section);
  return Array.from((fixture.nativeElement as HTMLElement).querySelectorAll('div.qt-collapsible-card'))
    .filter((c) => c.querySelector('.qt-collapsible-card-header')?.getAttribute('aria-expanded') === 'true')
    .map((c) => c.getAttribute('id') ?? '');
}

describe('MemoryTab — the seven cards (P4.D300, v4 MemorySearchTabContent.tsx:33-61)', () => {
  it('renders v4\'s cards in v4\'s order on v4\'s anchors', () => {
    const fixture = mount(null);
    const cards = Array.from(
      (fixture.nativeElement as HTMLElement).querySelectorAll('div.qt-collapsible-card'),
    ).map((c) => [
      c.getAttribute('id'),
      c.querySelector('h3.qt-card-title')?.textContent,
      c.querySelector('p.qt-card-description')?.textContent,
    ]);
    expect(cards).toEqual([
      ['embedding-profiles', 'Embedding Profiles', 'Configure embedding models for semantic memory'],
      ['memory-backfill', 'Repair Missing Embeddings', 'Generate embeddings for legacy memories that predate the embedding-aware gate'],
      ['memory-maintenance', 'Maintenance', 'Fold repetitions, consolidate into digests, keep each character under the cap, and let old archive notes go'],
      ['memory-extraction-grain', 'Extraction Grain', "Choose how often a character's observations of other characters are noted"],
      ['memory-recall', 'Recall Relevance', 'Keep project-specific memories from wandering into unrelated chats'],
      ['memory-regenerate', 'Regenerate Memories', 'Wipe and rebuild every chat-linked memory using the current extraction pipeline'],
      ['conversation-summaries-regenerate', 'Regenerate Conversation Summaries', "Re-mirror every summarised chat into its characters' vaults for relevant-conversation recall"],
    ]);
  });

  it('section=memory-maintenance force-opens the Maintenance card', () => {
    expect(openCards('memory-maintenance')).toContain('memory-maintenance');
  });

  it.each(['memory-housekeeping', 'memory-consolidation', 'memory-deduplication'])(
    'the retired anchor section=%s force-opens the Maintenance card (v4 RETIRED_SECTIONS)',
    (retired) => {
      const open = openCards(retired);
      expect(open).toContain('memory-maintenance');
      expect(open).not.toContain(retired);
    },
  );

  it('an anchor that is not retired passes through unchanged', () => {
    expect(openCards('memory-recall')).toContain('memory-recall');
    expect(openCards('memory-recall')).not.toContain('memory-maintenance');
  });
});
