import { ChangeDetectionStrategy, Component, computed, input } from '@angular/core';

export type AvatarSize = 'xs' | 'sm' | 'md' | 'lg' | 'xl' | 'chat';

interface SizeConfig {
  w: number;
  h: number;
  text: string;
}

const SIZE_CONFIGS: Record<AvatarSize, SizeConfig> = {
  xs: { w: 32, h: 32, text: 'text-xs' },
  sm: { w: 40, h: 40, text: 'text-sm' },
  md: { w: 48, h: 60, text: 'text-lg' },
  lg: { w: 80, h: 100, text: 'text-3xl' },
  xl: { w: 128, h: 160, text: 'text-5xl' },
  chat: { w: 120, h: 150, text: 'text-4xl' },
};

/**
 * A slim port of v4 `components/ui/Avatar.tsx` (foundation subset — the
 * `useAvatarDisplay` style context + queue badge + labels are vertical concerns).
 * Renders an image or the name's initial in a rounded / rectangular frame.
 */
@Component({
  selector: 'qt-avatar',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div class="relative flex-shrink-0" [style.width.px]="config().w" [style.height.px]="config().h">
      <div [class]="frameClass()" style="width: 100%; height: 100%;">
        @if (src()) {
          <img class="w-full h-full object-cover" [src]="src()" [alt]="name()" />
        } @else {
          <span [class]="'font-bold qt-text-secondary ' + config().text">{{ initial() }}</span>
        }
      </div>
    </div>
  `,
})
export class Avatar {
  readonly name = input.required<string>();
  readonly src = input<string | null | undefined>(undefined);
  readonly size = input<AvatarSize>('md');
  readonly circular = input(false);
  readonly active = input(false);

  protected readonly config = computed(() => SIZE_CONFIGS[this.size()]);
  protected readonly initial = computed(() => (this.name()[0] ?? '?').toUpperCase());
  protected readonly frameClass = computed(() => {
    const parts = ['overflow-hidden', 'qt-bg-muted', 'flex', 'items-center', 'justify-center'];
    if (this.circular()) {
      parts.push('rounded-full');
    }
    if (this.active()) {
      parts.push('ring-2', 'ring-primary', 'ring-offset-1', 'ring-offset-card');
    }
    return parts.join(' ');
  });
}
