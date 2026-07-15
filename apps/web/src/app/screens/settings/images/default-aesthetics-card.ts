import { ChangeDetectionStrategy, Component, inject } from '@angular/core';

import { CoreClient } from '../../../core/core-client';
import { AestheticEditorField } from '../../../ui/aesthetic-editor-field';
import {
  fetchSystemAesthetic,
  setSystemAesthetic,
  systemAestheticKeys,
  type AestheticKind,
} from './system-aesthetics.api';

/**
 * The Default Aesthetics card body (v4 `ImagesTabContent.tsx:52-66`, the third
 * card): two {@link AestheticEditorField}s over the Quilltap General document
 * store — the global house style every project inherits and may override.
 *
 * The two aesthetics are NOT interchangeable, and the copy says why: `lantern`
 * is the look of the SCENE (medium, era, palette, rendering), `aurora` is how
 * PEOPLE and their outfits are depicted. Both feed story backgrounds and ad-hoc
 * images; only `aurora` feeds plain avatars.
 *
 * The Images tab owns the collapsible wrapper (v4 puts it there too), so this is
 * just v4's `space-y-6` field pair. Saving empty content deletes the underlying
 * file and restores the fallback — the server's semantics, not the field's.
 */
@Component({
  selector: 'qt-default-aesthetics-card',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [AestheticEditorField],
  template: `
    <div class="space-y-6">
      <qt-aesthetic-editor-field
        label="Default Image Aesthetic"
        description="The overall look for scenes and backgrounds (medium, era, palette, rendering). Feeds story backgrounds and ad-hoc images."
        [queryKey]="lanternKey"
        [load]="loadLantern"
        [save]="saveLantern"
      />
      <qt-aesthetic-editor-field
        label="Default Character Aesthetic"
        description="How people and their outfits are depicted. Feeds avatars, plus the figures rendered in story backgrounds and ad-hoc images."
        [queryKey]="auroraKey"
        [load]="loadAurora"
        [save]="saveAurora"
      />
    </div>
  `,
})
export class DefaultAestheticsCard {
  private readonly core = inject(CoreClient);

  protected readonly lanternKey = systemAestheticKeys.aesthetic('lantern');
  protected readonly auroraKey = systemAestheticKeys.aesthetic('aurora');

  // Stable arrow properties: a fresh closure each render would churn the query's
  // identity. v4 keys each field by its `loadUrl`; these are the same two keys.
  protected readonly loadLantern = (): Promise<string> => this.load('lantern');
  protected readonly saveLantern = (content: string): Promise<void> =>
    this.save('lantern', content);
  protected readonly loadAurora = (): Promise<string> => this.load('aurora');
  protected readonly saveAurora = (content: string): Promise<void> => this.save('aurora', content);

  private load(kind: AestheticKind): Promise<string> {
    return fetchSystemAesthetic(this.core, kind);
  }

  private save(kind: AestheticKind, content: string): Promise<void> {
    return setSystemAesthetic(this.core, kind, content);
  }
}
