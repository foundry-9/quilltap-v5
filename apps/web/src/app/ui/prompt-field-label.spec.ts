import { Component, signal } from '@angular/core';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { describe, expect, it } from 'vitest';

import { PromptFieldLabel } from './prompt-field-label';
import { PROMPT_FIELD_HINTS, type PromptFieldHint } from './prompt-field-hints';

/**
 * `qt-prompt-field-label` against v4 `PromptFieldLabel.tsx` (`a6870c5a`): the
 * DOM shape (`div.mb-2` → flex row → helper `<p>` → example `<p>`), the class
 * strings, the `" (Optional)"` / destructive `*` suffixes, the
 * `Written as: <em>` convention, and the override precedence
 * (`label ?? hint.label`, `helper ?? hint.helper`, `example ?? hint.example`).
 */

@Component({
  imports: [PromptFieldLabel],
  template: `
    <qt-prompt-field-label
      [hint]="hint()"
      [label]="label()"
      [optional]="optional()"
      [required]="required()"
      [htmlFor]="htmlFor()"
      [helper]="helper()"
      [example]="example()"
    />
  `,
})
class Host {
  readonly hint = signal<PromptFieldHint | undefined>(undefined);
  readonly label = signal<string | undefined>(undefined);
  readonly optional = signal(false);
  readonly required = signal(false);
  readonly htmlFor = signal<string | undefined>(undefined);
  readonly helper = signal<string | undefined>(undefined);
  readonly example = signal<string | undefined>(undefined);
}

@Component({
  imports: [PromptFieldLabel],
  template: `
    <qt-prompt-field-label [hint]="hint">
      <button type="button" class="the-action">Import Template</button>
    </qt-prompt-field-label>
  `,
})
class ActionsHost {
  readonly hint = PROMPT_FIELD_HINTS.systemPrompt;
}

async function render<T>(host: new () => T): Promise<ComponentFixture<T>> {
  TestBed.configureTestingModule({ imports: [host as never] });
  const fixture = TestBed.createComponent(host);
  fixture.detectChanges();
  await fixture.whenStable();
  fixture.detectChanges();
  return fixture;
}

function labelEl(fixture: ComponentFixture<unknown>): HTMLLabelElement {
  return fixture.nativeElement.querySelector('label') as HTMLLabelElement;
}

function paragraphs(fixture: ComponentFixture<unknown>): HTMLParagraphElement[] {
  return Array.from(fixture.nativeElement.querySelectorAll('p')) as HTMLParagraphElement[];
}

describe('PromptFieldLabel', () => {
  it('renders a hint bundle as label + helper + Written-as example', async () => {
    const fixture = await render(Host);
    fixture.componentInstance.hint.set(PROMPT_FIELD_HINTS.groupInstructions);
    fixture.detectChanges();

    expect(labelEl(fixture).textContent).toBe('Group Instructions');
    expect(labelEl(fixture).className).toBe('block qt-label text-foreground');

    const ps = paragraphs(fixture);
    expect(ps).toHaveLength(2);
    expect(ps[0].textContent).toBe(PROMPT_FIELD_HINTS.groupInstructions.helper);
    expect(ps[0].className).toBe('text-xs qt-text-secondary mt-1');
    expect(ps[1].textContent).toBe(`Written as: ${PROMPT_FIELD_HINTS.groupInstructions.example}`);
    expect(ps[1].querySelector('em')?.textContent).toBe(
      PROMPT_FIELD_HINTS.groupInstructions.example,
    );

    // v4's wrapper: a single `div.mb-2` holding a `flex items-center
    // justify-between` row. It labels the field; it never wraps the editor.
    const root = fixture.nativeElement.querySelector('div.mb-2') as HTMLElement;
    expect(root).toBeTruthy();
    expect((root.firstElementChild as HTMLElement).className).toBe(
      'flex items-center justify-between',
    );
  });

  it('appends " (Optional)" with no intervening space', async () => {
    const fixture = await render(Host);
    fixture.componentInstance.hint.set(PROMPT_FIELD_HINTS.identity);
    fixture.componentInstance.optional.set(true);
    fixture.detectChanges();

    expect(labelEl(fixture).textContent).toBe('Identity (Optional)');
  });

  it('renders the required marker as a destructive-styled " *"', async () => {
    const fixture = await render(Host);
    fixture.componentInstance.hint.set(PROMPT_FIELD_HINTS.systemPrompt);
    fixture.componentInstance.label.set('Content');
    fixture.componentInstance.required.set(true);
    fixture.detectChanges();

    expect(labelEl(fixture).textContent).toBe('Content *');
    const marker = labelEl(fixture).querySelector('span') as HTMLSpanElement;
    expect(marker.className).toBe('qt-text-destructive');
    expect(marker.textContent).toBe(' *');
  });

  it('lets label / helper / example override the hint bundle', async () => {
    const fixture = await render(Host);
    fixture.componentInstance.hint.set(PROMPT_FIELD_HINTS.roleplayTemplatePrompt);
    fixture.componentInstance.label.set('LLM Prompt');
    fixture.componentInstance.helper.set(
      `${PROMPT_FIELD_HINTS.roleplayTemplatePrompt.helper} You can use placeholders like {{char}} and {{user}}.`,
    );
    fixture.componentInstance.example.set('an override example');
    fixture.detectChanges();

    expect(labelEl(fixture).textContent).toBe('LLM Prompt');
    const ps = paragraphs(fixture);
    expect(ps[0].textContent).toBe(
      'Formatting instructions delivered with every character’s prompt in chats using this template. You can use placeholders like {{char}} and {{user}}.',
    );
    expect(ps[1].textContent).toBe('Written as: an override example');
  });

  it('falls back to the hint bundle when no override is given, and to "" with neither', async () => {
    const bare = await render(Host);
    expect(labelEl(bare).textContent).toBe('');
    expect(paragraphs(bare)).toHaveLength(0);
  });

  it('omits the example paragraph for a hint that carries none', async () => {
    const fixture = await render(Host);
    fixture.componentInstance.hint.set(PROMPT_FIELD_HINTS.exampleDialogues);
    fixture.detectChanges();

    const ps = paragraphs(fixture);
    expect(ps).toHaveLength(1);
    expect(ps[0].textContent).toBe(PROMPT_FIELD_HINTS.exampleDialogues.helper);
  });

  it('binds htmlFor onto the <label for> attribute', async () => {
    const fixture = await render(Host);
    fixture.componentInstance.hint.set(PROMPT_FIELD_HINTS.identity);
    fixture.componentInstance.htmlFor.set('identity');
    fixture.detectChanges();

    expect(labelEl(fixture).getAttribute('for')).toBe('identity');
  });

  it('projects actions into the label row (v4 `actions`)', async () => {
    const fixture = await render(ActionsHost);
    const row = fixture.nativeElement.querySelector('div.flex') as HTMLElement;
    const action = row.querySelector('button.the-action');
    expect(action).toBeTruthy();
    expect(action?.textContent?.trim()).toBe('Import Template');
    // The label stays first in the row; the action is right-aligned by
    // `justify-between`, exactly as v4 lays it out.
    expect(row.firstElementChild?.tagName).toBe('LABEL');
  });
});
