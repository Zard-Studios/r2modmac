import type { Dispatch, SetStateAction } from 'react';

export interface ProgressState {
  isOpen: boolean;
  title: string;
  progress: number;
  currentTask: string;
  downloadSpeedBps?: number;
  downloadedBytes?: number;
  totalBytes?: number;
  activeDownloads?: number;
  isCancelable?: boolean;
  operation?: 'custom-import' | 'mod-sync';
}

export interface ModDownloadProgressEvent {
  mod_name: string;
  downloaded_bytes: number;
  total_bytes?: number | null;
  speed_bps: number;
  progress_percent: number;
  done: boolean;
}

export type ProgressSetter = Dispatch<SetStateAction<ProgressState>>;
