import { CUSTOM_ELEMENTS_SCHEMA } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { describe, expect, it } from 'vitest';

import { CollapsibleCard } from '../../../ui/collapsible-card';
import { MemoryTab } from './memory-tab';

/**
 * P4.D276 — the Memory tab's nine cards in v4's order, titles, descriptions and
 * `sectionId`s from v4 `components/settings/tabs/MemorySearchTabContent.tsx:25-59`
 * at `01a83539d`. The card bodies are stubbed out (each has its own spec).
 */
describe('MemoryTab — the nine cards (P4.D276, v4 MemorySearchTabContent.tsx:25-59)', () => {
  it('renders v4\'s cards in v4\'s order on v4\'s anchors', () => {
    TestBed.resetTestingModule();
    TestBed.configureTestingModule({ imports: [MemoryTab] });
    TestBed.overrideComponent(MemoryTab, {
      set: { imports: [CollapsibleCard], schemas: [CUSTOM_ELEMENTS_SCHEMA] },
    });
    const fixture = TestBed.createComponent(MemoryTab);
    fixture.detectChanges();
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
      ['memory-housekeeping', 'Memory Housekeeping', 'Retire stale, low-importance memories to the archive as characters approach their cap'],
      ['memory-consolidation', 'Consolidation', 'Fold clusters of near-duplicate memories into digests, and shelve the originals in the archive'],
      ['memory-extraction-grain', 'Extraction Grain', "Choose how often a character's observations of other characters are noted"],
      ['memory-recall', 'Recall Relevance', 'Keep project-specific memories from wandering into unrelated chats'],
      ['memory-deduplication', 'Memory Deduplication', 'Find and remove duplicate memories'],
      ['memory-regenerate', 'Regenerate Memories', 'Wipe and rebuild every chat-linked memory using the current extraction pipeline'],
      ['conversation-summaries-regenerate', 'Regenerate Conversation Summaries', "Re-mirror every summarised chat into its characters' vaults for relevant-conversation recall"],
    ]);
  });
});
