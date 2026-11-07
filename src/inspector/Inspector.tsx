/**
 * The inspector: one subject at a time, on the right of a place. Today the
 * one subject drawn here is a model on this machine (`local/`); the paired
 * machine's model, a Hugging Face repo and a loaded model's log join it in
 * I4, and stay wired in `places/library/LibraryPlace` until then.
 *
 * A model the library does not hold says so in one line, once the library
 * has been read. Each model gets a fresh inspector, so nothing picked for
 * one (a profile, a row being changed) carries to the next.
 *
 * @module inspector/Inspector
 */

import type { FC } from 'react';
import { useModelLibrary } from '../services/modelRegistry';
import type { GgufModel } from '../types';
import { LocalInspector, type LocalInspectorProps } from './local/LocalInspector';

export type InspectorProps = Omit<LocalInspectorProps, 'model'> & {
  /** This machine's model the address names; `null` until the library has it. */
  model: GgufModel | null;
};

export const Inspector: FC<InspectorProps> = ({ model, ...rest }) => {
  const { loading } = useModelLibrary();
  if (model) return <LocalInspector key={model.id} model={model} {...rest} />;
  return !loading ? <p className="m-0 p-md text-xs text-text-secondary">This model is not in the library.</p> : null;
};
