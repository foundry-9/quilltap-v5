import { ComponentFixture, TestBed } from '@angular/core/testing';
import { describe, expect, it } from 'vitest';

import { ProviderOptionsPanel } from '../providers/provider-options-panel';
import type {
  ProviderOptionField,
  ProviderOptionsSchema,
} from '../providers/provider-options-schema';
import recorded from './__fixtures__/openai-image-options-schemas.json';

/**
 * OpenAI's image-profile options schema, as v4's plugin builds it and the
 * shared panel must render it. VERIFY-ONLY on the rendering side: v4
 * `d8d2890ee` (PR #62) adds no bespoke editor code for OpenAI — the plugin
 * simply starts declaring `getImageProviderOptionsSchema`, so every field
 * below has to come out of the EXISTING schema-driven panel with zero new
 * client code. A red here would mean the generic panel cannot express this
 * schema, which is a §S.3 event, not a spec to relax.
 *
 * Unlike `nanogpt-options.spec.ts`, whose fixture is a TRANSCRIPTION of the
 * plugin's `index.ts`, this fixture is RECORDED from v4's REAL
 * `getOpenAIImageOptionsSchema` at the `5f0a57dc4` pin — see
 * `apps/web/oracle/openai-image-options.recorder.ts` for the invocation. The
 * server half of this round (P4.D196) records the same function's output at
 * the same pin for its own differential and the unifier diffs the two
 * recordings (§R.10(c)), so the two halves cannot drift into agreement by
 * accident.
 *
 * ⚠ Two things the panel deliberately does NOT draw, in both apps:
 *   - an enum value's `description` (v4's `EnumField` renders `option.label`
 *     alone — `ProviderOptionsPanel.tsx:192-196`), so the description
 *     assertions below read the recorded SCHEMA. They still matter: the
 *     descriptions are what tells sunburst's quality list apart from
 *     `gpt-image-2`'s, and losing them would be a silent recording failure.
 *   - a `number` field's min/max — the schema declares none for
 *     `output_compression` and the renderer emits no bounds attributes at
 *     all; the assertion pins that the box is unbounded, which is what lets a
 *     user type the 0-100 the help text describes.
 */

const SCHEMAS = recorded as unknown as Record<string, ProviderOptionsSchema>;

/** v4 `ARBITRARY_SIZE_RULES.experimentalAbovePixels` (`image-models.ts`). */
const EXPERIMENTAL_ABOVE_PIXELS = 2560 * 1440;

const EXPERIMENTAL_DESCRIPTION =
  'Experimental resolution — slower, and the model may decline it.';

function schemaFor(model: string): ProviderOptionsSchema {
  const schema = SCHEMAS[model];
  // A missing key means the recorder's input list moved without the spec's:
  // fail by name rather than let `undefined` cascade into vacuous assertions.
  expect(schema, `recorded schema for ${model}`).toBeDefined();
  return schema;
}

function groupTitles(model: string): (string | undefined)[] {
  return schemaFor(model).groups.map((g) => g.title);
}

function fieldsOf(model: string, groupTitle: string): ProviderOptionField[] {
  const group = schemaFor(model).groups.find((g) => g.title === groupTitle);
  expect(group, `group ${groupTitle} of ${model}`).toBeDefined();
  return (group as { fields: ProviderOptionField[] }).fields;
}

function fieldOf(model: string, groupTitle: string, key: string): ProviderOptionField {
  const field = fieldsOf(model, groupTitle).find((f) => f.key === key);
  expect(field, `field ${key} in ${groupTitle} of ${model}`).toBeDefined();
  return field as ProviderOptionField;
}

async function renderPanel(
  model: string,
  parameters: Record<string, unknown> = {},
): Promise<ComponentFixture<ProviderOptionsPanel>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({ imports: [ProviderOptionsPanel] });
  const fixture = TestBed.createComponent(ProviderOptionsPanel);
  fixture.componentRef.setInput('schema', schemaFor(model));
  fixture.componentRef.setInput('parameters', parameters);
  fixture.componentRef.setInput('fetchedModels', []);
  fixture.componentRef.setInput('modelName', model === '(no model)' ? '' : model);
  fixture.detectChanges();
  await fixture.whenStable();
  fixture.detectChanges();
  return fixture;
}

/** Every `<select>` the panel drew, keyed by the field id the panel assigns. */
function selectsById(fixture: ComponentFixture<ProviderOptionsPanel>): Map<string, HTMLSelectElement> {
  const nodes = Array.from(
    (fixture.nativeElement as HTMLElement).querySelectorAll('select'),
  ) as HTMLSelectElement[];
  return new Map(nodes.map((s) => [s.id, s]));
}

