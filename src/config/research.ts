/**
 * Research loop configuration constants.
 * 
 * These values control the behavior of the deep research system, including
 * step limits, timeouts, concurrency, and productivity thresholds.
 * 
 * Tuning these values affects the trade-off between thoroughness and efficiency.
 * Lower values make research faster but potentially less comprehensive.
 * Higher values allow deeper investigation but increase time and token costs.
 */

/**
 * Default maximum research steps before hard stop.
 * 
 * Controls the default depth of a research session. Each "step" represents
 * one iteration of the research loop where the LLM can:
 * - Call tools to gather information
 * - Analyze gathered facts
 * - Decide next actions
 * 
 * This is the soft limit that can be overridden by user configuration.
 * See also: HARD_MAX_STEPS for the safety backstop.
 * 
 * @default 30
 */
export const DEFAULT_MAX_STEPS = 30;

/**
 * Soft landing threshold - force synthesis at this percentage of max steps.
 * 
 * When research reaches this percentage of the maximum allowed steps,
 * the system begins strongly encouraging synthesis and conclusion.
 * This prevents hitting hard limits and ensures graceful termination.
 * 
 * Example: At 0.8 (80%), if max_steps=30, synthesis is encouraged at step 24.
 * 
 * @default 0.8 (80%)
 */
export const SOFT_LANDING_THRESHOLD = 0.8;

/**
 * Maximum concurrent tool calls in a batch.
 * 
 * Controls how many tools can execute in parallel during a single step.
 * Higher values increase throughput but may:
 * - Overwhelm external services
 * - Make debugging harder
 * - Increase token usage if all results are processed
 * 
 * Lower values provide more controlled, sequential research.
 * 
 * @default 5
 */
export const MAX_PARALLEL_TOOLS = 5;

/**
 * Tool execution timeout in milliseconds.
 * 
 * Maximum time to wait for a single tool call to complete before
 * considering it failed. Prevents hanging on unresponsive services.
 * 
 * This applies per-tool, not per-batch. In parallel execution,
 * each tool gets its own timeout.
 * 
 * @default 30000 (30 seconds)
 */
export const TOOL_TIMEOUT_MS = 30000;

/**
 * Maximum retries for transient tool errors.
 * 
 * Number of times to retry a tool call that fails with a transient error
 * (network timeout, rate limit, temporary service unavailability).
 * 
 * Permanent errors (invalid parameters, not found) are not retried.
 * 
 * @default 2
 */
export const MAX_TOOL_RETRIES = 2;

/**
 * Maximum consecutive unproductive steps before blocking a question.
 * 
 * A step is "unproductive" if:
 * - No new facts were gathered
 * - No tool calls succeeded
 * - LLM only produced text without taking action
 * 
 * After this many consecutive unproductive steps, the system considers
 * the current question blocked and may:
 * - Switch to a different research question
 * - Force synthesis with available information
 * - Escalate to user intervention
 * 
 * This prevents spinning on questions that cannot be answered with
 * available tools or information.
 * 
 * @default 5
 */
export const CONSECUTIVE_UNPRODUCTIVE_LIMIT = 5;

/**
 * Hard maximum steps regardless of productivity.
 * 
 * Safety net to prevent infinite loops even if the agent keeps finding
 * new information. Research will terminate after this many steps even
 * if every step was productive.
 * 
 * This is higher than DEFAULT_MAX_STEPS to allow continued research
 * when productivity is high, but prevents runaway sessions.
 * 
 * @default 50
 */
export const HARD_MAX_STEPS = 50;

/**
 * Maximum consecutive LLM responses without tool calls before penalizing.
 * 
 * When the LLM outputs text-only reasoning without calling any tools,
 * we track consecutive occurrences. After this many consecutive text-only
 * responses, the step is treated as unproductive.
 * 
 * This prevents "analysis paralysis" where the LLM thinks about the
 * problem endlessly without taking action to gather information.
 * 
 * @default 3
 */
export const MAX_TEXT_ONLY_STEPS = 3;

/**
 * Absolute maximum loop iterations (safety backstop).
 * 
 * This is the ultimate safety limit that fires regardless of any other
 * logic, productivity, or research quality. Prevents infinite loops even
 * if all other safeguards fail.
 * 
 * Should be significantly higher than HARD_MAX_STEPS to allow for:
 * - Internal loop overhead
 * - Error handling iterations
 * - Multiple research questions in a session
 * 
 * If this limit is hit, it indicates a serious bug in loop termination logic.
 * 
 * @default 100
 */
export const MAX_LOOP_ITERATIONS = 100;

/**
 * Maximum steps to spend on a single question before escalating.
 * 
 * After this many productive steps focused on the same question, the system
 * will strongly encourage answering or auto-trigger force-answer.
 * 
 * This prevents over-researching simple questions that gather redundant
 * facts, and encourages moving on to the next question or concluding research.
 * 
 * Set lower for faster, broader research across multiple questions.
 * Set higher for deeper investigation of complex individual questions.
 * 
 * @default 3
 */
export const STEPS_PER_QUESTION_LIMIT = 3;

