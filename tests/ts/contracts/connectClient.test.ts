/**
 * The Endpoint page's Test it and Connect a client say what the CLI prints.
 * `gglib-cli`'s client-block tests record a few proofs and two of the
 * daemon's recipe answers beside the lines `render_proof` and
 * `render_client_config` make of them, in `contracts/proxy/`; the page's
 * readers (`src/readings/connect.ts`) must make the same words.
 *
 * The page draws no terminal mark and no columns. A proof's line is the
 * CLI's after its two-space indent and its mark (`✓` answered, `!` not);
 * a recipe's lines are the CLI's once laid out as the CLI lays them: the
 * client in 24 columns before its hint, each label padded to the widest
 * label (at least 8) before its value.
 *
 * Nothing here reads Rust source.
 */

import { describe, it, expect } from 'vitest';

import { proofAnswered, proofFailed, recipesShown } from '../../../src/readings/connect';
import type { ClientRecipes } from '../../../src/types/generated/ClientRecipes';
import type { ProofOutcome } from '../../../src/types/generated/ProofOutcome';

import { rust } from './rustSource';

interface RecordedProof {
  answered: ProofOutcome | null;
  failed: string | null;
  line: string;
}
interface RecordedConfig {
  recipes: ClientRecipes;
  lines: string[];
}

const PROOFS = JSON.parse(rust('contracts/proxy/proof.json')) as RecordedProof[];
const CONFIGS = JSON.parse(rust('contracts/proxy/client_config.json')) as RecordedConfig[];

describe('the proof sentence is the line the CLI records', () => {
  it('records answers, an answer under another name, and a refusal', () => {
    expect(PROOFS.some((p) => p.answered && p.answered.answered_by !== p.answered.asked_for)).toBe(true);
    expect(PROOFS.some((p) => p.failed !== null)).toBe(true);
  });

  it.each(PROOFS.map((p) => [p.line, p] as const))('%s', (_line, recorded) => {
    if (recorded.answered) {
      expect(`  ✓ ${proofAnswered(recorded.answered)}`).toBe(recorded.line);
    } else {
      expect(`  ! ${proofFailed(recorded.failed ?? '')}`).toBe(recorded.line);
    }
  });
});

/** The CLI's recipe lines, in order: the client lines and the value lines, without the curl. */
function cliRecipeLines(lines: string[]): string[] {
  return lines.filter((l) => /^ {4}\S/.test(l) && l !== '    Check it from another shell:');
}
function cliValueLines(lines: string[]): string[] {
  return lines.filter((l) => /^ {6}\S/.test(l) && !l.startsWith('      curl '));
}

describe("the recipes' lines are the lines the CLI records", () => {
  it('records a key required and none needed', () => {
    expect(CONFIGS.map((c) => c.recipes.key).sort()).toEqual(['none_needed', 'required']);
  });

  it.each(CONFIGS.map((c) => [c.recipes.key, c] as const))('key %s', (_key, recorded) => {
    const shown = recipesShown(recorded.recipes);
    const width = Math.max(8, ...shown.flatMap((r) => r.lines.map((l) => l.label.length)));

    expect(shown.map((r) => `    ${r.client.padEnd(24)} ${r.hint}`)).toEqual(cliRecipeLines(recorded.lines));
    expect(shown.flatMap((r) => r.lines.map((l) => `      ${l.label.padEnd(width)} ${l.value}`))).toEqual(
      cliValueLines(recorded.lines),
    );
  });
});
