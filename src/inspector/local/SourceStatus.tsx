/**
 * The line under Details › Source that a file check or an update check
 * writes: how far the check is (the file the daemon is hashing, with a 2 px
 * bar), then its answer and the one action it offers, or the daemon's
 * refusal with Retry. A file check's answer is the daemon's verdict in its
 * word, or its line for how many files are damaged, and opens a list of every file, closed until asked, each named by
 * what it is and its health's word, a damaged file's expected and actual
 * hash on hover.
 *
 * @module inspector/local/SourceStatus
 */

import type { FC, ReactNode } from 'react';
import { Check, ChevronRight } from 'lucide-react';
import { StatusMark } from '../../components/status/StatusMark';
import { FILE_HEALTH_WORDS, VERDICT_WORDS } from '../../components/status/words';
import { Button } from '../../components/ui/Button';
import { Icon } from '../../components/ui/Icon';
import type { VerificationReport } from '../../services/transport';
import type { VerifiedFileKind } from '../../types/generated/VerifiedFileKind';
import type { Verification } from './useVerification';

/** A file named by what it is, as `VerifiedFileKind::label` names it: a shard counted over the weights alone, from 1. */
const fileLabel = (kind: VerifiedFileKind): string => {
  switch (kind.kind) {
    case 'shard':
      return `Shard ${kind.number}/${kind.of}`;
    case 'projector':
      return 'Projector';
    case 'companion':
      return 'Companion';
  }
};

const plural = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`;

const Action: FC<{ onClick: () => void; disabled?: boolean; children: ReactNode }> = ({ onClick, disabled, children }) => (
  <Button variant="link" size="sm" className="text-xs font-normal" onClick={onClick} disabled={disabled}>
    {children}
  </Button>
);

/** An answer that is good news: a check and the words. */
const Good: FC<{ title?: string; children: ReactNode }> = ({ title, children }) => (
  <span className="inline-flex items-center gap-xs" title={title}>
    <Icon icon={Check} size={12} className="text-success" />
    {children}
  </span>
);

const Line: FC<{ children: ReactNode }> = ({ children }) => <span className="flex flex-wrap items-baseline gap-x-sm gap-y-xs">{children}</span>;

/** Every file the check read, closed until opened from the line. */
const FileList: FC<{ report: VerificationReport }> = ({ report }) => (
  <details className="group/files">
    <summary className="flex cursor-pointer list-none items-center gap-xs text-text-secondary [&::-webkit-details-marker]:hidden">
      <Icon icon={ChevronRight} size={12} className="group-open/files:rotate-90" />
      {plural(report.shards.length, 'file', 'files')}
    </summary>
    <ul aria-label="Files checked" className="m-0 mt-xs flex list-none flex-col gap-xs p-0">
      {report.shards.map((file) => (
        <li
          key={file.index}
          className="flex items-baseline gap-sm"
          title={file.health.type === 'corrupt' ? `Expected ${file.health.expected}\nActual ${file.health.actual}` : undefined}
        >
          <span className="shrink-0 text-text-secondary">{fileLabel(file.kind)}</span>
          <span className="min-w-0 flex-1 truncate font-mono">{file.file_path.split('/').pop()}</span>
          <span className="shrink-0">{FILE_HEALTH_WORDS[file.health.type]}</span>
        </li>
      ))}
    </ul>
  </details>
);

/** The daemon's verdict in its word, or how many files are damaged in its line, with Re-download for those. */
const Verified: FC<{ v: Verification; report: VerificationReport }> = ({ v, report }) => {
  const verdict = report.overall_health;
  const { damage } = report;
  return (
    <>
      <Line>
        <span role="status">
          {verdict === 'healthy' ? (
            <Good>{VERDICT_WORDS[verdict]}</Good>
          ) : (
            <StatusMark kind="attention" word={damage?.line ?? VERDICT_WORDS[verdict]} className="text-xs text-text" />
          )}
        </span>
        {damage && (
          <Action onClick={() => void v.repairDamaged(damage)} disabled={v.busy}>
            {damage.files.length === 1 ? 'Re-download it' : 'Re-download them'}
          </Action>
        )}
      </Line>
      <FileList report={report} />
    </>
  );
};

const Checked: FC<{ v: Verification; status: Extract<Verification['status'], { kind: 'checked' }> }> = ({ v, status }) => {
  const { upgrade, updates } = status;
  const changed = updates.update_available ? (updates.details?.changes.length ?? 0) : 0;
  const offerChanged = changed > 0 && (
    <Action onClick={() => void v.downloadChanged(updates)} disabled={v.busy}>
      Download changed files
    </Action>
  );
  const offerLatest = (
    <Action onClick={() => void v.redownloadLatest()} disabled={v.busy}>
      Re-download at latest…
    </Action>
  );
  // No recorded revision: the check reports an update only for want of a baseline, so it says that instead.
  if (upgrade.current === null)
    return (
      <Line>
        <StatusMark kind="attention" word="No revision is recorded for this model, so there is nothing to compare." className="text-xs text-text" />
        {offerChanged}
        {offerLatest}
      </Line>
    );
  if (!upgrade.hasUpdate && changed === 0) return <Good title={upgrade.current.value}>{`Up to date (${upgrade.current.short})`}</Good>;
  return (
    <Line>
      <span title={`${upgrade.current.short} → ${upgrade.latest.short}`}>
        <StatusMark
          kind="attention"
          word={changed > 0 ? `A newer revision exists: ${plural(changed, 'file', 'files')} changed` : 'A newer revision exists (may be README only)'}
          className="text-xs text-text"
        />
      </span>
      {offerChanged}
      {upgrade.hasUpdate && offerLatest}
    </Line>
  );
};

export const SourceStatus: FC<{ v: Verification }> = ({ v }) => {
  const { status } = v;
  switch (status.kind) {
    case 'idle':
      return null;
    case 'verifying':
      return (
        <span className="flex flex-col gap-xs" role="status">
          <StatusMark kind="changing" word={status.file ? `Checking ${status.file.label}… ${status.file.percent}%` : 'Checking files…'} className="text-xs" />
          <span
            role="progressbar"
            aria-label="Files checked"
            aria-valuemin={0}
            aria-valuemax={100}
            aria-valuenow={status.file?.percent ?? 0}
            className="h-[2px] w-[96px] overflow-hidden rounded-full bg-surface-elevated"
          >
            {/* Dynamic: the bar's fill is the file's percentage. */}
            <span className="block h-full bg-text-secondary" style={{ width: `${status.file?.percent ?? 0}%` }} />
          </span>
        </span>
      );
    case 'verified':
      return <Verified v={v} report={status.report} />;
    case 'checking':
      return <StatusMark kind="changing" word="Checking for updates…" className="text-xs" />;
    case 'checked':
      return <Checked v={v} status={status} />;
    case 'queued':
      return <span>Sent to the download queue.</span>;
    case 'failed':
      return (
        <Line>
          <span role="alert" className="flex items-baseline gap-xs">
            <StatusMark kind="failed" label="Failed" />
            {status.message}
          </span>
          <Action onClick={status.retry === 'verify' ? v.verify : v.checkUpdates}>Retry</Action>
        </Line>
      );
  }
};