function selectFor(
  fixture: ComponentFixture<ProviderOptionsPanel>,
  key: string,
): HTMLSelectElement {
  const select = selectsById(fixture).get(`pof-${key}`);
  expect(select, `rendered select for ${key}`).toBeDefined();
  return select as HTMLSelectElement;
}

describe('the recorded corpus itself', () => {
  it('carries the five §R.10(c) inputs', () => {
    expect(Object.keys(SCHEMAS)).toEqual([
      'gpt-image-2.5-sunburst',
      'gpt-image-2',
      'dall-e-3',
      'dall-e-2',
      '(no model)',
    ]);
  });

  it('answers the no-model call with the first table entry’s schema (v4 `getOpenAIImageOptionsSchema`’s `?? OPENAI_IMAGE_MODELS[0]`)', () => {
    expect(SCHEMAS['(no model)']).toEqual(SCHEMAS['gpt-image-2.5-sunburst']);
  });

  it('leads every enum with the blank “(model default)” and defaults to it', () => {
    for (const [model, schema] of Object.entries(SCHEMAS)) {
      for (const group of schema.groups) {
        for (const field of group.fields) {
          if (field.type !== 'enum') continue;
          const values = field.enumValues ?? [];
          expect(values.length, `${model}/${field.key} has options`).toBeGreaterThan(1);
          expect(values[0], `${model}/${field.key} leading option`).toEqual({
            value: '',
            label: '(model default)',
          });
          expect(field.default, `${model}/${field.key} default`).toBe('');
        }
      }
    }
  });
});

describe('field labels — keys and order were pinned; the labels are the third thing the order asked for', () => {
  // Added at the `53294163f` unification (§3 review): every field's `label` is
  // rendered as the control's `<label>`, and until this block nothing read one.
  const IMAGE_PARAMS = ['Quality', 'Default Size'];
  const OUTPUT = ['Background', 'Output Format', 'Output Compression', 'Moderation'];

  it('spells every field label as v4 does, per family', () => {
    for (const model of ['gpt-image-2.5-sunburst', 'gpt-image-2', '(no model)']) {
      expect(fieldsOf(model, 'Image Parameters').map((f) => f.label)).toEqual(IMAGE_PARAMS);
      expect(fieldsOf(model, 'GPT Image Output').map((f) => f.label)).toEqual(OUTPUT);
    }
    expect(fieldsOf('dall-e-3', 'Image Parameters').map((f) => f.label)).toEqual([
      ...IMAGE_PARAMS,
      'Style',
    ]);
    expect(fieldsOf('dall-e-2', 'Image Parameters').map((f) => f.label)).toEqual(IMAGE_PARAMS);
  });

  it('renders each label as the <label> of its own control', async () => {
    const fixture = await renderPanel('gpt-image-2.5-sunburst');
    const host = fixture.nativeElement as HTMLElement;
    const expected: Array<[string, string]> = [
      ['pof-quality', 'Quality'],
      ['pof-size', 'Default Size'],
      ['pof-background', 'Background'],
      ['pof-output_format', 'Output Format'],
      ['pof-output_compression', 'Output Compression'],
      ['pof-moderation', 'Moderation'],
    ];
    for (const [id, text] of expected) {
      const label = host.querySelector(`label[for="${id}"]`);
      expect(label, `rendered label for ${id}`).not.toBeNull();
      expect(label?.textContent?.trim()).toBe(text);
    }
  });
});

