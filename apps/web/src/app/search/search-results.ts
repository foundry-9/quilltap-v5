import { ChangeDetectionStrategy, Component, computed, inject, input, output } from '@angular/core';
import { Router } from '@angular/router';

import { HighlightedText } from './highlighted-text';
import { groupResultsByType, mapResultUrl } from './search.logic';
import {
  TYPE_ICONS,
  TYPE_LABELS,
  TYPE_LABELS_PLURAL,
  type CharacterSearchResult,
  type ChatSearchResult,
  type MemorySearchResult,
  type MessageSearchResult,
  type SearchResult,
  type SearchType,
  type TagSearchResult,
} from './search.types';

/**
 * The search result list (v4 `components/search/search-results.tsx`): loading
 * spinner / empty state / results grouped by type in first-encounter order,
 * with the per-type `(shown/total)` header — a button opening the dialog
 * filtered to that type when the caller wires `typeCountClick` and more rows
 * exist than are shown.
 *
 * Navigation: each row is an `<a>` whose href is the SERVER's URL mapped onto
 * the v5 route space (`mapResultUrl` — the documented divergences live there);
 * the click intercepts into `router.navigateByUrl` so query strings survive.
 *
 * OMITTED (dead in v4 itself): the `matchedTag` / `matchedViaCharacter` card
 * arms and the character card's `avatarUrl` image arm — v4's
 * `/api/v1/ui/search` route never emits any of the three fields (they are
 * relics of an older search shape in `types.ts`), so the arms cannot render;
 * v5 ports the reachable rendering.
 */
