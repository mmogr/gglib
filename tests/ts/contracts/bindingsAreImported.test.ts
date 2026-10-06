/**
 * Every generated binding is imported, or is on the list below.
 *
 * A binding nothing imports is a Rust wire type the frontend does not check
 * itself against. Most often it is the body of a route no client here calls,
 * or a shape a client builds or reads by hand, where a renamed field is a key
 * the daemon drops in silence. Neither fails to compile, and knip does not
 * look inside `src/types/generated`.
 *
 * Imported means an `import` or `export … from` in `src` or `tests/ts` that
 * resolves to the binding's file, or one that reaches it through the
 * bindings' own imports of each other. A name in a comment, or a hand-written
 * type of the same name, does not count: both are how an unused binding looks
 * used.
 *
 * What this cannot see is a hand-written copy of a shape whose binding another
 * binding imports: the copy's binding is still reached through that one.
 *
 * `NOT_IMPORTED` records the bindings nothing imports. The check is that no
 * binding joins them unrecorded. It does not fail when an entry is imported
 * or its binding deleted, so an entry can outlive its reason: delete the line
 * then.
 */

import { readdirSync, readFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { describe, it, expect } from 'vitest';

const REPO_ROOT = resolve(import.meta.dirname, '../../..');
const GENERATED = join(REPO_ROOT, 'src/types/generated');

/** Bindings nothing imports. To take one off: import it, or drop its `TS` derive in Rust. */
const NOT_IMPORTED = [
  'AgentChatRequest',
  'AssistantContent',
  'CallToolRequest',
  'ChatChoice',
  'ChatCompletionResponse',
  'ChatMessage',
  'ChatProxyRequest',
  'ChatUsage',
  'DisableFastDownloadsResponse',
  'ErrorBody',
  'ExplainQueryParams',
  'GetRunResponse',
  'GgufFileRole',
  'ListRunsQuery',
  'McpToolCallRequest',
  'McpToolCallResponse',
  'ModelAgenticHistoryQuery',
  'ModelBenchmarkQuery',
  'ModelBenchmarkResponse',
  'ModelListQueryParams',
  'ModelTuneHistoryQuery',
  'ModelTuneHistoryResponse',
  'ModelsResponse',
  'PublishedStateDto',
  'QueueDownloadRequest',
  'ReorderFullRequest',
  'ReorderRequest',
  'RepairResponse',
  'SaveMessageRequest',
  'StartServerBody',
  'UpdateMessageRequest',
  'VerifyResponse',
];

/** Every `.ts`/`.tsx` file under `dir`, the generated bindings left out. */
function sources(dir: string): string[] {
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) return path === GENERATED ? [] : sources(path);
    return /\.tsx?$/.test(entry.name) ? [path] : [];
  });
}

/**
 * The module specifiers a file imports or re-exports from, comment lines left
 * out. A statement spread over several lines ends in `} from '…'`, which is
 * the part this reads; `import('…')` is the inline form of a type import.
 */
function specifiers(source: string): string[] {
  const code = source
    .split('\n')
    .filter((line) => !/^\s*(\/\/|\/\*|\*)/.test(line))
    .join('\n');
  return [...code.matchAll(/(?:\bfrom\s*|\bimport\s*\(\s*)(['"])([^'"\n]+)\1/g)].map((match) => match[2]);
}

/** The binding a relative specifier in `file` resolves to, if it is one. */
function bindingOf(file: string, specifier: string): string | null {
  if (!specifier.startsWith('.')) return null;
  const target = resolve(dirname(file), specifier);
  return dirname(target) === GENERATED ? target.slice(GENERATED.length + 1) : null;
}

/** The bindings `file` imports. */
function bindingsImportedBy(file: string): string[] {
  return specifiers(readFileSync(file, 'utf8'))
    .map((specifier) => bindingOf(file, specifier))
    .filter((name): name is string => name !== null);
}

/** `roots`, and everything they reach by following `importsOf`. */
function reach(roots: string[], importsOf: (name: string) => string[]): Set<string> {
  const reached = new Set<string>();
  const pending = [...roots];
  while (pending.length > 0) {
    const name = pending.pop() as string;
    if (reached.has(name)) continue;
    reached.add(name);
    pending.push(...importsOf(name));
  }
  return reached;
}

describe('generated bindings', () => {
  it('each is imported by the app or its tests, or is recorded as not', () => {
    const bindings = readdirSync(GENERATED)
      .filter((name) => name.endsWith('.ts'))
      .map((name) => name.slice(0, -'.ts'.length));
    // An empty directory listing would otherwise pass as a tree with nothing unused.
    expect(bindings).toContain('GuiModel');

    const imported = [join(REPO_ROOT, 'src'), join(REPO_ROOT, 'tests/ts')]
      .flatMap(sources)
      .flatMap(bindingsImportedBy);
    const reached = reach(imported, (name) => bindingsImportedBy(join(GENERATED, `${name}.ts`)));

    const unrecorded = bindings.filter((name) => !reached.has(name) && !NOT_IMPORTED.includes(name));
    expect(
      unrecorded.sort(),
      'nothing imports these bindings: type the client that sends or reads the shape with it, or drop its `TS` derive',
    ).toEqual([]);
  });

  it('an import counts in a statement and not in a comment', () => {
    const source = [
      "import type { A } from './generated/A';",
      'export type {',
      '  B,',
      "} from \"../generated/B\";",
      "type C = import('./generated/C').C;",
      "// import type { D } from './generated/D';",
      '/**',
      " * Convert to the shape `import type { E } from './generated/E'` declares.",
      ' */',
      "/* import type { F } from './generated/F'; */",
    ].join('\n');
    expect(specifiers(source)).toEqual(['./generated/A', '../generated/B', './generated/C']);
  });

  it('a specifier names a binding only when it resolves into the generated directory', () => {
    const file = join(REPO_ROOT, 'src/types/benchmark.ts');
    expect(bindingOf(file, './generated/BenchmarkRun')).toBe('BenchmarkRun');
    expect(bindingOf(join(GENERATED, 'BenchmarkRun.ts'), './BenchmarkRunStatus')).toBe('BenchmarkRunStatus');
    // A hand-written module of the same name, and a package, are not bindings.
    expect(bindingOf(file, './BenchmarkRun')).toBeNull();
    expect(bindingOf(file, 'generated/BenchmarkRun')).toBeNull();
  });

  it('a binding is reached through the bindings that import it, and not otherwise', () => {
    const graph: Record<string, string[]> = { A: ['B'], B: ['C', 'A'], C: [], D: ['A'] };
    expect([...reach(['A'], (name) => graph[name])].sort()).toEqual(['A', 'B', 'C']);
  });
});
