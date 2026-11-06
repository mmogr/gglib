/**
 * The library's rows, grouped, filtered and sorted: pure, so the rules are
 * pinned without a screen.
 *
 * A model is in one group: Loaded when a server for it is in the server
 * registry, On this Mac otherwise. A search is words, every one of which a
 * row must match (AND, as the tag filter was): a word matches the name, the
 * quant, a tag, the architecture, "vision" / "sees" / "images" on a model
 * that reads images, "draws" or the family on one that draws, and a size
 * like "8b" on a model of about that many parameters. The paired machine's
 * rows match by what its list says of them: the id, the quant, the
 * description, and "vision" on one that reads images.
 *
 * @module places/library/groups
 */

import type { GgufModel } from '../../types';
import type { ModelInfo } from '../../types/generated/ModelInfo';
import type { DownloadRow } from '../../services/transport/types/downloads';
import { canSee } from '../../utils/canSee';
import { imageFamilyLabel } from '../../utils/imageFamily';

export type SortKey = 'name' | 'added' | 'size' | 'speed';

export interface LibrarySort {
  key: SortKey;
  ascending: boolean;
}

export const DEFAULT_SORT: LibrarySort = { key: 'name', ascending: true };

export interface LibraryGroups {
  loaded: GgufModel[];
  here: GgufModel[];
}

/** The words of a search, lower case, empty words dropped. */
function wordsOf(query: string): string[] {
  return query.toLowerCase().split(/\s+/).filter(Boolean);
}

/** A size word, "8b" or "0.6b": the number of billions it names, else `null`. */
function billionsOf(word: string): number | null {
  const m = /^(\d+(?:\.\d+)?)b$/.exec(word);
  return m ? Number(m[1]) : null;
}

/** Whether `params` billions is what a size word names: to the unit, or to the tenth when it has one. */
function sizeMatches(params: number, word: string): boolean {
  const named = billionsOf(word);
  if (named === null) return false;
  return word.includes('.') ? Math.round(params * 10) === Math.round(named * 10) : Math.round(params) === named;
}

function haystack(model: GgufModel): string {
  const parts = [model.name, model.quantization ?? '', model.architecture ?? '', ...model.tags];
  if (canSee(model)) parts.push('vision sees images');
  if (model.imageFamily) parts.push('draws', imageFamilyLabel(model.imageFamily));
  return parts.join(' ').toLowerCase();
}

/** Whether this machine's `model` matches every word of `query`. */
export function matchesModel(model: GgufModel, query: string): boolean {
  const text = haystack(model);
  return wordsOf(query).every((word) => text.includes(word) || sizeMatches(model.paramCountB, word));
}

/** Whether the paired machine's `model` matches every word of `query`. */
export function matchesFar(model: ModelInfo, query: string): boolean {
  const parts = [model.id, model.quantization ?? '', model.description ?? ''];
  if (canSee(model)) parts.push('vision sees images');
  const text = parts.join(' ').toLowerCase();
  return wordsOf(query).every((word) => text.includes(word));
}

/** Whether a download matches every word of `query`, by its name. */
export function matchesDownload(row: DownloadRow, query: string): boolean {
  const text = row.text.title.toLowerCase();
  return wordsOf(query).every((word) => text.includes(word));
}

/** A model's measured speed, tokens a second, or `null` when it has none. */
function speedOf(model: GgufModel): number | null {
  return model.benchmarkSummary?.latest_tg_tps ?? model.benchmarkSummary?.best_tg_tps ?? null;
}

function compare(a: GgufModel, b: GgufModel, key: SortKey): number {
  switch (key) {
    case 'name':
      return a.name.localeCompare(b.name, undefined, { sensitivity: 'base', numeric: true });
    case 'added':
      // `%Y-%m-%d %H:%M:%S`: in order as text.
      return a.addedAt < b.addedAt ? -1 : a.addedAt > b.addedAt ? 1 : 0;
    case 'size':
      return a.sizeBytes - b.sizeBytes;
    case 'speed':
      return (speedOf(a) ?? 0) - (speedOf(b) ?? 0);
  }
}

/**
 * `models` in `sort`'s order. By speed, a model never measured comes last
 * either way, since it has no figure to stand in its place.
 */
export function sortModels(models: readonly GgufModel[], sort: LibrarySort): GgufModel[] {
  const sign = sort.ascending ? 1 : -1;
  return [...models].sort((a, b) => {
    if (sort.key === 'speed') {
      const unmeasured = Number(speedOf(a) === null) - Number(speedOf(b) === null);
      if (unmeasured !== 0) return unmeasured;
    }
    return sign * compare(a, b, sort.key);
  });
}

/**
 * This machine's models in their groups, each matching `query`, each group
 * in `sort`'s order: Loaded holds the models `loadedIds` names, On this Mac
 * the rest, so a model is in one.
 */
export function libraryGroups(
  models: readonly GgufModel[],
  loadedIds: ReadonlySet<number>,
  query: string,
  sort: LibrarySort,
): LibraryGroups {
  const shown = sortModels(
    models.filter((m) => matchesModel(m, query)),
    sort,
  );
  return {
    loaded: shown.filter((m) => loadedIds.has(m.id)),
    here: shown.filter((m) => !loadedIds.has(m.id)),
  };
}
