import type { GgufModel, ServerInfo, HfModelSummary, HfSearchResponse, HfQuantizationsResponse, AppSettings } from '../../types';
import type { DownloadQueueStatus } from '../../services/transport/types/downloads';
import type { ProxyStatus } from '../../services/transport/types/proxy';

export type Model = GgufModel;
export type ServerStatus = ServerInfo;

export type HuggingFaceModel = HfModelSummary;
export type HuggingFaceSearchResponse = HfSearchResponse;
export type HfQuantizations = HfQuantizationsResponse;

export type DownloadsStatus = DownloadQueueStatus;

export type ProxyState = ProxyStatus;

export type Settings = AppSettings;
