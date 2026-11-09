/**
 * Setup: five steps in one centred 560 px column, the window's only
 * content while `SetupGate` holds it. This Mac, Engines, First model,
 * Endpoint, Connect; each step says "n of 5" and its 22 px headline, with
 * Back as text and Continue the one ink button. No stepper, no sidebar.
 *
 * The place has the setup status the gate read, and reads the
 * recommendation once; a recommendation that cannot be read is "Can't
 * reach gglib." with Try again. Everything a
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
import { getRecommendedModel } from '../../services/transport/api/setup';
import type { RecommendationDto, SetupStatus } from '../../types/setup';
import { useEndpointActions } from '../endpoint/useEndpointActions';
import { ConnectStep } from './ConnectStep';
import { EndpointStep } from './EndpointStep';
import { EnginesStep } from './EnginesStep';
import { FirstModelStep } from './FirstModelStep';
import { ThisMacStep } from './ThisMacStep';
import { useEngines } from './useEngines';
import { useFastDownloads } from './useFastDownloads';
import { useFirstModel } from './useFirstModel';

const Column: FC<{ children: React.ReactNode }> = ({ children }) => (
  <main aria-label="Setup" className="fixed inset-0 overflow-y-auto bg-background">
    <div className="mx-auto flex min-h-full w-full max-w-[560px] flex-col justify-center px-base py-xl">{children}</div>
  </main>
);

interface SetupPlaceProps {
  /** The setup status the gate read: one read, shared. */
  status: SetupStatus;
  onDone: () => void;
}

export default function SetupPlace({ status, onDone }: SetupPlaceProps) {
  // `undefined` while asked; `null` is the route's answer that nothing fits.
  const [rec, setRec] = useState<RecommendationDto | null | 'failed' | undefined>(undefined);

  const readRec = useCallback(() => {
    setRec(undefined);
    let live = true;
    getRecommendedModel()
      .then((value) => live && setRec(value))
      .catch(() => live && setRec('failed'));
    return () => {
      live = false;
    };
  }, []);

  useEffect(readRec, [readRec]);

  if (rec === 'failed') {
    return (
      <Column>
        <p role="alert" className="m-0 flex items-baseline gap-sm text-sm text-text">
          Can&apos;t reach gglib.
          <Button size="sm" onClick={() => void readRec()}>
            Try again
          </Button>
        </p>
      </Column>
    );
  }
  if (rec === undefined) return <Column>{null}</Column>;
  return (
    <Column>
      <Steps status={status} rec={rec} onDone={onDone} />
    </Column>
  );
}

interface StepsProps {
  status: SetupStatus;
  rec: RecommendationDto | null;
  onDone: () => void;
}

const Steps: FC<StepsProps> = ({ status, rec, onDone }) => {
  const navigate = useNavigate();
  const [step, setStep] = useState(1);
  const [folder, setFolder] = useState(status.modelsDirectory);
  const engines = useEngines();
  const fast = useFastDownloads(status.fastDownloads, status.pythonAvailable);
  const recommended = rec;
  const first = useFirstModel(recommended, fast.setUp);
  const endpoint = useEndpointActions();
  const facts = useEndpointFacts();
  const library = useModelLibrary();
  const chatModels = library.models.filter((m) => m.imageFamily === undefined);
  // What was already done when a step was first reached: `gglib up` ran first.
  const [had, setHad] = useState<{ model?: boolean; endpoint?: boolean }>({});
  const up = facts.state === 'on' || facts.state === 'pinned';

  // "Already done" moves a step on by itself once, ever: a step left any
  // way while it read "Already done" is not moved on again.
  const [advanced, setAdvanced] = useState<number[]>([]);
  const go = (by: number) => {
    if ((step === 3 && had.model) || (step === 4 && had.endpoint)) setAdvanced((a) => (a.includes(step) ? a : [...a, step]));
    setStep(step + by);
  };
  const next = () => go(1);
  const back = () => go(-1);

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


  // The model the endpoint is proved with and a client is told: the one
  // downloaded, else, with no download asked for, the smallest here.
  const smallest = [...chatModels].sort((a, b) => a.sizeBytes - b.sizeBytes)[0] ?? null;
  const model = first.choice === 'get' ? first.model : smallest;

  // The proof runs once the endpoint is up and the model is here, on step 4 or after.
  // A ref, not state: StrictMode runs the effect twice before a state update lands.
  const proved = useRef<number | null>(null);
  const { test } = endpoint;
  useEffect(() => {
    if (step < 4 || had.endpoint !== false || !up || facts.baseUrl === null || model === null || proved.current === model.id) return;
    proved.current = model.id;
    void test(facts.pinned !== null ? null : model.id);
  }, [step, had.endpoint, up, facts.baseUrl, facts.pinned, model, test]);

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
      return <EnginesStep engines={engines} fast={fast} onNext={next} onBack={back} />;
    case 3:
      return (
        <FirstModelStep
          rec={rec}
          first={first}
          preparing={fast.fast === 'setting-up'}
          alreadyDone={had.model === true}
          advance={advanced.includes(3) ? null : next}
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
          advance={advanced.includes(4) ? null : next}
          waitingOn={first.choice === 'get' && first.model === null ? (first.row?.text.title ?? recommended?.repo ?? null) : null}
          onRetryProof={() => model && void test(facts.pinned !== null ? null : model.id)}
          onNext={next}
          onBack={back}
        />
      );
    default:
      return <ConnectStep facts={facts} models={chatModels} model={model} onOpen={() => void open()} onBack={back} />;
  }
};
