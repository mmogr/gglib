/**
 * Setup's step 5, Connect: the Connect-a-client block the Endpoint place
 * shows (`components/ConnectClient`), its Model ID filled in with the first
 * model, then Open gglib. While the endpoint is not up there is no address
 * to give a client, and the step says so.
 *
 * @module places/setup/ConnectStep
 */

import { useState, type FC } from 'react';
import { ConnectClient } from '../../components/ConnectClient';
import { Button } from '../../components/ui/Button';
import type { EndpointFacts } from '../../services/proxyRegistry';
import type { GgufModel } from '../../types';
import { Step } from './Step';

interface ConnectStepProps {
  facts: EndpointFacts;
  /** The chat models a client may name. */
  models: GgufModel[];
  /** The first model, the Model ID filled in. */
  model: GgufModel | null;
  onOpen: () => void;
  onBack: () => void;
}

export const ConnectStep: FC<ConnectStepProps> = ({ facts, models, model, onOpen, onBack }) => {
  const [picked, setPicked] = useState<number | null>(null);
  const up = facts.state === 'on' || facts.state === 'pinned';
  return (
    <Step n={5} title="Connect" onBack={onBack} actions={<Button onClick={onOpen}>Open gglib</Button>}>
      {up && facts.baseUrl !== null ? (
        <div className="-mx-md">
          <ConnectClient
            baseUrl={facts.baseUrl}
            keyState={facts.key}
            models={models}
            modelId={picked ?? model?.id ?? null}
            onModel={setPicked}
          />
        </div>
      ) : (
        <p className="m-0 text-sm text-text-secondary">Clients connect once the endpoint is on: turn it on in Endpoint.</p>
      )}
    </Step>
  );
};
