import { createContext } from 'react';
import type { Branching } from '../../../hooks/useGglibRuntime/branchChanges';

/**
 * What the open chat offers of its branches, for the turns and rows that
 * show them: the branch points along it, whether it ends in a question
 * nothing answers, and the changes and openings made from them. The same
 * object until what the daemon says of the chat changes. None on a far chat.
 */
export const BranchingContext = createContext<Branching | null>(null);
