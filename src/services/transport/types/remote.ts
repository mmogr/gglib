/**
 * Remote tunnel types (ADR 0012) — the daemon's `/api/remote/*` shapes.
 *
 * Every one is the generated binding, re-exported: the ticket appears in
 * exactly one of them (`RemoteEnableResponse`, the answer to `enable`), and
 * the status carries fingerprints only, which is a property of the Rust side
 * this file must not weaken with a hand-written mirror.
 */

export type { RemoteStatus } from '../../../types/generated/RemoteStatus';
export type { RemotePeer } from '../../../types/generated/RemotePeer';
export type { RemoteConnection } from '../../../types/generated/RemoteConnection';
export type { RemoteEnableBody } from '../../../types/generated/RemoteEnableBody';
export type { RemoteEnableResponse } from '../../../types/generated/RemoteEnableResponse';
export type { RemoteJoinBody } from '../../../types/generated/RemoteJoinBody';
export type { RemoteJoinResponse } from '../../../types/generated/RemoteJoinResponse';
export type { RemoteDevice } from '../../../types/generated/RemoteDevice';
export type { RemoteForgotten } from '../../../types/generated/RemoteForgotten';
