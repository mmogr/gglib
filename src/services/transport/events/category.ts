/**
 * Wire-tag → event-category routing for the shared `/api/events` stream.
 *
 * Every frame the daemon sends carries a `type` discriminant from Rust's
 * `AppEvent` (`#[serde(tag = "type", rename_all = "snake_case")]`). Consumers
 * subscribe by *category*, not by tag, so this is the one place that decides
 * which family a tag belongs to.
 *
 * It lives apart from the connection machinery because it is the piece worth
 * testing on its own: routing is pure, while everything around it is fetch,
 * backoff and lifecycle.
 */

import type { AppEventType } from '../types/events';

/**
 * Map an outer event `type` string to an `AppEventType` category.
 *
 * Returns `null` for a tag no category claims. That is not a soft failure —
 * the caller drops the frame without a word, so a family missing from this
 * function is silent rather than degraded. The model lifecycle events spent
 * their whole existence in exactly that state.
 */
export function getEventCategory(outerType: string): AppEventType | null {
  if (outerType === 'download') return 'download';
  if (outerType.startsWith('server_') || outerType === 'server_snapshot') return 'server';
  if (outerType === 'log' || outerType.startsWith('log_')) return 'log';
  if (outerType.startsWith('model_')) return 'model';
  if (outerType.startsWith('verification_') || outerType.startsWith('verification:')) {
    return 'verification';
  }
  if (outerType.startsWith('proxy_')) return 'proxy';
  return null;
}
