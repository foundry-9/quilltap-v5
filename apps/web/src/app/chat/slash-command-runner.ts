/**
 * Immediate runs for the composer's `/` commands (v4
 * `components/chat/useSlashCommandRunner.ts`, `4877fc2da`) — a tool that
 * declares no parameters has nothing to complete, so picking it runs it at once
 * through the same verbs the dialogs use. "A private run is never reachable from
 * here; the dialog is the place for that."
 *
 * Also the quick rolls (v4 `lexical/slash-commands/context.ts`'s
 * `buildQuickRollActions`): `/d6`, `/d20`, `/coin`, `/bottle` are the dice
 * button's own preview request, landing as the same pending-result chip. They
 * live here rather than beside the plugin because their one dependency is the
 * transport, and `editor/slash/` stays transport-free.
 *
 * ### Recorded divergences
 *
 * - **`RUNNER_ERROR_ARMS`.** v4 has two failure arms for a built-in (`!res.ok ||
 *   !data.success` → `data.error || data.message || fallback`; a throw →
 *   `err.message`). v5's `runTool` / `runCustomTool` THROW on a refused
 *   dispatch carrying the server's text, so both arms are one catch:
 *   `coreErrorMessage(err, fallback)` — the server's text verbatim, v4's
 *   fallback for a throw that carries none.
 * - **`CUSTOM_RUN_PARAMETERS_OMITTED`.** v4 posts `parameters: null`; v5's
 *   `runCustomTool` omits the key. The Rust verb reads it as an `Option` through
 *   v4's `.nullish()`, so absent and `null` are the same request. `private` is
 *   omitted on both sides: "the definition's own default visibility decides".
 *
 * @module chat/slash-command-runner
 */

import type { QueryClient } from '@tanstack/angular-query-experimental';

import { coreErrorMessage, type CoreClient } from '../core/core-client';
import type { ToastService } from '../ui/toast.service';
import type { SlashCustomToolRef } from '../editor/slash/slash-command-host';
import type { ComposerAction } from './slash-commands';
import { runTool } from './chat-admin.api';
import { chatKeys } from './chat-keys';
import { customToolsKeys, runCustomTool } from './custom-tools.api';
import { requestRngPendingResult, type RngPendingResult, type RngRequestArgs } from './rng-request';

export interface SlashCommandRunnerOptions {
  core: CoreClient;
  queryClient: QueryClient;
  toasts: Pick<ToastService, 'showSuccess' | 'showError'>;
  chatId: string;
  /** The character a built-in runs as — Run Tool's default, the first active character seat. */
  defaultCharacterId: string | null;
  /** Called after a successful run so the Salon can refetch the chat. */
  onRan?: () => void;
}

export interface SlashCommandRunner {
  runBuiltInNow(toolId: string): Promise<void>;
  runCustomNow(tool: SlashCustomToolRef): Promise<void>;
}

export function createSlashCommandRunner(options: SlashCommandRunnerOptions): SlashCommandRunner {
  const { core, queryClient, toasts, chatId, defaultCharacterId, onRan } = options;

  return {
    async runBuiltInNow(toolId: string) {
      try {
        await runTool(core, {
          chatId,
          toolName: toolId,
          arguments: {},
          characterId: defaultCharacterId || undefined,
          private: false,
        });
      } catch (err) {
        toasts.showError(coreErrorMessage(err, `${toolId} could not be run`));
        return;
      }
      toasts.showSuccess(`Prospero has run ${toolId}.`);
      onRan?.();
    },

    async runCustomNow(tool: SlashCustomToolRef) {
      try {
        await runCustomTool(core, chatId, {
          tool: tool.name,
          // `parameters` and `private` omitted — see CUSTOM_RUN_PARAMETERS_OMITTED.
          asCharacterId: tool.asCharacterId ?? undefined,
        });
      } catch (err) {
        toasts.showError(coreErrorMessage(err, `${tool.name} could not be run`));
        return;
      }
      // The NAME — the dialog toasts the title.
      toasts.showSuccess(`Pascal has settled ${tool.name}.`);
      void queryClient.invalidateQueries({ queryKey: chatKeys.detail(chatId) });
      void queryClient.invalidateQueries({ queryKey: customToolsKeys.byChat(chatId) });
      onRan?.();
    },
  };
}

/** v4 `QUICK_ROLLS`. */
export const QUICK_ROLLS: Readonly<Record<'d6' | 'd20' | 'coin' | 'bottle', RngRequestArgs>> = {
  d6: { type: 6, rolls: 1 },
  d20: { type: 20, rolls: 1 },
  coin: { type: 'flip_coin', rolls: 1 },
  bottle: { type: 'spin_the_bottle', rolls: 1 },
};

/**
 * `/d6`, `/d20`, `/coin`, `/bottle`: the dice button's own preview request,
 * landing as the same pending-result chip.
 */
export function buildQuickRollActions(
  core: CoreClient,
  chatId: string,
  onPendingResult: (result: RngPendingResult) => void,
  toasts: Pick<ToastService, 'showError'>,
): Partial<Record<ComposerAction, () => void>> {
  const actions: Partial<Record<ComposerAction, () => void>> = {};
  for (const [action, args] of Object.entries(QUICK_ROLLS) as Array<
    [keyof typeof QUICK_ROLLS, RngRequestArgs]
  >) {
    actions[action] = () => {
      requestRngPendingResult(core, chatId, args)
        .then(onPendingResult)
        .catch((err: unknown) => {
          toasts.showError(coreErrorMessage(err, 'The dice would not roll'));
        });
    };
  }
  return actions;
}
