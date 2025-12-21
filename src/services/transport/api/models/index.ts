/**
 * Models API module (combined local + HuggingFace).
 */

import * as local from './local';
import * as hf from './hf';

export function createModelsApi() {
  return {
    ...local,
    ...hf,
  };
}
