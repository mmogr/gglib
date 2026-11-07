/**
 * Setup: five steps in one centred 560 px column, the window's only
 * content while `SetupGate` holds it. This Mac, Engines, First model,
 * Endpoint, Connect; each step says "n of 5" and its 22 px headline, with
 * Back as text and Continue the one ink button. No stepper, no sidebar.
 *
 * The place reads the setup status and the recommendation once; a status
 * that cannot be read is "Can't reach gglib." with Try again. Everything a
 * step starts (the installs, the download, the endpoint, the proof) is held
 * here, across the steps, so Back starts nothing twice. When `gglib up`
 * ran first, the engines read ready, and First model and Endpoint read
 * "Already done" and move on by themselves.
 *
 * Open gglib writes `setupCompleted` and lands on the Library with the new
 * model in the inspector (`/models/:id`): the one downloaded, else the
 * library's smallest chat model; with Choose another, the Library's search
 * for Hugging Face. Nothing opens setup again.
 *
 * @module places/setup/SetupPlace
 */

import { useCallback, useEffect, useRef, useState, type FC } from 'react';
import { useNavigate } from 'react-router';
import { paths } from '../../app/paths';
import { Button } from '../../components/ui/Button';
import { useModelLibrary } from '../../services/modelRegistry';
import { useEndpointFacts } from '../../services/proxyRegistry';
import { getTransport } from '../../services/transport';
import { getRecommendedModel, getSetupStatus } from '../../services/transport/api/setup';
import type { RecommendationDto, SetupStatus } from '../../types/setup';
import { formatError } from '../../utils/errors';
import { useEndpointActions } from '../endpoint/useEndpointActions';
import { ConnectStep } from './ConnectStep';
import { EndpointStep } from './EndpointStep';
import { EnginesStep } from './EnginesStep';
import { FirstModelStep } from './FirstModelStep';
import { ThisMacStep } from './ThisMacStep';
import { useEngines } from './useEngines';
import { useFirstModel } from './useFirstModel';

/** The recommendation as read: the model, `null` when nothing fits, or why it could not be read. */
export type RecRead = { rec: RecommendationDto | null } | { failed: string };

const Column: FC<{ children: React.ReactNode }> = ({ children }) => (
  <main aria-label="Setup" className="fixed inset-0 overflow-y-auto bg-background">
    <div className="mx-auto flex min-h-full w-full max-w-[560px] flex-col justify-center px-base py-xl">{children}</div>
  </main>
);

export default function SetupPlace({ onDone }: { onDone: () => void }) {
  const [status, setStatus] = useState<SetupStatus | 'failed' | null>(null);
  const [rec, setRec] = useState<RecRead | null>(null);

  const readRec = useCallback(() => {
    getRecommendedModel()
      .then((value) => setRec({ rec: value }))
      .catch((err: unknown) => setRec({ failed: formatError(err) }));
  }, []);

  const load = useCallback(() => {
    setStatus(null);
    getSetupStatus()
      .then(setStatus)
      .catch(() => setStatus('failed'));
    readRec();
  }, [readRec]);

  useEffect(load, [load]);

  if (status === 'failed') {
    return (
      <Column>
        <p role="alert" className="m-0 flex items-baseline gap-sm text-sm text-text">
          Can&apos;t reach gglib.
          <Button size="sm" onClick={load}>
            Try again
          </Button>
        </p>
      </Column>
    );
  }
  if (status === null || rec === null) return <Column>{null}</Column>;
  return (
    <Column>
      <Steps status={status} rec={rec} onReadRec={readRec} onDone={onDone} />
    </Column>
  );
}

interface StepsProps {
  status: SetupStatus;
  rec: RecRead;
  onReadRec: () => void;
  onDone: () => void;
}

