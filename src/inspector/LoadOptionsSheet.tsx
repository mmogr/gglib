/**
 * Load with options: a sheet in the inspector's place, growing from the
 * control that opened it (⋯ Load with options… and Load and pin, a launch
 * line token's Change for this model…, `?serve=1` and the native menu's
 * Serve). Each field shows what the launch would use without it and whose
 * choice that is, from the model's explanation (the plan's arguments, the
 * fit, the sampling sources; IN-4, IN-5): Context, Jinja, Speculative (a
 * model with MTP heads only), the sampling form, ▸ Port, Pin the endpoint
 * to this model, and This load only. An image model has its Parts line in
 * place of the chat fields.
 *
 * Load sends only what was typed (`loadOptions`); with This load only
 * unchecked, the typed values with a per-model home are saved first, each
 * over the stored rest. A failed save or load stays here in the daemon's
 * words; a load opens no chat. With llama.cpp missing, Load is Install
 * llama.cpp (the library banner's install, as the inspector's Load is),
 * then this load once the install completes.
 *
 * @module inspector/LoadOptionsSheet
 */

import { useCallback, useEffect, useRef, useState, type FC, type ReactNode } from 'react';
import { SamplingForm } from '../components/SamplingForm';
import { InstallProgress } from '../components/SetupWizard/InstallProgress';
import { StatusMark } from '../components/status/StatusMark';
import { Button } from '../components/ui/Button';
import { Checkbox } from '../components/ui/Checkbox';
import { Input } from '../components/ui/Input';
import { Select } from '../components/ui/Select';
import { installLlama, useEngineInstall } from '../places/library/engineInstall';
import type { GgufModel, LaunchExplanation, ModelDetail } from '../types';
import { formatError } from '../utils/errors';
import { ImageParts } from './ImageParts';
import { UNTOUCHED, defaultsPatch, launchConfig, problem, type SheetField, type Typed } from './loadOptions';
import type { Update } from './local/Change';
import type { LoadState } from './local/useLoad';
import { argsValue, argsWhy } from './local/rows';
import { Speculative } from './Speculative';

/** Where the sheet grows from: the ⋯ menu (top right), or a launch-line token (top left). */
export type SheetOrigin = 'menu' | 'line';

export interface SheetOpen {
  pin: boolean;
  focus?: SheetField;
  origin: SheetOrigin;
}

interface LoadOptionsSheetProps extends SheetOpen {
  model: GgufModel;
  detail: ModelDetail | null;
  explanation: LaunchExplanation | null;
  /** The explanation could not be read: the sampling captions say so. */
  failed: boolean;
  /**
   * Speculative is offered: the model has MTP heads (its `mtp` tag), its plan
   * drafts, or it has an MTP default of its own (so drafting can always be
   * turned off from here when something turns it on).
   */
  speculative: boolean;
  /** The settings' base port, which an automatic port counts up from. */
  basePort: number | null;
  load: Pick<LoadState, 'busy' | 'loadWith' | 'needsLlama'>;
  onUpdateModel: Update;
  /** Defaults were saved: read the explanation again. */
  onSaved: () => void;
  onClose: () => void;
}

const Field: FC<{ label: string; caption?: string; children: ReactNode }> = ({ label, caption, children }) => (
  <div className="flex flex-col gap-xs">
    <span className="text-xs font-semibold text-text">{label}</span>
    {children}
    {caption && <span className="whitespace-pre-line text-xs text-text-secondary">{caption}</span>}
  </div>
);

const JINJA = [
  ['', 'Auto'],
  ['on', 'On'],
  ['off', 'Off'],
] as const;

