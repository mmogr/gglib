/**
 * Fact Extraction Module for Deep Research Mode
 *
 * The "Digestive System" - converts raw search observations into structured facts.
 *
 * Implements three data-integrity constraints:
 * 1. Strict Source Lineage - validates URLs exist in observations
 * 2. Semantic Deduplication - prevents duplicate facts via fuzzy matching
 * 3. Reference-Aware Pruning - protects facts used by answered questions
 *
 * Uses the cheap/fast extraction model to minimize cost.
 *
 * @module useDeepResearch/factExtractor
 */

import type {
  ResearchState,
  GatheredFact,
  PendingObservation,
  ModelEndpoint,
} from './types';
import {
  createFact,
  addFacts,
} from './types';
import type { TurnMessage } from './buildTurnMessages';

// =============================================================================
// Configuration
// =============================================================================

/** Similarity threshold for deduplication (0-1, higher = stricter) */
export const DEDUP_SIMILARITY_THRESHOLD = 0.8;

/** Maximum facts to retain after pruning */
export const MAX_FACTS_RETAINED = 50;

/** Maximum facts to extract per observation */
export const MAX_FACTS_PER_OBSERVATION = 5;

// =============================================================================
// Types
// =============================================================================

/**
 * Raw fact from LLM extraction (before validation).
 */
interface RawExtractedFact {
  claim: string;
  sourceUrl: string;
  sourceTitle: string;
  confidence: 'high' | 'medium' | 'low';
  relevantToQuestion?: string;
}

/**
 * LLM caller for extraction (simplified signature).
 */
export type ExtractionLLMCaller = (
  messages: TurnMessage[],
  endpoint: ModelEndpoint,
  abortSignal?: AbortSignal
) => Promise<string>;

/**
 * Options for fact extraction.
 */
export interface ExtractFactsOptions {
  /** Current research state */
  state: ResearchState;
  /** Extraction model endpoint (cheap/fast) */
  extractionEndpoint: ModelEndpoint;
  /** LLM caller function */
  callLLM: ExtractionLLMCaller;
  /** Abort signal for cancellation */
  abortSignal?: AbortSignal;
}

/**
 * Result from fact extraction.
 */
export interface ExtractFactsResult {
  /** New facts extracted (after validation and dedup) */
  newFacts: GatheredFact[];
  /** Number of facts discarded due to invalid URL */
  discardedInvalidUrl: number;
  /** Number of facts discarded as duplicates */
  discardedDuplicates: number;
  /** Updated state with facts added */
  updatedState: ResearchState;
}

// =============================================================================
// Extraction Prompt
// =============================================================================

/**
 * Build the extraction prompt.
 * Explicitly instructs the model to only use URLs from the provided sources.
 */
function buildExtractionPrompt(
  observations: PendingObservation[],
  currentQuestionId?: string
): TurnMessage[] {
  // Build source inventory for the model
  const sourceInventory: string[] = [];
  const observationTexts: string[] = [];

  for (let i = 0; i < observations.length; i++) {
    const obs = observations[i];
    
    // Extract URLs from raw result
    const urls = extractUrlsFromResult(obs.rawResult);
    for (const url of urls) {
      sourceInventory.push(`- ${url}`);
    }

    // Format observation for extraction
    observationTexts.push(`### Observation ${i + 1}: ${obs.toolName}`);
    observationTexts.push('```json');
    observationTexts.push(
      typeof obs.rawResult === 'string'
        ? obs.rawResult
        : JSON.stringify(obs.rawResult, null, 2)
    );
    observationTexts.push('```');
    observationTexts.push('');
  }

  const systemPrompt = `You are a fact extraction assistant. Your job is to extract factual claims from search results.

## CRITICAL RULES

1. **SOURCE LINEAGE**: You may ONLY cite URLs that appear in the source data below. Do NOT invent or hallucinate URLs.
2. **ATOMIC CLAIMS**: Each fact should be a single, verifiable claim (not opinions or speculation).
3. **BREVITY**: Keep claims under 200 characters.
4. **ATTRIBUTION**: Every fact MUST have a sourceUrl from the VALID SOURCES list.

## VALID SOURCES (you may ONLY use these URLs)

${sourceInventory.length > 0 ? sourceInventory.join('\n') : '(No URLs found in observations)'}

## OUTPUT FORMAT

Respond with ONLY valid JSON:
{
  "facts": [
    {
      "claim": "Specific factual claim under 200 chars",
      "sourceUrl": "https://... (MUST be from VALID SOURCES above)",
      "sourceTitle": "Source Name",
      "confidence": "high|medium|low"
    }
  ]
}

Extract up to ${MAX_FACTS_PER_OBSERVATION * observations.length} most important facts.
If no valid facts can be extracted, return: {"facts": []}`;

  const userPrompt = `Extract facts from these search results:

${observationTexts.join('\n')}

${currentQuestionId ? `Focus on facts relevant to answering the current research question.` : ''}

Remember: ONLY use URLs from the VALID SOURCES list. Any other URL will be rejected.`;

  return [
    { role: 'system', content: systemPrompt },
    { role: 'user', content: userPrompt },
  ];
}

