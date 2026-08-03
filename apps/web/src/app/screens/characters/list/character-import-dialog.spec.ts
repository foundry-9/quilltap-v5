import { ComponentFixture, TestBed } from '@angular/core/testing';
import { describe, expect, it } from 'vitest';

import { CoreClient } from '../../../core/core-client';
import { ToastService } from '../../../ui/toast.service';
import { CharacterImportDialog } from './character-import-dialog';

function toasts(): { type: string; message: string }[] {
  return TestBed.inject(ToastService)
    .toasts()
    .map((t) => ({ type: t.type, message: t.message }));
}

interface DispatchReq {
  type: string;
  payload?: unknown;
  [k: string]: unknown;
}

function stubClient(onDispatch?: (req: DispatchReq) => void): Partial<CoreClient> {
  return {
    dispatchData: (async (req: DispatchReq) => {
      onDispatch?.(req);
      if (req.type === 'characterImport') {
        return { character: { id: 'new-1', name: 'Imported' } };
      }
      return {};
    }) as CoreClient['dispatchData'],
  };
}

/** A File whose `.text()` resolves the given content (the test DOM lacks the
 *  Blob.text() the production dialog calls, so we supply it per instance). */
function jsonFile(name: string, content: string): File {
  const file = new File([content], name, { type: 'application/json' });
  Object.defineProperty(file, 'text', { value: async () => content });
  return file;
}

async function render(
  client: Partial<CoreClient>,
): Promise<ComponentFixture<CharacterImportDialog>> {
  TestBed.configureTestingModule({
    imports: [CharacterImportDialog],
    providers: [{ provide: CoreClient, useValue: client }],
  });
  const fixture = TestBed.createComponent(CharacterImportDialog);
  fixture.detectChanges();
  return fixture;
}

describe('CharacterImportDialog', () => {
  it('reads a JSON file, dispatches characterImport with the parsed payload, and emits imported', async () => {
    const seen: DispatchReq[] = [];
    const fixture = await render(stubClient((r) => seen.push(r)));
    const cmp = fixture.componentInstance as unknown as {
      onPick(files: FileList): void;
      submit(): Promise<void>;
    };

    let importedEmitted = false;
    let closeEmitted = false;
    fixture.componentInstance.imported.subscribe(() => (importedEmitted = true));
    fixture.componentInstance.close.subscribe(() => (closeEmitted = true));

    const card = { name: 'Imported', description: 'from ST' };
    const file = jsonFile('imported.json', JSON.stringify(card));
    cmp.onPick([file] as unknown as FileList);
    await cmp.submit();

    const call = seen.find((r) => r.type === 'characterImport');
    expect(call).toBeTruthy();
    expect(call?.payload).toEqual(card);
    expect(importedEmitted).toBe(true);
    expect(closeEmitted).toBe(true);
    // v4 `AuroraView.tsx:306` — the import success toast.
    expect(toasts()).toEqual([{ type: 'success', message: 'Character imported successfully!' }]);
  });

  it('toasts v4\'s fixed sentence when the JSON file is malformed (no inline surface, v4 :291-311)', async () => {
    const fixture = await render(stubClient());
    const cmp = fixture.componentInstance as unknown as {
      onPick(files: FileList): void;
      submit(): Promise<void>;
    };
    const bad = jsonFile('broken.json', '{not valid json');
    cmp.onPick([bad] as unknown as FileList);
    await cmp.submit();
    fixture.detectChanges();
    expect(toasts()).toEqual([
      {
        type: 'error',
        message:
          "Failed to import character. Make sure it's a valid SillyTavern PNG or JSON file.",
      },
    ]);
    expect(fixture.nativeElement.textContent).not.toContain('Failed to import character');
  });
});
