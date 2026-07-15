import { ComponentFixture, TestBed } from '@angular/core/testing';
import { By } from '@angular/platform-browser';
import { describe, expect, it } from 'vitest';

import { CoreClient } from '../../../core/core-client';
import type { ProjectDetail } from '../../../core/core-contract';
import { RichEditor } from '../../../editor/rich-editor';
import type { ProjectEditForm } from './project-header';
import { ProjectSettingsCard } from './project-settings-card';

function project(over: Partial<ProjectDetail> = {}): ProjectDetail {
  return {
    id: 'p1',
    name: 'The Estate',
    description: null,
    instructions: '',
    isActive: true,
    allowAnyCharacter: false,
    characters: [],
    files: [],
    createdAt: '2024-01-01T00:00:00.000Z',
    updatedAt: '2024-01-01T00:00:00.000Z',
    ...over,
  } as ProjectDetail;
}

async function render(
  instructions: string,
): Promise<ComponentFixture<ProjectSettingsCard>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [ProjectSettingsCard],
    providers: [{ provide: CoreClient, useValue: { dispatchData: async () => ({}) } }],
  });
  const fixture = TestBed.createComponent(ProjectSettingsCard);
  fixture.componentRef.setInput('project', project());
  fixture.componentRef.setInput('form', { instructions } as ProjectEditForm);
  fixture.componentRef.setInput('defaultOpen', true);
  fixture.detectChanges();
  await settle(fixture);
  return fixture;
}

async function settle(fixture: ComponentFixture<unknown>): Promise<void> {
  for (let i = 0; i < 4; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

function editor(fixture: ComponentFixture<unknown>): RichEditor {
  return fixture.debugElement.query(By.directive(RichEditor)).componentInstance as RichEditor;
}

describe('ProjectSettingsCard — the instructions markdown field', () => {
  it('seeds the field from the form', async () => {
    const fixture = await render('Keep the *tone* dry.');
    expect(editor(fixture).getMarkdown()).toBe('Keep the *tone* dry.');
  });

  it('emits an edit as an instructions formChange patch', async () => {
    const fixture = await render('Keep the tone dry.');
    const patches: Partial<ProjectEditForm>[] = [];
    fixture.componentInstance.formChange.subscribe((p) => patches.push(p));

    editor(fixture).setMarkdown('Keep the tone **dry**, and the rain out.');
    await settle(fixture);

    expect(patches).toEqual([{ instructions: 'Keep the tone **dry**, and the rain out.' }]);
  });

  it('does not emit a patch on load (no dirty-on-load)', async () => {
    // __dry__ normalizes to **dry** on parse; the absorb-once seam must swallow
    // that, or merely opening the card would mark the project form dirty.
    const fixture = await render('Keep it __dry__.');
    const patches: Partial<ProjectEditForm>[] = [];
    fixture.componentInstance.formChange.subscribe((p) => patches.push(p));
    await settle(fixture);
    expect(patches).toEqual([]);
  });
});