/**
 * Extract URLs from a raw observation result.
 */
function extractUrlsFromResult(result: unknown): string[] {
  const urls: string[] = [];
  
  const extract = (obj: unknown): void => {
    if (typeof obj === 'string') {
      // Match URLs in strings
      const urlMatches = obj.match(/https?:\/\/[^\s"'<>]+/g);
      if (urlMatches) {
        urls.push(...urlMatches);
      }
    } else if (Array.isArray(obj)) {
      obj.forEach(extract);
    } else if (obj && typeof obj === 'object') {
      // Check common URL field names
      const record = obj as Record<string, unknown>;
      for (const key of ['url', 'link', 'href', 'source', 'sourceUrl']) {
        if (typeof record[key] === 'string') {
          urls.push(record[key] as string);
        }
      }
      // Recurse into values
      Object.values(record).forEach(extract);
    }
  };
  
  extract(result);
  
  // Deduplicate and clean
  return [...new Set(urls.map(normalizeUrl))];
}

/**
 * Normalize URL for comparison (remove trailing slash, lowercase host).
 */
function normalizeUrl(url: string): string {
  try {
    const parsed = new URL(url);
    parsed.hash = '';
    return parsed.href.replace(/\/$/, '');
  } catch {
    return url.toLowerCase().replace(/\/$/, '');
  }
}

// =============================================================================
// Source Lineage Validation
// =============================================================================

/**
 * Validate that extracted facts only reference URLs from observations.
 * Discards facts with hallucinated URLs.
 */
function validateSourceLineage(
  rawFacts: RawExtractedFact[],
  observations: PendingObservation[]
): { valid: RawExtractedFact[]; invalidCount: number } {
  // Build set of all valid URLs from observations
  const validUrls = new Set<string>();
  
  for (const obs of observations) {
    const urls = extractUrlsFromResult(obs.rawResult);
    for (const url of urls) {
      validUrls.add(normalizeUrl(url));
    }
  }
  
  const valid: RawExtractedFact[] = [];
  let invalidCount = 0;
  
  for (const fact of rawFacts) {
    const normalizedFactUrl = normalizeUrl(fact.sourceUrl);
    
    // Check exact match
    if (validUrls.has(normalizedFactUrl)) {
      valid.push(fact);
      continue;
    }
    
    // Check partial match (domain + path prefix)
    let foundMatch = false;
    for (const validUrl of validUrls) {
      if (
        normalizedFactUrl.startsWith(validUrl) ||
        validUrl.startsWith(normalizedFactUrl)
      ) {
        // Use the valid URL instead of the potentially hallucinated one
        valid.push({ ...fact, sourceUrl: validUrl });
        foundMatch = true;
        break;
      }
    }
    
    if (!foundMatch) {
      console.warn(
        `[factExtractor] Discarding fact with invalid URL: ${fact.sourceUrl}`
      );
      invalidCount++;
    }
  }
  
  return { valid, invalidCount };
}

// =============================================================================
// Semantic Deduplication
// =============================================================================

/**
 * Normalize text for comparison (lowercase, remove punctuation, collapse whitespace).
 */
function normalizeText(text: string): string {
  return text
    .toLowerCase()
    .replace(/[^\w\s]/g, ' ')
    .replace(/\s+/g, ' ')
    .trim();
}

/**
 * Tokenize text into words/ngrams for comparison.
 */
function tokenize(text: string): Set<string> {
  const normalized = normalizeText(text);
  const words = normalized.split(' ').filter((w) => w.length > 2);
  
  // Include bigrams for better matching
  const tokens = new Set(words);
  for (let i = 0; i < words.length - 1; i++) {
    tokens.add(`${words[i]} ${words[i + 1]}`);
  }
  
  return tokens;
}

/**
 * Calculate Jaccard similarity between two token sets.
 */
function jaccardSimilarity(setA: Set<string>, setB: Set<string>): number {
  if (setA.size === 0 && setB.size === 0) return 1;
  if (setA.size === 0 || setB.size === 0) return 0;
  
  let intersection = 0;
  for (const token of setA) {
    if (setB.has(token)) intersection++;
  }
  
  const union = setA.size + setB.size - intersection;
  return intersection / union;
}

/**
 * Check if a fact is a duplicate of any existing fact.
 */
function isDuplicate(
  newClaim: string,
  existingFacts: GatheredFact[],
  threshold: number = DEDUP_SIMILARITY_THRESHOLD
): { isDup: boolean; existingFact?: GatheredFact } {
  const newTokens = tokenize(newClaim);
  
  for (const existing of existingFacts) {
    const existingTokens = tokenize(existing.claim);
    const similarity = jaccardSimilarity(newTokens, existingTokens);
    
    if (similarity >= threshold) {
      return { isDup: true, existingFact: existing };
    }
  }
  
  return { isDup: false };
}

/**
 * Deduplicate facts, keeping higher confidence versions.
 */
function deduplicateFacts(
  newFacts: RawExtractedFact[],
  existingFacts: GatheredFact[],
  threshold: number = DEDUP_SIMILARITY_THRESHOLD
): { unique: RawExtractedFact[]; duplicateCount: number } {
  const unique: RawExtractedFact[] = [];
  let duplicateCount = 0;
  
  // Track facts we're adding in this batch too
  const addedClaims: string[] = [];
  
  const confidenceRank = { high: 3, medium: 2, low: 1 };
  
  for (const fact of newFacts) {
    // Check against existing facts
    const { isDup, existingFact } = isDuplicate(
      fact.claim,
      existingFacts,
      threshold
    );
    
    if (isDup && existingFact) {
      // Keep existing if same or higher confidence
      const newRank = confidenceRank[fact.confidence];
      const existingRank = confidenceRank[existingFact.confidence];
      
      if (newRank > existingRank) {
        // New fact is better - we'll add it, but note we can't remove the old one here
        // (that would require more complex state management)
        console.log(
          `[factExtractor] Duplicate found but new has higher confidence: "${fact.claim.slice(0, 50)}..."`
        );
        unique.push(fact);
      } else {
        console.log(
          `[factExtractor] Discarding duplicate: "${fact.claim.slice(0, 50)}..."`
        );
        duplicateCount++;
      }
      continue;
    }
    
    // Check against facts we're adding in this batch
    const batchDup = addedClaims.some((claim) => {
      const tokens1 = tokenize(fact.claim);
      const tokens2 = tokenize(claim);
      return jaccardSimilarity(tokens1, tokens2) >= threshold;
    });
    
    if (batchDup) {
      console.log(
        `[factExtractor] Discarding batch duplicate: "${fact.claim.slice(0, 50)}..."`
      );
      duplicateCount++;
      continue;
    }
    
    unique.push(fact);
    addedClaims.push(fact.claim);
  }
  
  return { unique, duplicateCount };
}

// =============================================================================
// Reference-Aware Pruning
// =============================================================================

/**
 * Get IDs of facts that are referenced by answered questions.
 * These facts are "protected" and must not be pruned.
 */
function getProtectedFactIds(state: ResearchState): Set<string> {
  const protected_ = new Set<string>();
  
  for (const question of state.researchPlan) {
    // Protect facts used by answered or in-progress questions
    if (question.status === 'answered' || question.status === 'in-progress') {
      for (const factId of question.supportingFactIds) {
        protected_.add(factId);
      }
    }
  }
  
  // Also protect facts referenced in citations (if any partial report exists)
  for (const citation of state.citations) {
    protected_.add(citation.factId);
  }
  
  return protected_;
}

/**
 * Prune facts to stay within budget, protecting referenced facts.
 *
 * Pruning strategy:
 * 1. Never remove protected facts (referenced by questions/citations)
 * 2. Score remaining facts by: recency + confidence + relevance
 * 3. Remove lowest-scored facts until under budget
 */
export function pruneFacts(
  state: ResearchState,
  maxFacts: number = MAX_FACTS_RETAINED
): ResearchState {
  if (state.gatheredFacts.length <= maxFacts) {
    return state;
  }
  
  const protectedIds = getProtectedFactIds(state);
  
  // Separate protected and prunable facts
  const protectedFacts: GatheredFact[] = [];
  const prunableFacts: GatheredFact[] = [];
  
  for (const fact of state.gatheredFacts) {
    if (protectedIds.has(fact.id)) {
      protectedFacts.push(fact);
    } else {
      prunableFacts.push(fact);
    }
  }
  
  // If protected facts alone exceed budget, we can't prune safely
  if (protectedFacts.length >= maxFacts) {
    console.warn(
      `[factExtractor] Protected facts (${protectedFacts.length}) exceed budget (${maxFacts}). Cannot prune safely.`
    );
    return state;
  }
  
  // Calculate how many prunable facts we can keep
  const prunableSlots = maxFacts - protectedFacts.length;
  
  // Score prunable facts
  const confidenceScore = { high: 3, medium: 2, low: 1 };
  
  const scored = prunableFacts.map((fact) => ({
    fact,
    score:
      // Recency (newer = higher, max 10 points)
      Math.min(10, state.currentStep - fact.gatheredAtStep + 5) +
      // Confidence (max 3 points)
      confidenceScore[fact.confidence] +
      // Relevance (referenced by any question, max 5 points)
      Math.min(5, fact.relevantQuestionIds.length * 2),
  }));
  
  // Sort by score descending
  scored.sort((a, b) => b.score - a.score);
  
  // Keep top scored facts
  const keptPrunable = scored.slice(0, prunableSlots).map((s) => s.fact);
  const prunedCount = prunableFacts.length - keptPrunable.length;
  
  if (prunedCount > 0) {
    console.log(
      `[factExtractor] Pruned ${prunedCount} facts. Protected: ${protectedFacts.length}, Kept: ${keptPrunable.length}`
    );
  }
  
  return {
    ...state,
    gatheredFacts: [...protectedFacts, ...keptPrunable],
  };
}

// =============================================================================
// Response Parsing
// =============================================================================

/**
 * Parse extraction response from LLM.
 */
function parseExtractionResponse(content: string): RawExtractedFact[] {
  const trimmed = content.trim();
  
  // Try to extract JSON from markdown code blocks
  const jsonMatch = trimmed.match(/```(?:json)?\s*([\s\S]*?)```/);
  const jsonStr = jsonMatch ? jsonMatch[1].trim() : trimmed;
  
  // Find JSON object in content
  const jsonStart = jsonStr.indexOf('{');
  const jsonEnd = jsonStr.lastIndexOf('}');
  
  if (jsonStart === -1 || jsonEnd === -1) {
    console.warn('[factExtractor] No JSON found in extraction response');
    return [];
  }
  
  try {
    const parsed = JSON.parse(jsonStr.slice(jsonStart, jsonEnd + 1));
    
    if (!parsed || !Array.isArray(parsed.facts)) {
      console.warn('[factExtractor] Invalid extraction response structure');
      return [];
    }
    
    // Validate each fact
    const validFacts: RawExtractedFact[] = [];
    
    for (const fact of parsed.facts) {
      if (
        typeof fact.claim === 'string' &&
        typeof fact.sourceUrl === 'string' &&
        fact.claim.length > 0 &&
        fact.sourceUrl.length > 0
      ) {
        validFacts.push({
          claim: fact.claim.slice(0, 500), // Enforce max length
          sourceUrl: fact.sourceUrl,
          sourceTitle: fact.sourceTitle || 'Unknown Source',
          confidence: ['high', 'medium', 'low'].includes(fact.confidence)
            ? fact.confidence
            : 'medium',
          relevantToQuestion: fact.relevantToQuestion,
        });
      }
    }
    
    return validFacts;
  } catch (error) {
    console.warn('[factExtractor] Failed to parse extraction response:', error);
    return [];
  }
}

// =============================================================================
// Main Extraction Function
// =============================================================================

/**
 * Extract facts from pending observations.
 *
 * Pipeline:
 * 1. Build extraction prompt with source inventory
 * 2. Call extraction model (cheap/fast)
 * 3. Parse structured response
 * 4. Validate source lineage (discard hallucinated URLs)
 * 5. Deduplicate against existing facts
 * 6. Add to state with pruning
 */
export async function extractFacts(
  options: ExtractFactsOptions
): Promise<ExtractFactsResult> {
  const { state, extractionEndpoint, callLLM, abortSignal } = options;
  
  // Nothing to extract
  if (state.pendingObservations.length === 0) {
    return {
      newFacts: [],
      discardedInvalidUrl: 0,
      discardedDuplicates: 0,
      updatedState: state,
    };
  }
  
  console.log(
    `[factExtractor] Extracting facts from ${state.pendingObservations.length} observation(s)`
  );
  
  // Find current question for relevance attribution
  const currentQuestion = state.researchPlan.find(
    (q) => q.status === 'in-progress'
  );
  
  // Build extraction prompt
  const messages = buildExtractionPrompt(
    state.pendingObservations,
    currentQuestion?.id
  );
  
  // Call extraction model
  let responseContent: string;
  try {
    responseContent = await callLLM(messages, extractionEndpoint, abortSignal);
  } catch (error) {
    console.error('[factExtractor] LLM call failed:', error);
    // Return state unchanged on extraction failure
    return {
      newFacts: [],
      discardedInvalidUrl: 0,
      discardedDuplicates: 0,
      updatedState: state,
    };
  }
  
  // Parse response
  const rawFacts = parseExtractionResponse(responseContent);
  console.log(`[factExtractor] Parsed ${rawFacts.length} raw facts`);
  
  if (rawFacts.length === 0) {
    return {
      newFacts: [],
      discardedInvalidUrl: 0,
      discardedDuplicates: 0,
      updatedState: state,
    };
  }
  
  // Step 1: Validate source lineage
  const { valid: lineageValid, invalidCount } = validateSourceLineage(
    rawFacts,
    state.pendingObservations
  );
  console.log(
    `[factExtractor] Source lineage: ${lineageValid.length} valid, ${invalidCount} discarded`
  );
  
  // Step 2: Deduplicate
  const { unique, duplicateCount } = deduplicateFacts(
    lineageValid,
    state.gatheredFacts
  );
  console.log(
    `[factExtractor] Deduplication: ${unique.length} unique, ${duplicateCount} duplicates`
  );
  
  // Step 3: Convert to GatheredFact objects
  const newFacts: GatheredFact[] = unique.map((raw) =>
    createFact(
      raw.claim,
      raw.sourceUrl,
      raw.sourceTitle,
      raw.confidence,
      state.currentStep,
      currentQuestion ? [currentQuestion.id] : []
    )
  );
  
  // Step 4: Add to state with automatic pruning
  let updatedState = addFacts(state, newFacts, MAX_FACTS_RETAINED);
  
  // Step 5: Apply reference-aware pruning
  updatedState = pruneFacts(updatedState, MAX_FACTS_RETAINED);
  
  console.log(
    `[factExtractor] Final: ${newFacts.length} new facts added, total: ${updatedState.gatheredFacts.length}`
  );
  
  return {
    newFacts,
    discardedInvalidUrl: invalidCount,
    discardedDuplicates: duplicateCount,
    updatedState,
  };
}

// =============================================================================
// Utility Exports
// =============================================================================

/**
 * Calculate similarity between two text strings.
 * Exposed for testing and debugging.
 */
export function calculateSimilarity(textA: string, textB: string): number {
  return jaccardSimilarity(tokenize(textA), tokenize(textB));
}

/**
 * Check if a claim would be considered a duplicate.
 * Exposed for UI preview/validation.
 */
export function wouldBeDuplicate(
  claim: string,
  existingFacts: GatheredFact[],
  threshold: number = DEDUP_SIMILARITY_THRESHOLD
): boolean {
  return isDuplicate(claim, existingFacts, threshold).isDup;
}
