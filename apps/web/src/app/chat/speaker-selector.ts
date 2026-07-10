import {
  ChangeDetectionStrategy,
  Component,
  ElementRef,
  HostListener,
  computed,
  inject,
  input,
  output,
  signal,
} from '@angular/core';

import { Avatar } from '../ui/avatar';
import { Icon } from '../ui/icon';

/** One character the user can speak as (v4 `ControlledCharacter`). */
export interface ControlledCharacter {
  participantId: string;
  name: string;
  avatarUrl?: string | null;
}

/**
 * The "Speaking as {name}" dropdown (v4 `components/chat/SpeakerSelector.tsx`),
 * shown above the composer when the user controls two or more characters. Picks
 * which character the user is currently typing as; renders nothing with fewer
 * than two candidates.
 */
@Component({
  selector: 'qt-speaker-selector',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Avatar, Icon],
  template: `
    @if (characters().length >= 2) {
      <div class="relative">
        <button
          type="button"
          class="flex items-center gap-2 px-3 py-2 rounded-lg transition-colors qt-button qt-button-secondary"
          [class.opacity-50]="disabled()"
          [class.cursor-not-allowed]="disabled()"
          [disabled]="disabled()"
          [attr.aria-expanded]="isOpen()"
          aria-haspopup="listbox"
          (click)="toggle()"
        >
          @if (activeCharacter(); as active) {
            <qt-avatar [name]="active.name" [src]="active.avatarUrl" size="xs" />
            <span class="qt-label">Speaking as {{ active.name }}</span>
          } @else {
            <span class="text-sm">Select speaker...</span>
          }
          <qt-icon
            name="chevron-down"
            class="w-4 h-4 transition-transform"
            [class.rotate-180]="isOpen()"
          />
        </button>

        @if (isOpen()) {
          <div
            class="absolute bottom-full left-0 mb-1 w-64 max-h-64 overflow-y-auto qt-card qt-shadow-lg rounded-lg border z-50"
            role="listbox"
          >
            <div class="py-1">
              @for (character of characters(); track character.participantId) {
                <button
                  type="button"
                  class="w-full flex items-center gap-3 px-3 py-2 text-left transition-colors"
                  [class.qt-bg-primary\\/10]="isActive(character)"
                  [class.text-primary]="isActive(character)"
                  role="option"
                  [attr.aria-selected]="isActive(character)"
                  (click)="pick(character.participantId)"
                >
                  <qt-avatar [name]="character.name" [src]="character.avatarUrl" size="xs" />
                  <span class="flex-1 font-medium truncate">{{ character.name }}</span>
                  @if (isActive(character)) {
                    <qt-icon name="check" class="w-4 h-4" />
                  }
                </button>
              }
            </div>
          </div>
        }
      </div>
    }
  `,
})
export class SpeakerSelector {
  readonly characters = input.required<ControlledCharacter[]>();
  readonly activeParticipantId = input<string | null>(null);
  readonly disabled = input(false);

  readonly select = output<string>();

  private readonly host = inject(ElementRef<HTMLElement>);
  protected readonly isOpen = signal(false);

  protected readonly activeCharacter = computed(
    () => this.characters().find((c) => c.participantId === this.activeParticipantId()) ?? null,
  );

  protected isActive(character: ControlledCharacter): boolean {
    return character.participantId === this.activeParticipantId();
  }

  protected toggle(): void {
    if (this.disabled()) return;
    this.isOpen.update((v) => !v);
  }

  protected pick(participantId: string): void {
    this.isOpen.set(false);
    if (participantId === this.activeParticipantId()) {
      return;
    }
    this.select.emit(participantId);
  }

  @HostListener('document:click', ['$event'])
  protected onDocumentClick(event: Event): void {
    if (this.isOpen() && !this.host.nativeElement.contains(event.target as Node)) {
      this.isOpen.set(false);
    }
  }
}
