/**
 * A small pure TS port of v4's `{{char}}`/`{{user}}` token substitution
 * (`lib/templates/processor.ts` `processTemplate`) — NOT the full template
 * processor. The list card + the Details tab highlight these tokens in the
 * character's own copy.
 *
 * v4 semantics preserved: `{{var}}` matched by `/\{\{(\w+)\}\}/g`; a present,
 * non-null value substitutes (stringified), an absent one collapses to the empty
 * string (SillyTavern behaviour); the `{{trim}}…{{/trim}}` macro strips
 * surrounding newlines.
 */

/** The substitution context (the subset the characters screens use). */
export interface TemplateContext {
  char?: string;
  user?: string;
  description?: string;
  manifesto?: string;
  personality?: string;
  scenario?: string;
  persona?: string;
  system?: string;
  [key: string]: string | undefined;
}

/** Replace `{{var}}` tokens and the `{{trim}}` macro (v4 `processTemplate`). */
export function processTemplate(template: string, context: TemplateContext): string {
  if (!template) {
    return '';
  }

  let result = template.replace(/\{\{(\w+)\}\}/g, (_match, variable: string) => {
    const value = context[variable];
    if (value !== undefined && value !== null) {
      return String(value);
    }
    return '';
  });

  result = result.replace(/\{\{trim\}\}([\s\S]*?)\{\{\/trim\}\}/g, (_match, content: string) =>
    content.replace(/^\n+|\n+$/g, ''),
  );

  return result;
}

/**
 * v4's list-card / detail resolution of the `{{user}}` token: a
 * user-controlled character speaks as itself, otherwise the default partner
 * name, otherwise the literal "User" (v4 `AuroraView.tsx:498-505`).
 */
export function resolveUserToken(
  controlledBy: 'llm' | 'user' | undefined,
  characterName: string,
  defaultPartnerName: string | null | undefined,
): string {
  return controlledBy === 'user' ? characterName : defaultPartnerName || 'User';
}
