import { ChangeDetectionStrategy, Component, OnInit, computed, inject, signal } from '@angular/core';
import { toSignal } from '@angular/core/rxjs-interop';
import { ActivatedRoute } from '@angular/router';

import { CollapsibleCard } from '../../../ui/collapsible-card';
import { ConversationSummaryRegenerateCard } from './conversation-summary-regenerate-card';
import { EmbeddingProfilesCard } from './embedding-profiles/embedding-profiles-card';
import { MemoryBackfillCard } from './memory-backfill-card';
import { MemoryDedupCard } from './memory-dedup-card';
import { MemoryHousekeepingCard } from './memory-housekeeping-card';
import { MemoryRecallCard } from './memory-recall-card';
import { MemoryRegenerateCard } from './memory-regenerate-card';

/**
 * The Settings → Memory tab (v4 `components/settings/tabs/
 * MemorySearchTabContent.tsx`, subsystem `commonplace-book`): every
 * CollapsibleCard v4 ships — Embedding Profiles, Repair Missing Embeddings,
 * Housekeeping, Recall Relevance, Memory Deduplication, Regenerate Memories, and
 * Regenerate Conversation Summaries — in v4's card order, on v4's `sectionId`
 * `?section=` deep links, ported verbatim.
 */
@Component({
  selector: 'qt-settings-memory',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [
    CollapsibleCard,
    ConversationSummaryRegenerateCard,
    EmbeddingProfilesCard,
    MemoryBackfillCard,
    MemoryDedupCard,
    MemoryHousekeepingCard,
    MemoryRecallCard,
    MemoryRegenerateCard,
  ],
  template: `
    <div>
      <p class="qt-text-small qt-text-muted italic mb-6">
        The Commonplace Book — where each character's remembered truths are kept, pruned, and
        recalled.
      </p>

      <div class="space-y-4">
        <qt-collapsible-card
          title="Embedding Profiles"
          description="Configure embedding models for semantic memory"
          sectionId="embedding-profiles"
          [defaultOpen]="defaultOpen()"
          [forceOpen]="section() === 'embedding-profiles'"
        >
          <qt-embedding-profiles-card />
        </qt-collapsible-card>

        <qt-collapsible-card
          title="Repair Missing Embeddings"
          description="Generate embeddings for legacy memories that predate the embedding-aware gate"
          sectionId="memory-backfill"
          [forceOpen]="section() === 'memory-backfill'"
        >
          <qt-memory-backfill-card />
        </qt-collapsible-card>

        <qt-collapsible-card
          title="Memory Housekeeping"
          description="Automatically prune stale, low-importance memories as characters approach their cap"
          sectionId="memory-housekeeping"
          [forceOpen]="section() === 'memory-housekeeping'"
        >
          <qt-memory-housekeeping-card />
        </qt-collapsible-card>

        <qt-collapsible-card
          title="Recall Relevance"
          description="Keep project-specific memories from wandering into unrelated chats"
          sectionId="memory-recall"
          [forceOpen]="section() === 'memory-recall'"
        >
          <qt-memory-recall-card />
        </qt-collapsible-card>

        <qt-collapsible-card
          title="Memory Deduplication"
          description="Find and remove duplicate memories"
          sectionId="memory-deduplication"
          [forceOpen]="section() === 'memory-deduplication'"
        >
          <qt-memory-dedup-card />
        </qt-collapsible-card>

        <qt-collapsible-card
          title="Regenerate Memories"
          description="Wipe and rebuild every chat-linked memory using the current extraction pipeline"
          sectionId="memory-regenerate"
          [forceOpen]="section() === 'memory-regenerate'"
        >
          <qt-memory-regenerate-card />
        </qt-collapsible-card>

        <qt-collapsible-card
          title="Regenerate Conversation Summaries"
          description="Re-mirror every summarised chat into its characters' vaults for relevant-conversation recall"
          sectionId="conversation-summaries-regenerate"
          [forceOpen]="section() === 'conversation-summaries-regenerate'"
        >
          <qt-conversation-summary-regenerate-card />
        </qt-collapsible-card>
      </div>
    </div>
  `,
})
export class MemoryTab implements OnInit {
  // Optional so the tab renders when hosted as a workspace tab (no ActivatedRoute);
  // v4 ignores `?section=` when hosted, so the deep-link simply falls back to null.
  private readonly route = inject(ActivatedRoute, { optional: true });
  private readonly queryParams = this.route
    ? toSignal(this.route.queryParamMap, { requireSync: true })
    : undefined;

  protected readonly section = computed(() => this.queryParams?.().get('section') ?? null);
  protected readonly defaultOpen = signal(false);

  ngOnInit(): void {
    // Open the first card by default (bound inputs have resolved by ngOnInit).
    this.defaultOpen.set(true);
  }
}
