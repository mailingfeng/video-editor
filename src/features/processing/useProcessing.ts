import { useCallback, useEffect, useRef, useState } from 'react';
import type { DesktopApi } from '../../api/desktop';
import type { AppError, JobSnapshot, LogExcerpt, MediaInfo, PresetSummary, StartJobRequest } from '../../api/contracts';

export const terminal = (s: JobSnapshot | null) => !s || ['succeeded', 'failed', 'canceled'].includes(s.state);
function message(error: unknown): string {
  if (typeof error === 'object' && error !== null && 'message' in error) return String(error.message);
  return typeof error === 'string' ? error : '操作未完成，请查看日志后重试。';
}
export function useProcessing(api: DesktopApi) {
  const [media, setMedia] = useState<MediaInfo | null>(null);
  const [presets, setPresets] = useState<PresetSummary[]>([]);
  const [outputDirectory, setOutputDirectory] = useState('');
  const [snapshot, setSnapshot] = useState<JobSnapshot | null>(null);
  const [jobSourceKey, setJobSourceKey] = useState<string | null>(null);
  const [log, setLog] = useState<LogExcerpt | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [probing, setProbing] = useState(false);
  const [starting, setStarting] = useState(false);
  const [cancelingProbe, setCancelingProbe] = useState(false);
  const mounted = useRef(false);
  const startLock = useRef(false);
  const importLock = useRef(false);
  const generation = useRef(0);
  const jobId = useRef<string | null>(null);
  const snapshotRef = useRef<JobSnapshot | null>(null);
  const busy = starting || probing || !terminal(snapshot);

  const merge = useCallback((next: JobSnapshot) => {
    if (!mounted.current || (startLock.current && !jobId.current)) return;
    if (jobId.current && jobId.current !== next.jobId) return;
    const previous = snapshotRef.current;
    if (previous?.jobId === next.jobId && previous.version >= next.version) return;
    jobId.current = next.jobId;
    snapshotRef.current = next;
    setSnapshot(next);
  }, []);

  const importPath = useCallback(async (path: string) => {
    if (importLock.current || startLock.current || !terminal(snapshotRef.current)) return;
    importLock.current = true;
    const attempt = ++generation.current;
    setError(null); setMedia(null); setProbing(true);
    try {
      const info = await api.probeInput(path);
      if (mounted.current && attempt === generation.current) setMedia(info);
    } catch (e) {
      if (mounted.current && attempt === generation.current && (e as AppError)?.code !== 'canceled') setError(message(e));
    } finally {
      if (attempt === generation.current) {
        importLock.current = false;
        if (mounted.current) setProbing(false);
      }
    }
  }, [api]);

  useEffect(() => {
    mounted.current = true;
    let alive = true;
    const disposers: (() => void)[] = [];
    const add = (off: () => void) => {if (alive) disposers.push(off); else off();};
    const initialGeneration = generation.current;
    if (api.available) {
      void api.subscribeSnapshots((next) => {if (alive) merge(next);}).then(async (off) => {
        add(off);
        if (!alive) return;
        const restored = await api.getCurrentJobSnapshot();
        if (alive && generation.current === initialGeneration && restored) merge(restored);
      }).catch((e) => {if (alive) setError(message(e));});
      void api.subscribeFileDrop((paths) => {
        if (paths.length !== 1) {setError('每次请选择一个 MP4 视频。'); return;}
        void importPath(paths[0]);
      }).then(add).catch((e) => {if (alive) setError(message(e));});
      void api.listPresets().then((items) => {if (alive) setPresets(items);}).catch((e) => {if (alive) setError(message(e));});
    }
    return () => {alive = false; mounted.current = false; disposers.forEach((off) => off());};
  }, [api, importPath, merge]);

  async function selectInput() {
    if (busy || importLock.current) return;
    try {const path = await api.pickInput(); if (path && mounted.current) await importPath(path);}
    catch (e) {if (mounted.current) setError(message(e));}
  }
  async function selectOutput() {
    try {const path = await api.pickOutputDirectory(); if (path && mounted.current) setOutputDirectory(path);}
    catch (e) {if (mounted.current) setError(message(e));}
  }
  async function cancelProbe() {
    if (cancelingProbe) return;
    setCancelingProbe(true);
    try {
      await api.cancelProbe(); ++generation.current; importLock.current = false;
      if (mounted.current) {setProbing(false); setMedia(null);}
    } catch (e) {if (mounted.current) setError(message(e));}
    finally {if (mounted.current) setCancelingProbe(false);}
  }
  async function start(metadata: StartJobRequest['metadata']) {
    if (startLock.current || importLock.current || !terminal(snapshotRef.current) || !media || !outputDirectory || !presets[0]) return;
    startLock.current = true; ++generation.current; jobId.current = null; snapshotRef.current = null;
    setJobSourceKey(`${media.identity.canonicalPath}\n${media.identity.sha256}`);
    setStarting(true); setSnapshot(null); setLog(null); setError(null);
    try {
      const id = await api.startJob({inputPath: media.identity.canonicalPath, outputDirectory, presetId: presets[0].presetId, metadata});
      jobId.current = id;
      merge(await api.getJobSnapshot(id));
    } catch (e) {if (mounted.current) setError(message(e));}
    finally {startLock.current = false; if (mounted.current) setStarting(false);}
  }
  async function cancelJob() {
    if (!jobId.current) return;
    try {merge(await api.cancelJob(jobId.current));}
    catch (e) {if (mounted.current) setError(message(e));}
  }
  const refreshLog = useCallback(async () => {
    const id = jobId.current;
    if (!id) return;
    try {const result = await api.getJobLog(id); if (mounted.current && id === jobId.current) setLog(result);}
    catch (e) {if (mounted.current) setError(message(e));}
  }, [api]);
  useEffect(() => {if (snapshot && terminal(snapshot)) void refreshLog();}, [snapshot?.jobId, snapshot?.state, refreshLog]);
  async function reveal() {
    if (snapshotRef.current?.state !== 'succeeded' || !jobId.current) return;
    try {await api.revealOutput(jobId.current);} catch (e) {if (mounted.current) setError(message(e));}
  }
  return {media, presets, outputDirectory, snapshot, jobSourceKey, log, error, probing, starting, busy, cancelingProbe,
    selectInput, selectOutput, cancelProbe, start, cancelJob, refreshLog, reveal};
}
