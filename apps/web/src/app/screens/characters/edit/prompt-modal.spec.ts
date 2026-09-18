import { ComponentFixture, TestBed } from '@angular/core/testing';
import { describe, expect, it } from 'vitest';

import { PromptModal } from './prompt-modal';

/**
 * v4 `baa85e19b` (bug 154) widened this dialog 2xl → 4xl, because its content
 * box could not hold the markdown editor's toolbar: the right half ran off the
 * dialog with nothing to scroll.
 *
 * The pin is on the rendered width rather than the attribute, so it survives
 * the token being renamed and reddens if the token's own value moves: `qt-modal`
 * maps `4xl` to `56rem` (`ui/modal.ts`'s `MAX_WIDTHS`, which already carried the
 * token — the Library file picker is its first consumer, so nothing in the
 * shared component had to move for this).
 */
async function render(): Promise<ComponentFixture<PromptModal>> {
  TestBed.configureTestingModule({ imports: [PromptModal] });
  const fixture = TestBed.createComponent(PromptModal);
  fixture.detectChanges();
  await fixture.whenStable();
  fixture.detectChanges();
  return fixture;
}

describe('PromptModal width (v4 baa85e19b)', () => {
  it('renders at the 4xl width, wide enough for the markdown toolbar', async () => {
    const fixture = await render();
    const dialog = fixture.nativeElement.querySelector('.qt-dialog') as HTMLElement;
    expect(dialog).toBeTruthy();
    // 56rem is `4xl`; the pre-fix `2xl` was 42rem.
    expect(dialog.style.maxWidth).toBe('56rem');
  });
});