const Steps: FC<StepsProps> = ({ status, rec, onReadRec, onDone }) => {
  const navigate = useNavigate();
  const [step, setStep] = useState(1);
  const [folder, setFolder] = useState(status.modelsDirectory);
  const engines = useEngines(status.fastDownloadReady, status.pythonAvailable);
  const recommended = 'rec' in rec ? rec.rec : null;
  const first = useFirstModel(recommended);
  const endpoint = useEndpointActions();
  const facts = useEndpointFacts();
  const library = useModelLibrary();
  const chatModels = library.models.filter((m) => m.imageFamily === undefined);
  // What was already done when a step was first reached: `gglib up` ran first.
  const [had, setHad] = useState<{ model?: boolean; endpoint?: boolean }>({});
  const up = facts.state === 'on' || facts.state === 'pinned';

  const next = useCallback(() => setStep((s) => s + 1), []);
  const back = () => setStep((s) => s - 1);

  // Step 2 starts its installs on its first arrival.
  const { start } = engines;
  useEffect(() => {
    if (step >= 2) start();
  }, [step, start]);

  if (step === 3 && had.model === undefined && !library.loading) {
    setHad({ ...had, model: chatModels.length > 0 && first.choice === null });
  }
  if (step === 4 && had.endpoint === undefined) setHad({ ...had, endpoint: had.model === true && up });

  // Step 4 turns the endpoint on, once, unless it was on already.
  const turned = useRef(false);
  const { turnOn } = endpoint;
  useEffect(() => {
    if (step !== 4 || had.endpoint !== false || turned.current) return;
    turned.current = true;
    if (!up) void turnOn();
  }, [step, had.endpoint, up, turnOn]);

  // "Already done" moves a step on by itself once; Back to it stays there.
  const [advanced, setAdvanced] = useState<number[]>([]);
  const advance = useCallback(
    (n: number) => () => {
      setAdvanced((a) => [...a, n]);
      next();
    },
    [next],
  );

  // The model the endpoint is proved with and a client is told: the one
  // downloaded, else, with no download asked for, the smallest here.
  const smallest = [...chatModels].sort((a, b) => a.sizeBytes - b.sizeBytes)[0] ?? null;
  const model = first.choice === 'get' ? first.model : smallest;

  // The proof runs once the endpoint is up and the model is here, on step 4 or after.
  const [proved, setProved] = useState<number | null>(null);
  const { test } = endpoint;
  useEffect(() => {
    if (step < 4 || had.endpoint !== false || !up || facts.baseUrl === null || model === null || proved === model.id) return;
    setProved(model.id);
    void test(facts.pinnedModel !== null ? null : model.id);
  }, [step, had.endpoint, up, facts.baseUrl, facts.pinnedModel, model, proved, test]);

  const open = async () => {
    try {
      // Written straight to the daemon: it is not a setting anyone chose, so no "applied".
      await getTransport().updateSettings({ setupCompleted: true });
    } catch {
      // A setting that did not save keeps nobody out: setup still ends here.
    }
    navigate(first.choice === 'another' ? paths.libraryAdd('browse') : model ? paths.model(model.id) : paths.library());
    onDone();
  };

  switch (step) {
    case 1:
      return <ThisMacStep status={status} rec={recommended} folder={folder} onFolder={setFolder} onNext={next} />;
    case 2:
      return <EnginesStep engines={engines} onNext={next} onBack={back} />;
    case 3:
      return (
        <FirstModelStep
          rec={rec}
          first={first}
          alreadyDone={had.model === true}
          advance={advanced.includes(3) ? null : advance(3)}
          onReadRec={onReadRec}
          onNext={next}
          onBack={back}
        />
      );
    case 4:
      return (
        <EndpointStep
          facts={facts}
          actions={endpoint}
          alreadyDone={had.endpoint === true}
          advance={advanced.includes(4) ? null : advance(4)}
          waitingOn={first.choice === 'get' && first.model === null ? (first.row?.text.title ?? recommended?.repo ?? null) : null}
          onRetryProof={() => model && void test(facts.pinnedModel !== null ? null : model.id)}
          onNext={next}
          onBack={back}
        />
      );
    default:
      return <ConnectStep facts={facts} models={chatModels} model={model} onOpen={() => void open()} onBack={back} />;
  }
};