export const LoadOptionsSheet: FC<LoadOptionsSheetProps> = (props) => {
  const { model, detail, explanation, failed, speculative, basePort, load, onUpdateModel, onSaved, onClose, focus, origin } = props;
  const [typed, setTyped] = useState<Typed>({ ...UNTOUCHED, pin: props.pin });
  const [portOpen, setPortOpen] = useState(focus === 'port');
  const [imageReady, setImageReady] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const refs = {
    ctx: useRef<HTMLInputElement>(null),
    templates: useRef<HTMLSelectElement>(null),
    mtp: useRef<HTMLInputElement>(null),
    port: useRef<HTMLInputElement>(null),
    sampling: useRef<HTMLDivElement>(null),
  };

  const draws = model.imageFamily !== undefined;
  const plan = explanation?.plan ?? null;
  const sampling = explanation?.sampling ?? null;
  const argsOf = (decision: string) => plan?.args.filter((a) => a.decision === decision) ?? [];
  const mtpArgs = argsOf('mtp');
  const templatesArgs = argsOf('templates');
  const ctxArgs = argsOf('ctx');
  const busy = saving || load.busy !== null;
  const blocked = draws ? (imageReady ? null : '') : problem(typed, mtpArgs.length > 0);
  const change = (next: Partial<Typed>) => setTyped((t) => ({ ...t, ...next }));
  const ready = useCallback((r: boolean) => setImageReady(r), []);
  const llama = useEngineInstall();
  const installing = load.needsLlama && llama.installing;

  // Opened on a field: that field takes the focus.
  useEffect(() => {
    if (focus === 'reasoning') {
      refs.sampling.current?.querySelector<HTMLElement>('#inference-param-reasoningBudgetTokens')?.focus();
    } else if (focus) {
      refs[focus].current?.focus();
    }
    // Once, on opening.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const submit = async () => {
    setError(null);
    const patch = draws ? null : defaultsPatch(model, typed);
    if (patch) {
      setSaving(true);
      try {
        await onUpdateModel(model.id, patch);
        onSaved();
      } catch (err) {
        setError(`Didn't save: ${formatError(err)}`);
        return;
      } finally {
        setSaving(false);
      }
    }
    const failed = await load.loadWith(launchConfig(model, typed), typed.pin);
    if (failed === null) onClose();
    else setError(`Didn't load: ${failed}`);
  };

  // An install asked for here completed: load as it was asked to.
  const asked = useRef(false);
  const seen = useRef(llama.completed);
  useEffect(() => {
    if (llama.completed === seen.current) return;
    seen.current = llama.completed;
    if (!asked.current) return;
    asked.current = false;
    void submit();
    // `submit` is this render's, with what was typed; the effect runs on a completion only.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [llama.completed]);

  const go = () => {
    if (!load.needsLlama) return void submit();
    asked.current = true;
    installLlama();
  };
  const failure = error ?? (load.needsLlama && llama.refusal ? `Didn't load: ${llama.refusal}` : null);

  return (
    <div
      role="dialog"
      aria-label="Load with options"
      className="flex min-h-0 flex-1 animate-modal-slide-in flex-col bg-surface"
      style={{ transformOrigin: origin === 'menu' ? 'top right' : 'top left' }}
      onKeyDown={(e) => e.key === 'Escape' && !busy && onClose()}
    >
      <div className="flex flex-col gap-xs px-md pt-md">
        <h2 className="m-0 text-base font-semibold text-text">Load with options</h2>
        <p className="m-0 truncate text-xs text-text-secondary">{model.name}</p>
      </div>

      <div className="flex min-h-0 flex-1 flex-col gap-lg overflow-y-auto p-md">
        {draws ? (
          <ImageParts model={model} components={detail?.components} onReady={ready} />
        ) : (
          <>
            <Field label="Context" caption={ctxArgs.length ? argsWhy(ctxArgs) : undefined}>
              <Input
                ref={refs.ctx}
                size="sm"
                type="number"
                min={1}
                aria-label="Context"
                className="max-w-[10rem] font-mono tabular-nums"
                value={typed.context}
                placeholder={plan?.fit ? plan.fit.servesAt.toLocaleString('en-US') : undefined}
                disabled={busy}
                onChange={(e) => change({ context: e.target.value })}
              />
            </Field>
            <Field label="Jinja" caption={templatesArgs.length ? `Auto: ${argsValue(templatesArgs)} · ${argsWhy(templatesArgs)}` : undefined}>
              {/* The width is the wrapper's, so the select and its chevron are one box. */}
              <span className="block max-w-[10rem]">
                <Select
                  ref={refs.templates}
                  size="sm"
                  aria-label="Jinja"
                  value={typed.jinja === null ? '' : typed.jinja ? 'on' : 'off'}
                  disabled={busy}
                  onChange={(e) => change({ jinja: e.target.value === '' ? null : e.target.value === 'on' })}
                >
                  {JINJA.map(([value, label]) => (
                    <option key={value} value={value}>
                      {label}
                    </option>
                  ))}
                </Select>
              </span>
            </Field>
            {speculative && <Speculative typed={typed} args={mtpArgs} disabled={busy} onChange={change} focusRef={refs.mtp} />}
            <div ref={refs.sampling}>
              <SamplingForm
                value={typed.sampling}
                onChange={(next) => change({ sampling: next })}
                disabled={busy}
                fallback={{ kind: 'resolved', ownLayer: 'request', resolution: { explanation: sampling, isLoading: explanation === null && !failed, hasError: failed } }}
                capabilities={{ reasoningEffort: detail?.reasoningEffortSupport ?? 'unknown' }}
              />
            </div>
          </>
        )}

        <details open={portOpen} onToggle={(e) => setPortOpen(e.currentTarget.open)} className="text-xs">
          <summary className="cursor-pointer text-text">
            {'Port '}
            <span className="text-text-secondary">{basePort ? `Auto (from ${basePort})` : 'Auto'}</span>
          </summary>
          <Input
            ref={refs.port}
            size="sm"
            type="number"
            min={1024}
            max={65535}
            aria-label="Port"
            className="mt-xs max-w-[10rem] font-mono tabular-nums"
            value={typed.port}
            placeholder={basePort ? `Auto (from ${basePort})` : 'Auto'}
            disabled={busy}
            onChange={(e) => change({ port: e.target.value })}
          />
        </details>
        <Checkbox
          label="Pin the endpoint to this model"
          description="The endpoint serves only this model and refuses requests naming another."
          checked={typed.pin}
          disabled={busy}
          onChange={(e) => change({ pin: e.target.checked })}
        />
        {!draws && (
          <Checkbox
            label="This load only"
            description="Unchecked, what you set here also becomes this model's own."
            checked={typed.thisLoadOnly}
            disabled={busy}
            onChange={(e) => change({ thisLoadOnly: e.target.checked })}
          />
        )}
      </div>

      <div className="flex flex-col gap-sm border-t border-border px-md py-sm">
        {blocked && <p className="m-0 text-xs text-text-secondary">{blocked}</p>}
        {installing && <InstallProgress progress={llama.progress} />}
        {failure && (
          <p role="alert" className="m-0 flex items-baseline gap-xs text-xs text-text">
            <StatusMark kind="failed" label="Failed" />
            {failure}
          </p>
        )}
        <div className="flex justify-end gap-sm">
          <Button variant="ghost" onClick={onClose} disabled={busy}>
            Cancel
          </Button>
          <Button onClick={go} isLoading={busy || installing} disabled={blocked !== null}>
            {load.needsLlama ? 'Install llama.cpp' : 'Load'}
          </Button>
        </div>
      </div>
    </div>
  );
};
