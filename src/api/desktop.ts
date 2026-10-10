import { invoke, isTauri } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import { open } from '@tauri-apps/plugin-dialog';
import type { BatchSettings, JobSnapshot, LicenseStatus, LogExcerpt, MediaInfo, PresetSummary, QueueSnapshot, StartJobRequest } from './contracts';

export interface DesktopApi {
  available: boolean;
  getLicenseStatus(): Promise<LicenseStatus>;
  pickInput(): Promise<string | null>;
  pickInputFolder(): Promise<string | null>;
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
  getQueueSnapshot(): Promise<QueueSnapshot>;
  importFolder(path:string): Promise<QueueSnapshot>;
  importPaths(paths:string[]): Promise<QueueSnapshot>;
  removeItem(itemId:string): Promise<QueueSnapshot>;
  startBatch(settings:BatchSettings): Promise<QueueSnapshot>;
  cancelItem(itemId:string): Promise<QueueSnapshot>;
  subscribeQueue(listener:(snapshot:QueueSnapshot)=>void): Promise<() => void>;
  subscribeFileDrop(listener: (paths: string[]) => void): Promise<() => void>;
}
export const desktopApi: DesktopApi = {
  available: isTauri(),
  getLicenseStatus: () => invoke('get_license_status'),
  pickInput: async () => {
    const selected = await open({multiple: false, directory: false, filters: [{name: '视频文件', extensions: ['mp4','mov','m4v','mkv','webm']}]});
    return typeof selected === 'string' ? selected : null;
  },
  pickOutputDirectory: async () => {
    const selected = await open({multiple: false, directory: true});
    return typeof selected === 'string' ? selected : null;
  },
  probeInput: (path) => invoke('probe_input', {path}),
  pickInputFolder: async () => {
    const selected = await open({multiple:false,directory:true});
    return typeof selected === 'string' ? selected : null;
  },
  getQueueSnapshot:()=>invoke('get_queue_snapshot'),
  importFolder:(path)=>invoke('import_folder',{path}),
  importPaths:(paths)=>invoke('import_paths',{paths}),
  removeItem:(itemId)=>invoke('remove_item',{itemId}),
  startBatch:(settings)=>invoke('start_batch',{settings}),
  cancelItem:(itemId)=>invoke('cancel_item',{itemId}),
  subscribeQueue:(listener)=>listen<QueueSnapshot>('queue_snapshot',(event)=>listener(event.payload)),
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