@Component({
  selector: 'qt-search-results',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [HighlightedText],
  template: `
    @if (isLoading()) {
      <div class="p-6 text-center">
        <div class="inline-block animate-spin rounded-full h-8 w-8 qt-spinner"></div>
        <p class="mt-2 qt-text-small">Searching...</p>
      </div>
    } @else if (results().length === 0) {
      <div class="p-6 text-center">
        <p class="qt-text-small">No results found for "{{ query() }}"</p>
        <p class="qt-text-xs mt-1">Try searching for characters, chats, messages, tags, or memories</p>
      </div>
    } @else {
      <div class="divide-y divide-border">
        @for (group of grouped(); track group.type) {
          <div class="py-2">
            <div
              class="px-3 py-1 qt-text-xs font-semibold uppercase tracking-wider flex items-center gap-1"
            >
              <span>{{ icon(group.type) }} {{ labelPlural(group.type) }}</span>
              @if (hasTypeCountClick() && totalFor(group.type) > group.results.length) {
                <button
                  type="button"
                  (click)="typeCountClick.emit(group.type)"
                  class="text-primary hover:underline cursor-pointer"
                  [title]="'Show all ' + totalFor(group.type) + ' ' + labelPlural(group.type).toLowerCase()"
                >
                  ({{ group.results.length }}/{{ totalFor(group.type) }})
                </button>
              } @else {
                <span>({{ totalFor(group.type) }})</span>
              }
            </div>
            <div class="space-y-1">
              @for (result of group.results; track result.type + '-' + result.id) {
                <a
                  [href]="href(result)"
                  (click)="go($event, result)"
                  class="block p-3 qt-hover-accent rounded-lg transition-colors"
                >
                  @switch (result.type) {
                    @case ('characters') {
                      <div class="flex items-start gap-3">
                        <div
                          class="w-10 h-10 rounded-full qt-bg-primary/10 flex items-center justify-center flex-shrink-0"
                        >
                          <span class="text-lg">{{ icon('characters') }}</span>
                        </div>
                        <div class="flex-1 min-w-0">
                          <div class="flex items-center gap-2">
                            <span class="qt-text-primary truncate">{{ result.name }}</span>
                            @if (asCharacter(result).isFavorite) {
                              <span class="qt-text-warning">★</span>
                            }
                            <span class="text-xs px-1.5 py-0.5 rounded qt-badge-character">
                              {{ label('characters') }}
                            </span>
                          </div>
                          @if (asCharacter(result).title) {
                            <p class="qt-text-xs truncate">{{ asCharacter(result).title }}</p>
                          }
                          <p class="qt-text-small mt-1 line-clamp-2">
                            <qt-highlighted-text [text]="result.snippet" [query]="query()" />
                          </p>
                        </div>
                      </div>
                    }
                    @case ('chats') {
                      <div class="flex items-start gap-3">
                        <div
                          class="w-10 h-10 rounded-full qt-bg-info/10 flex items-center justify-center flex-shrink-0"
                        >
                          <span class="text-lg">{{ icon('chats') }}</span>
                        </div>
                        <div class="flex-1 min-w-0">
                          <div class="flex items-center gap-2 flex-wrap">
                            <span class="qt-text-primary truncate">{{ result.name }}</span>
                            <span class="text-xs px-1.5 py-0.5 rounded qt-badge-chat">
                              {{ label('chats') }}
                            </span>
                          </div>
                          <div class="flex items-center gap-2 qt-text-xs">
                            @if ((asChat(result).characterNames ?? []).length > 0) {
                              <span>with {{ asChat(result).characterNames!.join(', ') }}</span>
                            }
                            @if (asChat(result).messageCount !== undefined) {
                              <span>• {{ asChat(result).messageCount }} messages</span>
                            }
                          </div>
                          <p class="qt-text-small mt-1 line-clamp-2">
                            <qt-highlighted-text [text]="result.snippet" [query]="query()" />
                          </p>
                        </div>
                      </div>
                    }
                    @case ('tags') {
                      <div class="flex items-start gap-3">
                        <div
                          class="w-10 h-10 rounded-full qt-bg-warning/10 flex items-center justify-center flex-shrink-0"
                        >
                          <span class="text-lg">{{ icon('tags') }}</span>
                        </div>
                        <div class="flex-1 min-w-0">
                          <div class="flex items-center gap-2">
                            <span class="qt-text-primary">
                              <qt-highlighted-text [text]="result.name" [query]="query()" />
                            </span>
                            <span class="text-xs px-1.5 py-0.5 rounded qt-badge-tag">
                              {{ label('tags') }}
                            </span>
                            @if (asTag(result).quickHide) {
                              <span class="qt-text-xs px-1.5 py-0.5 qt-bg-muted rounded">Quick Hide</span>
                            }
                          </div>
                          <p class="qt-text-small mt-1">
                            Used {{ asTag(result).usageCount }} time{{
                              asTag(result).usageCount === 1 ? '' : 's'
                            }}
                            across characters and chats
                          </p>
                        </div>
                      </div>
                    }
                    @case ('memories') {
                      <div class="flex items-start gap-3">
                        <div
                          class="w-10 h-10 rounded-full qt-bg-destructive/10 flex items-center justify-center flex-shrink-0"
                        >
                          <span class="text-lg">{{ icon('memories') }}</span>
                        </div>
                        <div class="flex-1 min-w-0">
                          <div class="flex items-center gap-2">
                            <span class="qt-text-primary truncate">{{ result.name }}</span>
                            <span class="text-xs px-1.5 py-0.5 rounded qt-badge-memory">
                              {{ label('memories') }}
                            </span>
                            <span
                              class="text-xs px-1.5 py-0.5 rounded"
                              [class.qt-badge-manual]="asMemory(result).source === 'MANUAL'"
                              [class.qt-badge-auto]="asMemory(result).source !== 'MANUAL'"
                            >
                              {{ asMemory(result).source === 'MANUAL' ? 'Manual' : 'Auto' }}
                            </span>
                          </div>
                          @if (asMemory(result).characterName) {
                            <p class="qt-text-xs">Memory for {{ asMemory(result).characterName }}</p>
                          }
                          <p class="qt-text-small mt-1 line-clamp-2">
                            <qt-highlighted-text [text]="result.snippet" [query]="query()" />
                          </p>
                          <div class="flex items-center gap-2 mt-1">
                            <div class="flex items-center gap-1">
                              <span class="qt-text-xs">Importance:</span>
                              <div class="w-16 h-1.5 bg-border rounded-full overflow-hidden">
                                <div
                                  class="h-full bg-destructive rounded-full"
                                  [style.width.%]="asMemory(result).importance * 100"
                                ></div>
                              </div>
                            </div>
                          </div>
                        </div>
                      </div>
                    }
                    @case ('messages') {
                      <div class="flex items-start gap-3">
                        <div
                          class="w-10 h-10 rounded-full qt-bg-warning/10 flex items-center justify-center flex-shrink-0"
                        >
                          <span class="text-lg">{{ icon('messages') }}</span>
                        </div>
                        <div class="flex-1 min-w-0">
                          <div class="flex items-center gap-2 flex-wrap">
                            <span class="qt-text-primary truncate">{{ asMessage(result).chatTitle }}</span>
                            <span class="text-xs px-1.5 py-0.5 rounded qt-badge-message">
                              {{ label('messages') }}
                            </span>
                            <span
                              class="text-xs px-1.5 py-0.5 rounded"
                              [class.qt-badge-info]="asMessage(result).role === 'USER'"
                              [class.qt-badge-character]="asMessage(result).role !== 'USER'"
                            >
                              {{ asMessage(result).role === 'USER' ? 'You' : 'Character' }}
                            </span>
                          </div>
                          @if ((asMessage(result).characterNames ?? []).length > 0) {
                            <p class="qt-text-xs">with {{ asMessage(result).characterNames!.join(', ') }}</p>
                          }
                          <p class="qt-text-small mt-1 line-clamp-2">
                            <qt-highlighted-text [text]="result.snippet" [query]="query()" />
                          </p>
                        </div>
                      </div>
                    }
                  }
                </a>
              }
            </div>
          </div>
        }
      </div>
    }
  `,
})
export class SearchResults {
  private readonly router = inject(Router);

