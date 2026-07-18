import {
  ChangeDetectionStrategy,
  Component,
  DestroyRef,
  afterNextRender,
  computed,
  inject,
  input,
  signal,
  viewChild,
  type ElementRef,
} from '@angular/core';
import { RouterLink } from '@angular/router';

import type { CharacterConnectionProfile } from '../../core/core-contract';
import { QuickHideService } from '../../quick-hide/quick-hide.service';
import { HomeCharacterCard } from './home-character-card';
import type { HomepageCharacter } from './home.api';

// Card dimensions for calculating grid fit (v4 CharactersSection.tsx)
const CARD_WIDTH_ESTIMATE = 150; // Card max width
const CARD_HEIGHT_ESTIMATE = 185; // Card height including 2-line description
const GRID_GAP = 12; // gap-3 = 0.75rem = 12px
// Minimum characters to show even if they don't all fit
const MIN_CHARACTERS = 2;

/**
 * The homepage Characters section (v4
 * `components/homepage/CharactersSection.tsx`): header + "View all" into the
 * roster (v4 `/aurora` → v5 `/characters`), the empty arm, and the card grid.
 * A `ResizeObserver` on the content box recomputes how many complete cards fit
 * (v4's fit math verbatim: floor per axis, ≥2 columns, ≥1 row, ≥2 cards;
 * 4 cards until first measured) — overflow stays hidden, not scrolled.
 *
 * Quick-hide filtering matches v4 `CharactersSection.tsx:29-32` exactly: hide a
 * character when ANY of its tags is hidden. Order matters — v4 filters FIRST and
 * then slices to what fits (`:72-75`), and the empty arm gates on the FILTERED
 * list (`:86`), so hiding every character shows the empty state rather than a
 * blank grid.
 */
@Component({
  selector: 'qt-characters-section',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [RouterLink, HomeCharacterCard],
  template: `
    <div class="qt-homepage-section">
      <div class="qt-homepage-section-header">
        <h2 class="qt-homepage-section-title">Characters</h2>
        <a routerLink="/characters" class="qt-homepage-section-link">View all &rarr;</a>
      </div>
      <div #content class="qt-homepage-section-content">
        @if (visibleCharacters().length === 0) {
          <div class="text-center py-6 qt-text-secondary">
            <p class="text-sm">No favorite characters</p>
            <a routerLink="/characters" class="text-xs text-primary hover:underline">
              Mark some as favorites
            </a>
          </div>
        } @else {
          <div class="qt-characters-grid">
            @for (character of displayedCharacters(); track character.id) {
              <qt-home-character-card
                [character]="character"
                [profile]="profileFor(character.defaultConnectionProfileId ?? null)"
              />
            }
          </div>
        }
      </div>
    </div>
  `,
})
export class CharactersSection {
  readonly characters = input.required<HomepageCharacter[]>();
  /** The shared `connectionProfileList` result (fetched once by the screen). */
  readonly profiles = input<CharacterConnectionProfile[]>([]);

  private readonly content = viewChild.required<ElementRef<HTMLElement>>('content');
  private readonly maxCards = signal<number | null>(null);
  private readonly quickHide = inject(QuickHideService);

  constructor() {
    const destroyRef = inject(DestroyRef);
    // afterNextRender is browser-only, like v4's effect-on-mount.
    afterNextRender(() => {
      // Initial calculation
      this.calculateFittingCards();

      // ResizeObserver may not be available in test environments
      if (typeof ResizeObserver === 'undefined') {
        return;
      }
      const resizeObserver = new ResizeObserver(() => this.calculateFittingCards());
      resizeObserver.observe(this.content().nativeElement);
      destroyRef.onDestroy(() => resizeObserver.disconnect());
    });
  }

  /** v4 `calculateFittingCards`: complete rows × columns, floored per axis. */
  private calculateFittingCards(): void {
    const container = this.content().nativeElement;
    const availableWidth = container.clientWidth;
    const availableHeight = container.clientHeight;

    // Each card needs CARD_WIDTH + GAP (except last card doesn't need trailing gap)
    const columnsCanFit = Math.max(
      2,
      Math.floor((availableWidth + GRID_GAP) / (CARD_WIDTH_ESTIMATE + GRID_GAP)),
    );
    const rowsCanFit = Math.max(
      1,
      Math.floor((availableHeight + GRID_GAP) / (CARD_HEIGHT_ESTIMATE + GRID_GAP)),
    );

    this.maxCards.set(Math.max(MIN_CHARACTERS, rowsCanFit * columnsCanFit));
  }

  /** v4 `:29-32` — drop characters carrying a hidden tag, before the fit slice. */
  protected readonly visibleCharacters = computed(() =>
    this.characters().filter((character) => !this.quickHide.shouldHideByIds(character.tags ?? [])),
  );

  /** Only show characters that fit (v4 `:72-75`: default to 4 until measured). */
  protected readonly displayedCharacters = computed(() => {
    const max = this.maxCards();
    const list = this.visibleCharacters();
    return max === null ? list.slice(0, 4) : list.slice(0, max);
  });

  protected profileFor(id: string | null): CharacterConnectionProfile | null {
    if (!id) {
      return null;
    }
    return this.profiles().find((p) => p.id === id) ?? null;
  }
}
