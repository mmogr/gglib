import type { ComponentRole, ImageFamily } from '../types';

/** Each family by the name `ImageFamily::label` gives it in Rust, so the page and the terminal agree. */
const FAMILY_LABELS: Record<ImageFamily, string> = {
  flux1: 'Flux.1',
  sdxl: 'SDXL',
  'qwen-image-2.1': 'Qwen-Image 2.1',
};

/** Each component role by the name `ComponentRole::label()` gives it in Rust, so the page and the terminal agree; the wire name is stable-diffusion.cpp's flag. */
const ROLE_LABELS: Record<ComponentRole, string> = {
  vae: 'VAE',
  clip_l: 'CLIP-L',
  t5xxl: 'T5-XXL',
  llm: 'LLM',
};

/** The family's name, e.g. "Flux.1" for `flux1`. */
export function imageFamilyLabel(family: ImageFamily): string {
  return FAMILY_LABELS[family];
}

/** The role's name, e.g. "VAE" for `vae`. */
export function componentRoleLabel(role: ComponentRole): string {
  return ROLE_LABELS[role];
}