describe('GPT Image 2.5 Sunburst', () => {
  const MODEL = 'gpt-image-2.5-sunburst';

  it('groups the fields as Image Parameters then GPT Image Output', () => {
    expect(groupTitles(MODEL)).toEqual(['Image Parameters', 'GPT Image Output']);
    expect(fieldsOf(MODEL, 'Image Parameters').map((f) => f.key)).toEqual(['quality', 'size']);
    expect(fieldsOf(MODEL, 'GPT Image Output').map((f) => f.key)).toEqual([
      'background',
      'output_format',
      'output_compression',
      'moderation',
    ]);
  });

  it('renders the six quality tiers with v4’s labels, blank first', async () => {
    const fixture = await renderPanel(MODEL);
    const options = Array.from(selectFor(fixture, 'quality').options);
    expect(options.map((o) => o.value)).toEqual([
      '',
      'auto',
      'low',
      'medium',
      'high',
      'xhigh',
      'max',
    ]);
    expect(options.map((o) => o.textContent?.trim())).toEqual([
      '(model default)',
      'Auto — the model chooses',
      'Low — fastest and cheapest',
      'Medium',
      'High',
      'Extra High',
      'Max — the finest the model offers',
    ]);
  });

  it('carries the premium-tier descriptions on xhigh and max, and on nothing else', () => {
    const quality = fieldOf(MODEL, 'Image Parameters', 'quality');
    const described = (quality.enumValues ?? []).filter((v) => v.description);
    expect(described.map((v) => v.value)).toEqual(['xhigh', 'max']);
    expect(described[0].description).toBe('GPT Image 2.5 only. Slower and dearer than High.');
    expect(described[1].description).toBe(
      'GPT Image 2.5 only. The slowest and most expensive tier.',
    );
  });

  it('spells the premium help text on the quality field', () => {
    expect(fieldOf(MODEL, 'Image Parameters', 'quality').helpText).toBe(
      'How much effort the model spends on the image. Extra High and Max are GPT Image 2.5’s premium tiers — sharper detail, at a higher price and a longer wait.',
    );
  });

  it('renders the thirteen wide sizes with shape-first labels, blank first', async () => {
    const fixture = await renderPanel(MODEL);
    const options = Array.from(selectFor(fixture, 'size').options);
    expect(options.map((o) => o.value)).toEqual([
      '',
      'auto',
      '1024x1024',
      '1536x1024',
      '1024x1536',
      '1792x1024',
      '1024x1792',
      '1920x1088',
      '1088x1920',
      '2048x2048',
      '2560x1440',
      '1440x2560',
      '3840x2160',
      '2160x3840',
    ]);
    expect(options.map((o) => o.textContent?.trim())).toEqual([
      '(model default)',
      'Auto — the model chooses the shape',
      'Square (1024×1024)',
      'Landscape (1536×1024)',
      'Portrait (1024×1536)',
      'Landscape (1792×1024)',
      'Portrait (1024×1792)',
      'Landscape (1920×1088)',
      'Portrait (1088×1920)',
      'Square (2048×2048)',
      'Landscape (2560×1440)',
      'Portrait (1440×2560)',
      'Landscape (3840×2160)',
      'Portrait (2160×3840)',
    ]);
  });

  /**
   * ⚠ The work order predicted "exactly the two >2560×1440 sizes". Measured at
   * the pin it is THREE: `2048x2048` is 4,194,304 pixels, over v4's
   * 2560×1440 = 3,686,400 threshold, so it is flagged experimental alongside
   * the two 4K entries. The rule below is derived, not listed, so it cannot go
   * stale the way the prediction did.
   */
  it('flags exactly the sizes over 2560×1440 as experimental (three, not the ordered two)', () => {
    const size = fieldOf(MODEL, 'Image Parameters', 'size');
    const expected = (size.enumValues ?? [])
      .filter((v) => {
        const match = /^(\d+)x(\d+)$/.exec(v.value);
        return match ? Number(match[1]) * Number(match[2]) > EXPERIMENTAL_ABOVE_PIXELS : false;
      })
      .map((v) => v.value);
    expect(expected).toEqual(['2048x2048', '3840x2160', '2160x3840']);
    const described = (size.enumValues ?? []).filter((v) => v.description);
    expect(described.map((v) => v.value)).toEqual(expected);
    for (const value of described) {
      expect(value.description, value.value).toBe(EXPERIMENTAL_DESCRIPTION);
    }
  });

  it('spells the arbitrary-size help text with v4’s numbers', () => {
    expect(fieldOf(MODEL, 'Image Parameters', 'size').helpText).toBe(
      'Default dimensions for this profile. This model also accepts any size whose edges divide by 16, at an aspect ratio between 1:3 and 3:1, up to 3840×2160 — the list below is a selection, not the limit. Asking for a portrait or landscape image in chat overrides this.',
    );
  });

  it('offers no Style row — style is DALL·E 3’s alone', async () => {
    expect(fieldsOf(MODEL, 'Image Parameters').map((f) => f.key)).not.toContain('style');
    const fixture = await renderPanel(MODEL);
    expect(selectsById(fixture).has('pof-style')).toBe(false);
  });

  it('renders the GPT Image Output group with its heading and group help text', async () => {
    const fixture = await renderPanel(MODEL);
    const text = (fixture.nativeElement as HTMLElement).textContent ?? '';
    expect(text).toContain('GPT Image Output');
    expect(text).toContain(
      'Parameters the GPT Image families accept. DALL·E models ignore this section because it is not offered for them.',
    );
  });

  it('renders background, output_format and moderation with v4’s options', async () => {
    const fixture = await renderPanel(MODEL);
    expect(Array.from(selectFor(fixture, 'background').options).map((o) => o.value)).toEqual([
      '',
      'auto',
      'opaque',
      'transparent',
    ]);
    expect(Array.from(selectFor(fixture, 'output_format').options).map((o) => o.value)).toEqual([
      '',
      'png',
      'webp',
      'jpeg',
    ]);
    expect(
      Array.from(selectFor(fixture, 'output_format').options).map((o) => o.textContent?.trim()),
    ).toEqual([
      '(model default)',
      'PNG — lossless, supports transparency',
      'WebP — smaller, supports transparency',
      'JPEG — smallest, no transparency',
    ]);
    expect(Array.from(selectFor(fixture, 'moderation').options).map((o) => o.value)).toEqual([
      '',
      'auto',
      'low',
    ]);
  });

  it('renders output_compression as an unbounded number box with its help text', async () => {
    const compression = fieldOf(MODEL, 'GPT Image Output', 'output_compression');
    expect(compression.type).toBe('number');
    // v4 declares neither bound and no default; the panel must therefore emit
    // no `min`/`max`/`step` at all and no placeholder.
    expect(Object.keys(compression).sort()).toEqual(['helpText', 'key', 'label', 'type']);

    const fixture = await renderPanel(MODEL);
    const input = (fixture.nativeElement as HTMLElement).querySelector(
      '#pof-output_compression',
    ) as HTMLInputElement | null;
    expect(input, 'rendered number input for output_compression').not.toBeNull();
    const box = input as HTMLInputElement;
    expect(box.type).toBe('number');
    expect(box.getAttribute('min')).toBeNull();
    expect(box.getAttribute('max')).toBeNull();
    expect(box.getAttribute('step')).toBeNull();
    expect(box.getAttribute('placeholder')).toBeNull();
    expect((fixture.nativeElement as HTMLElement).textContent).toContain(
      'Compression level from 0 to 100, applied only when the output format is WebP or JPEG. Higher keeps more detail; the model defaults to 100. Ignored for PNG.',
    );
  });

  it('preselects a stored value on every control it draws', async () => {
    const fixture = await renderPanel(MODEL, {
      quality: 'max',
      size: '2048x2048',
      background: 'transparent',
      output_format: 'webp',
      output_compression: 80,
      moderation: 'low',
    });
    expect(selectFor(fixture, 'quality').value).toBe('max');
    expect(selectFor(fixture, 'size').value).toBe('2048x2048');
    expect(selectFor(fixture, 'background').value).toBe('transparent');
    expect(selectFor(fixture, 'output_format').value).toBe('webp');
    expect(selectFor(fixture, 'moderation').value).toBe('low');
    const box = (fixture.nativeElement as HTMLElement).querySelector(
      '#pof-output_compression',
    ) as HTMLInputElement;
    expect(box.value).toBe('80');
  });
});

