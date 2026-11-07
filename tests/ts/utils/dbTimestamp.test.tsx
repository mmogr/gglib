/**
 * The database's zone-less timestamps read as UTC, at every place they are
 * shown.
 *
 * SQLite's `datetime('now')` writes `2026-10-01 08:00:00` for 08:00 UTC, and
 * `new Date` read that as 08:00 *local* — so in Brisbane, ten hours ahead,
 * every chat time showed ten hours early. These tests run in Brisbane, so a
 * site that reads the text as local shows 08:00 where 18:00 is right and
 * fails; under UTC, which CI runs in, the two readings agree and nothing
 * here could fail. The first assertion checks the zone took.
 */

import { afterAll, beforeAll, beforeEach, afterEach, describe, it, expect, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import { parseDbTimestamp } from '../../../src/utils/dbTimestamp';
import { buildLoadedMessage } from '../../../src/hooks/useChatPersistence/buildLoadedMessage';
import { buildThreadMessages } from '../../../src/hooks/useChatPersistence/buildThreadMessages';
import ConversationListPanel from '../../../src/components/ConversationListPanel/ConversationListPanel';
import { ModelMetadataGrid } from '../../../src/components/ModelInspectorPanel/components/ModelMetadataGrid';
import { Details } from '../../../src/inspector/local/Details';
import type { ModelDetailState } from '../../../src/inspector/local/useModelDetail';
import type { Verification, VerifyStatus } from '../../../src/inspector/local/useVerification';
import { ToastProvider } from '../../../src/contexts/ToastContext';
import type { ChatMessage, ConversationSummary } from '../../../src/services/transport';
import type { ModelDetail } from '../../../src/types';
import { guiModel } from '../fixtures/model';



/** 08:00 UTC, as SQLite writes it. */
const SAVED = '2026-10-01 08:00:00';
/** The same instant. */
const INSTANT = Date.UTC(2026, 9, 1, 8, 0, 0);

const zoneBefore = process.env.TZ;
beforeAll(() => {
  // Brisbane keeps no daylight saving, so it is +10 all year.
  process.env.TZ = 'Australia/Brisbane';
});
afterAll(() => {
  // Assigning `undefined` would store the string "undefined", a zone of UTC.
  if (zoneBefore === undefined) delete process.env.TZ;
  else process.env.TZ = zoneBefore;
});

describe('parseDbTimestamp', () => {
  it('runs under a +10 zone, or none of this proves anything', () => {
    expect(new Date(INSTANT).getTimezoneOffset()).toBe(-600);
  });

  it('reads a zone-less timestamp as UTC', () => {
    const parsed = parseDbTimestamp(SAVED);
    expect(parsed.getTime()).toBe(INSTANT);
    expect(parsed.getHours()).toBe(18);
  });

  it('reads the T-separated and fractional forms the same way', () => {
    expect(parseDbTimestamp('2026-10-01T08:00:00').getTime()).toBe(INSTANT);
    expect(parseDbTimestamp('2026-10-01 08:00:00.250').getTime()).toBe(INSTANT + 250);
  });

  it('leaves a timestamp that names its zone alone', () => {
    expect(parseDbTimestamp('2026-10-01T08:00:00Z').getTime()).toBe(INSTANT);
    expect(parseDbTimestamp('2026-10-01T18:00:00+10:00').getTime()).toBe(INSTANT);
    expect(parseDbTimestamp('2026-10-01T03:00:00-05:00').getTime()).toBe(INSTANT);
  });
});

describe('every place a database time is shown', () => {
  it('a saved message', () => {
    const row: ChatMessage = {
      id: 5,
      conversation_id: 1,
      role: 'user',
      content: 'hello',
      created_at: SAVED,
    };
    expect(buildLoadedMessage(row, 1).createdAt?.getHours()).toBe(18);
  });

  it("a conversation's system prompt", () => {
    const [prompt] = buildThreadMessages(
      [],
      { id: 1, system_prompt: 'be brief', created_at: SAVED },
      1,
    );
    expect(prompt.createdAt?.getHours()).toBe(18);
  });

  describe('the conversation list', () => {
    beforeEach(() => {
      vi.useFakeTimers({ toFake: ['Date'] });
      vi.setSystemTime(INSTANT + 30 * 60 * 1000);
    });
    afterEach(() => vi.useRealTimers());

    it('says how long ago a conversation changed', () => {
      const conversation: ConversationSummary = {
        id: 1,
        title: 'A chat',
        model_id: null,
        system_prompt: null,
        created_at: SAVED,
        updated_at: SAVED,
      };
      render(
        <ConversationListPanel
          conversations={[conversation]}
          activeConversationId={null}
          onSelectConversation={vi.fn()}
          searchQuery=""
          onSearchChange={vi.fn()}
          loading={false}
        />,
      );
      const thirtyMinutesAgo = new Intl.RelativeTimeFormat(undefined, { numeric: 'auto' })
        .format(-30, 'minute');
      expect(screen.getByText(thirtyMinutesAgo)).toBeInTheDocument();
    });
  });

  it("a model's download and update-check times", () => {
    const detail = {
      metadata: {},
      downloadDate: SAVED,
      lastUpdateCheck: SAVED,
    } as unknown as ModelDetail;
    render(<ModelMetadataGrid model={guiModel()} detail={detail} />);

    const shown = new Date(INSTANT).toLocaleString();
    expect(screen.getAllByText(shown)).toHaveLength(2);
  });

  /** A verification in `status`, its actions inert. */
  const verification = (status: VerifyStatus = { kind: 'idle' }): Verification => ({
    status,
    busy: false,
    verify: vi.fn(),
    checkUpdates: vi.fn(),
    repairDamaged: vi.fn(async () => {}),
    downloadChanged: vi.fn(async () => {}),
    redownloadLatest: vi.fn(async () => {}),
  });

  it("the inspector's Details: downloaded and files checked, exact on hover", () => {
    localStorage.setItem('gglib.inspector.details', 'open');
    const model = guiModel({ id: 3, hfRepoId: 'owner/repo' });
    const detail = {
      modelDetail: { id: 3, metadata: {}, tags: [], downloadDate: SAVED, lastUpdateCheck: SAVED } as unknown as ModelDetail,
      tags: [],
      newTagInput: '',
      setNewTagInput: vi.fn(),
      reload: vi.fn(async () => {}),
      addTag: vi.fn(async () => {}),
      removeTag: vi.fn(async () => {}),
      isLoading: false,
    } satisfies ModelDetailState;
    render(
      <ToastProvider>
        <Details model={model} detail={detail} loaded={false} onUpdateModel={vi.fn(async () => {})} onChanged={vi.fn()} verification={verification()} reveal={0} />
      </ToastProvider>,
    );

    const shown = new Date(INSTANT).toLocaleString();
    expect(screen.getByText(/^downloaded /)).toHaveAttribute('title', shown);
    expect(screen.getByText(/^files checked /)).toHaveAttribute('title', shown);
    localStorage.removeItem('gglib.inspector.details');
  });

  it("the inspector's Details: a file check's own time, read as UTC, in place of the stored one", () => {
    localStorage.setItem('gglib.inspector.details', 'open');
    const model = guiModel({ id: 3, hfRepoId: 'owner/repo' });
    const detail = {
      modelDetail: { id: 3, metadata: {}, tags: [], lastUpdateCheck: '2026-01-01 00:00:00' } as unknown as ModelDetail,
      tags: [],
      newTagInput: '',
      setNewTagInput: vi.fn(),
      reload: vi.fn(async () => {}),
      addTag: vi.fn(async () => {}),
      removeTag: vi.fn(async () => {}),
      isLoading: false,
    } satisfies ModelDetailState;
    const file = { index: 0, kind: { kind: 'shard' as const, number: 1, of: 1 }, file_path: 'a.gguf', health: { type: 'healthy' as const } };
    const report = { model_id: 3, overall_health: 'healthy' as const, shards: [file], verified_at: SAVED };
    render(
      <ToastProvider>
        <Details model={model} detail={detail} loaded={false} onUpdateModel={vi.fn(async () => {})} onChanged={vi.fn()} verification={verification({ kind: 'verified', report })} reveal={0} />
      </ToastProvider>,
    );

    expect(screen.getByText(/^files checked /)).toHaveAttribute('title', new Date(INSTANT).toLocaleString());
    expect(screen.getByRole('img', { name: 'intact' })).toBeInTheDocument();
    localStorage.removeItem('gglib.inspector.details');
  });
});
