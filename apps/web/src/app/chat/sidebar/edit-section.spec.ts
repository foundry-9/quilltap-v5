import { TestBed } from '@angular/core/testing';
import { describe, expect, it } from 'vitest';

import { EditSection } from './edit-section';

/** v4 `ChatSidebar.tsx:1737-1799` `EditContentSection` — four buttons, v4's order. */
function mount(count: number) {
  const f = TestBed.createComponent(EditSection);
  f.componentRef.setInput('memoryCount', count);
  f.detectChanges();
  const el = f.nativeElement as HTMLElement;
  const buttons = Array.from(el.querySelectorAll('button'));
  return { f, buttons };
}

describe('EditSection', () => {
  it('renders Replace, Bulk Replace, Re-extract Memories, Delete Memories (n) in v4 order', () => {
    const { buttons } = mount(3);
    expect(buttons.map((b) => b.textContent!.trim())).toEqual([
      'Replace',
      'Bulk Replace',
      'Re-extract Memories',
      'Delete Memories (3)',
    ]);
    expect(buttons[2].title).toBe('Re-extract memories');
    expect(buttons[3].title).toBe('Delete chat memories');
    expect(buttons[3].classList.contains('qt-tool-palette-button-danger')).toBe(true);
    expect(buttons[3].disabled).toBe(false);
  });

  it('Delete Memories is disabled at zero with the v4 title (M4)', () => {
    const { buttons } = mount(0);
    expect(buttons[3].disabled).toBe(true);
    expect(buttons[3].title).toBe('This chat has laid down no memories yet');
    expect(buttons[3].textContent!.trim()).toBe('Delete Memories (0)');
  });

  it('emits the two memory outputs', () => {
    const { f, buttons } = mount(2);
    let re = 0;
    let del = 0;
    f.componentInstance.reextractMemories.subscribe(() => re++);
    f.componentInstance.deleteMemories.subscribe(() => del++);
    buttons[2].click();
    buttons[3].click();
    expect([re, del]).toEqual([1, 1]);
  });
});
