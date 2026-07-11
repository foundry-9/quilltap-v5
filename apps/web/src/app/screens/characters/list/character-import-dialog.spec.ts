import { ComponentFixture, TestBed } from '@angular/core/testing';
import { describe, expect, it } from 'vitest';

import { CoreClient } from '../../../core/core-client';
import { CharacterImportDialog } from './character-import-dialog';

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
  });

  it('surfaces a friendly error when the JSON file is malformed', async () => {
    const fixture = await render(stubClient());
    const cmp = fixture.componentInstance as unknown as {
      onPick(files: FileList): void;
      submit(): Promise<void>;
    };
    const bad = jsonFile('broken.json', '{not valid json');
    cmp.onPick([bad] as unknown as FileList);
    await cmp.submit();
    fixture.detectChanges();
    expect(fixture.nativeElement.textContent).toContain('Failed to import character');
  });
});
