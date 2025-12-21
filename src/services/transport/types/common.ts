/**
 * Common utility types shared across transport sub-interfaces.
 */

/**
 * Function to unsubscribe from an event listener.
 */
export type Unsubscribe = () => void;

/**
 * Generic event handler function.
 */
export type EventHandler<T> = (payload: T) => void;

/**
 * Pagination parameters for list operations.
 */
export interface PaginationParams {
  page: number;
  limit: number;
}

/**
 * Paginated response wrapper.
 */
export interface PaginatedResponse<T> {
  items: T[];
  hasMore: boolean;
  page: number;
  totalCount?: number;
}
