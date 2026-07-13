import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { ConflictResolution, FileConflictInfo } from './chat-files.api';
import { FileConflictDialog } from './file-conflict-dialog';

function conflict(over: Partial<FileConflictInfo> = {}): FileConflictInfo {
  return {
    conflictType: 'filename',
    existingFile: { id: 'old', filename: 'report.pdf', size: 2048, createdAt: '2026-01-01', sha256: 'a' },
    newFile: { filename: 'report.pdf', size: 3072, sha256: 'b' },
    ...over,
  };
}

function render(info: FileConflictInfo): ComponentFixture<FileConflictDialog> {
  TestBed.configureTestingModule({ imports: [FileConflictDialog] });
  const fixture = TestBed.createComponent(FileConflictDialog);
  fixture.componentRef.setInput('conflict', info);
  fixture.detectChanges();
  return fixture;
}

describe('FileConflictDialog', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('describes a filename conflict and shows both files', () => {
    const fixture = render(conflict());
    const text = fixture.nativeElement.textContent as string;
    expect(text).toContain('A file with this name already exists');
    expect(text).toContain('Existing File');
    expect(text).toContain('New File');
    expect(text).toContain('2.0 KB');
    expect(text).toContain('3.0 KB');
  });

  it('describes a content conflict', () => {
    const fixture = render(conflict({ conflictType: 'content' }));
    expect(fixture.nativeElement.textContent).toContain('already exists in the project (with a different name)');
  });

  it.each<[string, ConflictResolution]>([
    ['Skip Upload', 'skip'],
    ['Keep Both', 'keepBoth'],
    ['Replace', 'replace'],
  ])('emits %s → %s', (label, resolution) => {
    const fixture = render(conflict());
    const resolve = vi.fn();
    fixture.componentInstance.resolve.subscribe(resolve);
    const btn = Array.from(fixture.nativeElement.querySelectorAll('.qt-dialog-footer button')).find(
      (b) => (b as HTMLButtonElement).textContent?.trim() === label,
    ) as HTMLButtonElement;
    btn.click();
    expect(resolve).toHaveBeenCalledWith(resolution);
  });
});
