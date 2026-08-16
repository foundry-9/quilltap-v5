/**
 * The provider-options schema each LLM plugin declares for the
 * connection-profile editor (v4
 * `packages/plugin-types/src/plugins/provider-options.ts`).
 *
 * The wire carries it per provider on the providers listing as
 * `optionsSchema: ProviderOptionsSchema | null` — `null` exactly when the
 * plugin declares no schema. The SPA contract types that field as
 * `unknown | null` (`core-contract.ts` `ProviderInfo.optionsSchema`); these
 * declarations are the client-side narrowing, written from the shared
 * contract text rather than from the server's own structs.
 *
 * Every `key` is a storage key inside the profile's `parameters` blob and must
 * round-trip untouched: the renderer reads and writes the flat bag, and the
 * provider reads the same key off `profileParameters` at call time.
 */

/** Supported field types (v4 `ProviderOptionFieldType`). */
export type ProviderOptionFieldType = 'boolean' | 'enum' | 'multi-enum' | 'string' | 'number';

/** One option in an `enum` or `multi-enum` field (v4 `ProviderOptionEnumValue`). */
export interface ProviderOptionEnumValue {
  /** Stored value (also written back into the parameters map). */
  value: string;
  /** Human-readable label displayed in the UI. */
  label: string;
  /**
   * Optional secondary description. Declared by Ollama's `thinking_effort` and
   * `keep_alive` values, and rendered by NEITHER app: v4's `EnumField` draws
   * `option.label` alone (`ProviderOptionsPanel.tsx:192-196`). Carried in the
   * type because the wire carries it.
   */
  description?: string;
}

/**
 * A field marked as affecting something outside the panel (v4
 * `ProviderOptionDirective`). `modelInput` swaps the host's model control
 * between selector and free text.
 */
export type ProviderOptionDirective = 'modelInput';

/** Conditional render guard (v4 `ProviderOptionShowIf`). */
export interface ProviderOptionShowIf {
  field: string;
  equals: unknown;
}

/** A single configurable field (v4 `ProviderOptionField`). */
export interface ProviderOptionField {
  /** Storage key inside the profile's `parameters` blob. */
  key: string;
  /** Label rendered above the input. */
  label: string;
  /** Optional help text rendered under the input. */
  helpText?: string;
  /** Field type — determines which control the renderer uses. */
  type: ProviderOptionFieldType;
  /**
   * Value shown when the bag holds nothing for this key. A DISPLAY fallback
   * only: neither app writes it into the bag until the control is touched (v4
   * `fieldValue`, `ProviderOptionsPanel.tsx:43-47`).
   */
  default?: unknown;
  /** Choices for `enum`, and for `multi-enum` without a `multiEnumSource`. */
  enumValues?: ProviderOptionEnumValue[];
  /** Runtime source for `multi-enum` options instead of a fixed list. */
  multiEnumSource?: 'fetchedModels';
  /** Maximum number of selected entries for `multi-enum` fields. */
  max?: number;
  /** Render only when a sibling key in the same bag equals the given value. */
  showIf?: ProviderOptionShowIf;
  /** Mark this field as affecting an external piece of the host UI. */
  affects?: ProviderOptionDirective;
  /**
   * Reserved for the model-keyed gating follow-up. Intentionally not consumed
   * by either renderer (v4's comment: "Renderers that don't understand this
   * field should render unconditionally").
   */
  appliesToModels?: string[];
}

/** A visually grouped set of fields (v4 `ProviderOptionGroup`). */
export interface ProviderOptionGroup {
  /** Optional section heading rendered above the group. */
  title?: string;
  /** Optional help text rendered under the group's fields. */
  helpText?: string;
  /** Fields in render order. */
  fields: ProviderOptionField[];
}

/** The full schema a plugin declares (v4 `ProviderOptionsSchema`). */
export interface ProviderOptionsSchema {
  groups: ProviderOptionGroup[];
}