describe('GPT Image 2 — the same groups, the four-tier quality list', () => {
  const MODEL = 'gpt-image-2';

  it('renders only the four GPT Image tiers, with no premium descriptions', async () => {
    const fixture = await renderPanel(MODEL);
    const options = Array.from(selectFor(fixture, 'quality').options);
    expect(options.map((o) => o.value)).toEqual(['', 'auto', 'low', 'medium', 'high']);
    const quality = fieldOf(MODEL, 'Image Parameters', 'quality');
    expect((quality.enumValues ?? []).filter((v) => v.description)).toEqual([]);
  });

  it('spells the non-premium quality help text', () => {
    expect(fieldOf(MODEL, 'Image Parameters', 'quality').helpText).toBe(
      'How much effort the model spends on the image. Higher tiers cost more and take longer.',
    );
  });

  it('keeps the wide size list — the arbitrary-size families share it', () => {
    const sunburst = fieldOf('gpt-image-2.5-sunburst', 'Image Parameters', 'size');
    expect(fieldOf(MODEL, 'Image Parameters', 'size')).toEqual(sunburst);
  });

  it('offers no Style row', async () => {
    expect(fieldsOf(MODEL, 'Image Parameters').map((f) => f.key)).toEqual(['quality', 'size']);
    const fixture = await renderPanel(MODEL);
    expect(selectsById(fixture).has('pof-style')).toBe(false);
  });

  it('still carries the GPT Image Output group, field for field', () => {
    expect(fieldsOf(MODEL, 'GPT Image Output')).toEqual(
      fieldsOf('gpt-image-2.5-sunburst', 'GPT Image Output'),
    );
  });
});

