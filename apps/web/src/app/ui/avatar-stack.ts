import { ChangeDetectionStrategy, Component, computed, input } from '@angular/core';

/** One entity in the stack (v4 `AvatarStackEntity` subset). */
export interface AvatarStackEntity {
  id: string;
  name: string;
  avatarUrl?: string | null;
}

/**
 * A slim port of v4 `components/ui/AvatarStack.tsx` — overlapping participant
 * avatars for a chat card. Shows up to {@link maxDisplay} (v4 default 4) with no
 * `+N` overflow (v4 silently drops the rest). Circular frames, `border-2` against
 * the card, descending z-index so the first participant sits on top.
 */
@Component({
  selector: 'qt-avatar-stack',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    @if (shown().length === 0) {
      <div
        class="flex-shrink-0 flex items-center justify-center rounded-full qt-bg-muted border-2 border-card"
        [style.width.px]="diameter()"
        [style.height.px]="diameter()"
      >
        <span class="font-bold qt-text-secondary text-lg">?</span>
      </div>
    } @else {
      <div class="flex-shrink-0 flex items-center" aria-hidden="false">
        @for (entity of shown(); track entity.id; let i = $index) {
          <div
            class="relative overflow-hidden rounded-full qt-bg-muted border-2 border-card flex items-center justify-center"
            [style.width.px]="diameter()"
            [style.height.px]="diameter()"
            [style.margin-left.px]="i === 0 ? 0 : overlap()"
            [style.zIndex]="shown().length - i"
            [title]="entity.name"
          >
            @if (src(entity)) {
              <img class="w-full h-full object-cover" [src]="src(entity)" [alt]="entity.name" />
            } @else {
              <span class="font-bold qt-text-secondary text-base">{{ initial(entity) }}</span>
            }
          </div>
        }
      </div>
    }
  `,
})
export class AvatarStack {
  readonly entities = input.required<AvatarStackEntity[]>();
  /** Avatar diameter in px (v4 `lg` ≈ 56). */
  readonly diameter = input(56);
  readonly maxDisplay = input(4);

  protected readonly shown = computed(() => this.entities().slice(0, this.maxDisplay()));
  protected readonly overlap = computed(() => Math.round(this.diameter() * -0.35));

  protected src(entity: AvatarStackEntity): string | null {
    return normalizeAvatarSrc(entity.avatarUrl);
  }

  protected initial(entity: AvatarStackEntity): string {
    return (entity.name[0] ?? '?').toUpperCase();
  }
}

/** v4 `getAvatarSrc` string branch: null → null, else ensure a leading slash. */
export function normalizeAvatarSrc(src: string | null | undefined): string | null {
  if (!src) {
    return null;
  }
  return src.startsWith('/') || src.startsWith('http') ? src : `/${src}`;
}
