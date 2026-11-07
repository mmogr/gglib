/**
 * When a change to a setting takes effect, for the settings that do not take
 * effect at once: the endpoint reads its own when it turns on, gglib its
 * address when it starts, and a model server its port when it is loaded.
 *
 * @module places/settings/notes
 */

export const ON_ENDPOINT_START = 'Takes effect when the endpoint next turns on.';
export const ON_GGLIB_START = 'Takes effect when gglib next starts.';
export const ON_NEXT_LOAD = 'Takes effect for the next model loaded.';