describe('DALL·E 3 — Style, and no GPT Image Output', () => {
  const MODEL = 'dall-e-3';

  it('draws one group only', async () => {
    expect(groupTitles(MODEL)).toEqual(['Image Parameters']);
    const fixture = await renderPanel(MODEL);
    const text = (fixture.nativeElement as HTMLElement).textContent ?? '';
    expect(text).toContain('Image Parameters');
    expect(text).not.toContain('GPT Image Output');
    for (const key of ['background', 'output_format', 'moderation']) {
      expect(selectsById(fixture).has(`pof-${key}`), `select for ${key}`).toBe(false);
    }
    expect(
      (fixture.nativeElement as HTMLElement).querySelector('#pof-output_compression'),
    ).toBeNull();
  });

  it('offers standard and hd alone', async () => {
    const fixture = await renderPanel(MODEL);
    const options = Array.from(selectFor(fixture, 'quality').options);
    expect(options.map((o) => o.value)).toEqual(['', 'standard', 'hd']);
    expect(options.map((o) => o.textContent?.trim())).toEqual([
      '(model default)',
      'Standard',
      'HD — finer detail',
    ]);
  });

  it('offers the three DALL·E 3 sizes, none experimental', async () => {
    const fixture = await renderPanel(MODEL);
    const options = Array.from(selectFor(fixture, 'size').options);
    expect(options.map((o) => o.value)).toEqual(['', '1024x1024', '1792x1024', '1024x1792']);
    expect((fieldOf(MODEL, 'Image Parameters', 'size').enumValues ?? []).filter((v) => v.description)).toEqual(
      [],
    );
  });

  it('spells the plain size help text — no arbitrary-size paragraph', () => {
    expect(fieldOf(MODEL, 'Image Parameters', 'size').helpText).toBe(
      'Default dimensions for this profile. Asking for a portrait or landscape image in chat overrides this.',
    );
  });

  it('draws the Style row with v4’s two values and its help text', async () => {
    expect(fieldsOf(MODEL, 'Image Parameters').map((f) => f.key)).toEqual([
      'quality',
      'size',
      'style',
    ]);
    const fixture = await renderPanel(MODEL);
    const options = Array.from(selectFor(fixture, 'style').options);
    expect(options.map((o) => o.value)).toEqual(['', 'vivid', 'natural']);
    expect(options.map((o) => o.textContent?.trim())).toEqual([
      '(model default)',
      'Vivid — dramatic, hyper-real',
      'Natural — realistic, less exaggerated',
    ]);
    expect((fixture.nativeElement as HTMLElement).textContent).toContain(
      'DALL·E 3 only. Vivid leans dramatic and hyper-real; Natural is more restrained.',
    );
  });
});

describe('DALL·E 2 — one quality, three small sizes, no style', () => {
  const MODEL = 'dall-e-2';

  it('draws one group with quality and size alone', async () => {
    expect(groupTitles(MODEL)).toEqual(['Image Parameters']);
    expect(fieldsOf(MODEL, 'Image Parameters').map((f) => f.key)).toEqual(['quality', 'size']);
    const fixture = await renderPanel(MODEL);
    expect(selectsById(fixture).has('pof-style')).toBe(false);
    expect((fixture.nativeElement as HTMLElement).textContent).not.toContain('GPT Image Output');
  });

  it('offers standard alone', async () => {
    const fixture = await renderPanel(MODEL);
    const options = Array.from(selectFor(fixture, 'quality').options);
    expect(options.map((o) => o.value)).toEqual(['', 'standard']);
    expect(options.map((o) => o.textContent?.trim())).toEqual(['(model default)', 'Standard']);
  });

  it('offers the three DALL·E 2 sizes with shape-first labels', async () => {
    const fixture = await renderPanel(MODEL);
    const options = Array.from(selectFor(fixture, 'size').options);
    expect(options.map((o) => o.value)).toEqual(['', '256x256', '512x512', '1024x1024']);
    expect(options.map((o) => o.textContent?.trim())).toEqual([
      '(model default)',
      'Square (256×256)',
      'Square (512×512)',
      'Square (1024×1024)',
    ]);
  });
});
