import { invoke, isTauri } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import { open } from '@tauri-apps/plugin-dialog';
import type { JobSnapshot, LogExcerpt, MediaInfo, PresetSummary, StartJobRequest } from './contracts';

export interface DesktopApi {
  available: boolean;
  pickInput(): Promise<string | null>;
  pickOutputDirectory(): Promise<string | null>;
  probeInput(path: string): Promise<MediaInfo>;
  cancelProbe(): Promise<void>;
  listPresets(): Promise<PresetSummary[]>;
  startJob(request: StartJobRequest): Promise<string>;
  getJobSnapshot(jobId: string): Promise<JobSnapshot>;
  getCurrentJobSnapshot(): Promise<JobSnapshot | null>;
  cancelJob(jobId: string): Promise<JobSnapshot>;
  getJobLog(jobId: string): Promise<LogExcerpt>;
  revealOutput(jobId: string): Promise<void>;
  subscribeSnapshots(listener: (snapshot: JobSnapshot) => void): Promise<() => void>;
  subscribeFileDrop(listener: (paths: string[]) => void): Promise<() => void>;
}
export const desktopApi: DesktopApi = {
  available: isTauri(),
  pickInput: async () => {
    const selected = await open({multiple: false, directory: false, filters: [{name: 'MP4 视频', extensions: ['mp4']}]});
    return typeof selected === 'string' ? selected : null;
  },
  pickOutputDirectory: async () => {
    const selected = await open({multiple: false, directory: true});
    return typeof selected === 'string' ? selected : null;
  },
  probeInput: (path) => invoke('probe_input', {path}),
  cancelProbe: () => invoke('cancel_probe'),
  listPresets: () => invoke('list_presets'),
  startJob: (request) => invoke('start_job', {request}),
  getJobSnapshot: (jobId) => invoke('get_job_snapshot', {jobId}),
  getCurrentJobSnapshot: () => invoke('get_current_job_snapshot'),
  cancelJob: (jobId) => invoke('cancel_job', {jobId}),
  getJobLog: (jobId) => invoke('get_job_log', {jobId}),
  revealOutput: (jobId) => invoke('reveal_output', {jobId}),
  subscribeSnapshots: (listener) => listen<JobSnapshot>('job_snapshot', (event) => listener(event.payload)),
  subscribeFileDrop: (listener) => getCurrentWebview().onDragDropEvent((event) => {
    if (event.payload.type === 'drop') listener(event.payload.paths);
  }),
};
