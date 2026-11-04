import { ChangeDetectionStrategy, Component, model } from '@angular/core';

import { FONT_OPTIONS, type FullAddOns } from './template-form';

/** Per-instance id seed for the label ↔ select pairs (v4 uses `useId()`). */
let addOnsEditorCounter = 0;

/**
 * The flourish checkboxes and selects, shared by every delimiter and by the
 * unmarked-text style — v4 `AddOnsEditor` (`roleplay-templates/index.tsx:44-110`,
 * `778b97b2b` at 236c04e35). Two-way bound to a {@link FullAddOns}; every change
 * replaces the object. Each select owns a per-instance id behind a
 * `<label for>` (WCAG rule R4 — v4's `htmlFor={`${id}-underline`}` shape).
 */
@Component({
  selector: 'qt-template-add-ons-editor',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div>
      <p class="qt-text-xs qt-text-secondary">Flourishes</p>
      <div class="flex flex-wrap items-center gap-x-3 gap-y-1 mt-1">
        <label class="flex items-center gap-1 cursor-pointer">
          <input
            type="checkbox"
            class="qt-checkbox"
            [checked]="addOns().bold"
            (change)="set('bold', $any($event.target).checked)"
          />
          <span class="text-xs">Bold</span>
        </label>
        <label class="flex items-center gap-1 cursor-pointer">
          <input
            type="checkbox"
            class="qt-checkbox"
            [checked]="addOns().italic"
            (change)="set('italic', $any($event.target).checked)"
          />
          <span class="text-xs">Italic</span>
        </label>
        <label class="flex items-center gap-1 cursor-pointer" title="Swap foreground and background">
          <input
            type="checkbox"
            class="qt-checkbox"
            [checked]="addOns().reverse"
            (change)="set('reverse', $any($event.target).checked)"
          />
          <span class="text-xs">Reverse</span>
        </label>
      </div>
      <div class="grid grid-cols-3 gap-2 mt-2">
        <div>
          <label class="qt-text-xs" [for]="id + '-underline'">Underline</label>
          <select
            class="qt-select w-full text-sm"
            [id]="id + '-underline'"
            [value]="addOns().underline"
            (change)="set('underline', $any($event.target).value)"
          >
            <option value="none">None</option>
            <option value="single">Single</option>
            <option value="double">Double</option>
          </select>
        </div>
        <div>
          <label class="qt-text-xs" [for]="id + '-border'">Border</label>
          <select
            class="qt-select w-full text-sm"
            [id]="id + '-border'"
            [value]="addOns().border"
            (change)="set('border', $any($event.target).value)"
          >
            <option value="none">None</option>
            <option value="solid">Solid</option>
            <option value="dashed">Dashed</option>
          </select>
        </div>
        <div>
          <label class="qt-text-xs" [for]="id + '-font'">Font</label>
          <select
            class="qt-select w-full text-sm"
            [id]="id + '-font'"
            [value]="addOns().font"
            (change)="set('font', $any($event.target).value)"
          >
            @for (opt of fontOptions; track opt.value) {
              <option [value]="opt.value">{{ opt.label }}</option>
            }
          </select>
        </div>
      </div>
    </div>
  `,
})
export class TemplateAddOnsEditor {
  readonly addOns = model.required<FullAddOns>();

  protected readonly id = `qt-add-ons-${++addOnsEditorCounter}`;
  protected readonly fontOptions = FONT_OPTIONS;

  protected set<K extends keyof FullAddOns>(key: K, value: FullAddOns[K]): void {
    this.addOns.set({ ...this.addOns(), [key]: value });
  }
}
