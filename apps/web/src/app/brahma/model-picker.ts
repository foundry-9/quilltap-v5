/**
 * ModelPicker (port of v4 `components/brahma-console/ModelPicker.tsx`).
 *
 * A small inline dropdown for choosing the Console's connection profile (model),
 * placed in the header so the operator can switch engines mid-conversation — the
 * same chat continues with the new model. Shows provider + model so it's clear
 * which engine is live. Closes on outside click / Escape.
 *
 * @module brahma/model-picker
 */

import {
  ChangeDetectionStrategy,
  Component,
  ElementRef,
  computed,
  inject,
  input,
  output,
  signal,
} from '@angular/core';

import { Icon } from '../ui/icon';
import type { BrahmaConnectionProfile } from './brahma-console.service';

@Component({
  selector: 'qt-brahma-model-picker',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon],
  host: {
    '(document:mousedown)': 'onDocumentMousedown($event)',
    '(document:keydown.escape)': 'close()',
  },
  template: `
    <div class="relative">
      <button
        type="button"
        class="flex items-center gap-1 px-2 py-1 rounded text-xs qt-hover-accent qt-text-secondary transition-colors max-w-[180px]"
        [disabled]="disabled() || profiles().length === 0"
        [title]="buttonTitle()"
        aria-haspopup="listbox"
        [attr.aria-expanded]="open()"
        (click)="toggle()"
      >
        <qt-icon name="brahma-console" class="w-3.5 h-3.5 flex-shrink-0" />
        <span class="truncate">{{ label() }}</span>
        <qt-icon name="chevron-down" class="w-3 h-3 flex-shrink-0" />
      </button>

      @if (open() && profiles().length > 0) {
        <div
          class="absolute right-0 mt-1 z-50 min-w-[220px] max-h-[280px] overflow-y-auto rounded qt-bg-surface qt-border-default border shadow-lg py-1"
          role="listbox"
        >
          @for (profile of profiles(); track profile.id) {
            <button
              type="button"
              role="option"
              [attr.aria-selected]="profile.id === activeId()"
              class="flex items-start gap-2 w-full px-3 py-1.5 text-left text-xs qt-hover-accent transition-colors"
              (click)="pick(profile.id)"
            >
              <qt-icon
                name="check"
                class="w-3.5 h-3.5 mt-0.5 flex-shrink-0"
                [class.opacity-0]="profile.id !== activeId()"
              />
              <span class="flex flex-col min-w-0">
                <span class="truncate font-medium">{{ profile.name }}</span>
                <span class="truncate qt-text-secondary"
                  >{{ profile.provider }} · {{ profile.modelName }}</span
                >
              </span>
            </button>
          }
        </div>
      }
    </div>
  `,
})
export class ModelPicker {
  readonly profiles = input.required<BrahmaConnectionProfile[]>();
  readonly activeId = input<string | null>(null);
  readonly disabled = input(false);
  readonly select = output<string>();

  private readonly rootEl = inject(ElementRef<HTMLElement>);
  protected readonly open = signal(false);

  protected readonly active = computed(
    () => this.profiles().find((p) => p.id === this.activeId()) ?? null,
  );
  protected readonly label = computed(() => this.active()?.name ?? 'Choose a model');
  protected readonly buttonTitle = computed(() => {
    const a = this.active();
    return a ? `${a.name} (${a.provider} · ${a.modelName})` : 'Choose a model';
  });

  protected toggle(): void {
    this.open.update((o) => !o);
  }

  protected close(): void {
    this.open.set(false);
  }

  protected pick(id: string): void {
    this.select.emit(id);
    this.open.set(false);
  }

  protected onDocumentMousedown(event: MouseEvent): void {
    if (!this.open()) return;
    if (!this.rootEl.nativeElement.contains(event.target as Node)) {
      this.open.set(false);
    }
  }
}
