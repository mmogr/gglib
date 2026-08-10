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