  readonly results = input.required<SearchResult[]>();
  readonly query = input.required<string>();
  readonly isLoading = input<boolean>(false);
  /** Total counts per type (from the API, before pagination). */
  readonly countsByType = input<Partial<Record<SearchType, number>>>({});
  /** Wire to enable the `(shown/total)` filter buttons (the bar's dropdown does). */
  readonly hasTypeCountClick = input<boolean>(false);
  readonly resultClick = output<void>();
  readonly typeCountClick = output<SearchType>();

  protected readonly grouped = computed(() => groupResultsByType(this.results()));

  protected icon(type: SearchType): string {
    return TYPE_ICONS[type];
  }
  protected label(type: SearchType): string {
    return TYPE_LABELS[type];
  }
  protected labelPlural(type: SearchType): string {
    return TYPE_LABELS_PLURAL[type];
  }
  /** v4: `countsByType?.[type] ?? typeResults.length`. */
  protected totalFor(type: SearchType): number {
    return this.countsByType()[type] ?? this.grouped().find((g) => g.type === type)!.results.length;
  }

  protected href(result: SearchResult): string {
    return mapResultUrl(result.url);
  }

  protected go(event: MouseEvent, result: SearchResult): void {
    event.preventDefault();
    this.resultClick.emit();
    void this.router.navigateByUrl(mapResultUrl(result.url));
  }

  protected asCharacter(r: SearchResult): CharacterSearchResult {
    return r as CharacterSearchResult;
  }
  protected asChat(r: SearchResult): ChatSearchResult {
    return r as ChatSearchResult;
  }
  protected asTag(r: SearchResult): TagSearchResult {
    return r as TagSearchResult;
  }
  protected asMemory(r: SearchResult): MemorySearchResult {
    return r as MemorySearchResult;
  }
  protected asMessage(r: SearchResult): MessageSearchResult {
    return r as MessageSearchResult;
  }
}
