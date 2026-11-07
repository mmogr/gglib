/**
 * Settings › Endpoint: its port and key, who may reach it, its prompt cache
 * on disk, where the model servers behind it start, and under "What gglib
 * does to requests" the four things the endpoint does to a request. Each
 * saves on change; those the endpoint reads when it turns on say so.
 *
 * The loop guard is on or off here: on writes `note`, off writes `off`.
 * `refuse` stays the terminal's (`gglib config settings set
 * --loop-guard-mode refuse`), and is shown as on with a line saying so.
 * The guard's trips sit under it until Diagnostics has its Live tab (G1).
 *
 * @module places/settings/EndpointSection
 */

import type { FC } from 'react';
import { LLAMA_BASE_PORT, PROXY_PORT } from '../../constants/settingsDefaults';
import type { LoopGuardMode } from '../../types/generated/LoopGuardMode';
import { Disclosure } from './Disclosure';
import { NumberSetting, TextSetting, ToggleSetting, numberOrNull } from './fields';
import { LoopGuardTripsPanel } from './LoopGuardTripsPanel';
import { ON_ENDPOINT_START, ON_GGLIB_START, ON_NEXT_LOAD } from './notes';
import { SecretSetting } from './SecretSetting';

/** The hosts as the box shows them, one comma between each. */
export const hostsText = (hosts: string[] | null) => (hosts ?? []).join(', ');

/** The hosts typed, one per comma; an empty box is no list. */
export function hostsFrom(typed: string): string[] | null {
  const hosts = typed
    .split(',')
    .map((host) => host.trim())
    .filter(Boolean);
  return hosts.length > 0 ? hosts : null;
}

/** Absent means on, for the three corrections the endpoint should not lose silently. */
const onUnlessOff = (stored: boolean | null) => stored !== false;

export const EndpointSection: FC = () => (
  <>
    <NumberSetting
      k="proxyPort"
      id="proxy-port-input"
      label="Port"
      spec={PROXY_PORT}
      description="Where the OpenAI-compatible endpoint listens."
      note={ON_ENDPOINT_START}
    />

    <SecretSetting note={ON_ENDPOINT_START} />

    <ToggleSetting k="shareLan" id="share-lan-input" label="Share on the local network" note={ON_GGLIB_START}>
      Binds gglib beyond loopback: every device on your network can reach it, and its management
      API can download models and start or stop them on this machine. Set an API key first.
    </ToggleSetting>

    <TextSetting
      k="bindHost"
      id="bind-host-input"
      label="Bind address"
      defaultHint="127.0.0.1"
      placeholder="127.0.0.1"
      mono
      description="The literal IP gglib binds; a hostname is refused, so the bind and the mDNS record agree. --host overrides it for one run."
      note={ON_GGLIB_START}
    />

    <TextSetting
      k="proxyHost"
      id="proxy-host-input"
      label="Endpoint address"
      defaultHint="127.0.0.1"
      placeholder="127.0.0.1"
      mono
      description="The literal IP the endpoint binds, for Turn on, the tray, proxy and serve alike. 0.0.0.0 accepts LAN clients and makes the endpoint demand the API key. --host on proxy and serve overrides it for one run."
      note={ON_ENDPOINT_START}
    />

    <TextSetting
      k="proxyAllowedHosts"
      id="proxy-allowed-hosts-input"
      label="Allowed hosts"
      defaultHint="none"
      placeholder="gglib.lan, 192.168.1.20"
      mono
      toText={hostsText}
      fromText={hostsFrom}
      description="Host header values the endpoint accepts beyond loopback and its own address, comma-separated: a client reaching a wildcard bind by a name or a LAN IP needs that value here."
      note={ON_ENDPOINT_START}
    />

    <ToggleSetting k="proxyCacheOnDisk" id="proxy-cache-on-disk-input" label="Keep the prompt cache on disk" note={ON_ENDPOINT_START}>
      Keeps prompt caches across model swaps and restarts, in the data folder&apos;s slots.
    </ToggleSetting>

    <TextSetting
      k="proxyCacheDiskGb"
      id="proxy-cache-disk-gb-input"
      label="Disk budget (GiB)"
      type="number"
      min="1"
      width="xs"
      defaultHint="sized from free disk"
      placeholder="auto"
      fromText={numberOrNull}
      description="How much disk the prompt cache may hold before its oldest slots are swept, at least 1 GiB."
      note={ON_ENDPOINT_START}
    />

    <NumberSetting
      k="llamaBasePort"
      id="server-port-input"
      label="Model servers start at"
      spec={LLAMA_BASE_PORT}
      description="The first port a model's server is given."
      note={ON_NEXT_LOAD}
    />

    <Disclosure label="What gglib does to requests">
      <ToggleSetting k="toolCallRepair" id="tool-call-repair-input" label="Repair broken tool calls" on={onUnlessOff}>
        A tool call that fails its schema is asked for once more under a grammar, so a malformed call
        is usually repaired rather than forwarded as a broken turn. Turn it off to measure what a
        model produces.
      </ToggleSetting>

      <ToggleSetting
        k="loopGuardMode"
        id="loop-guard-input"
        label="Stop runaway loops"
        on={(mode: LoopGuardMode | null) => mode !== 'off'}
        toWire={(checked): LoopGuardMode => (checked ? 'note' : 'off')}
        aside={(mode: LoopGuardMode | null) =>
          mode === 'refuse' && (
            <p className="m-0 text-xs text-text-secondary">
              Refusing, as <code>gglib config settings set --loop-guard-mode refuse</code> set it: a
              looping request is answered with a 400.
            </p>
          )
        }
      >
        A conversation that repeats the same tool calls and gets the same answers back, or repeats one
        reply, is forwarded with a note saying what repeated, and its run ends if it repeats again.
      </ToggleSetting>
      <LoopGuardTripsPanel />

      <ToggleSetting k="agenticSampling" id="agentic-sampling-input" label="Cap sampling on agent runs" on={onUnlessOff}>
        A turn that may call a tool has its temperature capped to 0.3, over a value nobody chose and
        never on a reasoning model. Anything a person set stands.
      </ToggleSetting>

      <ToggleSetting k="trustClientSampling" id="trust-client-sampling-input" label="Let clients set sampling">
        Off, a client&apos;s temperature, top-p, top-k, min-p and penalties are dropped and gglib&apos;s
        defaults apply: most clients send fixed values nobody chose. A client&apos;s max tokens and
        reasoning budget always apply.
      </ToggleSetting>
    </Disclosure>
  </>
);
