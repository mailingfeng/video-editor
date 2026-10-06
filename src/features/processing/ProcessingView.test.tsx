import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { DesktopApi } from '../../api/desktop';
import type { JobSnapshot, MediaInfo } from '../../api/contracts';
import { ProcessingView } from './ProcessingView';

afterEach(cleanup);
function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}
const media: MediaInfo = {
  identity: { canonicalPath: '/input/原 视频.mp4', sizeBytes: 12345, modifiedNs: '123', sha256: 'a'.repeat(64) },
  container: 'mp4', title: null, comment: null,
  video: { streamIndex: 0, codec: 'h264', width: 720, height: 1280, bitDepth: 8, pixelFormat: 'yuv420p', frameRate: {num: 30, den: 1}, timeBase: {num: 1, den: 15360}, frameCount: 60, startPts: 0, durationTicks: 30720, bitRate: null, colorRange: null, colorSpace: null, colorPrimaries: null, colorTransfer: null },
  audio: null,
};
function snapshot(jobId = 'job-1', state: JobSnapshot['state'] = 'running', version = 2): JobSnapshot {
  return {jobId, state, version, progress: 0.4, startedAtMs: 123, endedAtMs: null, outputPath: null, error: null, cleanupPending: false};
}
function fakeApi() {
  const listeners = new Set<(s: JobSnapshot) => void>();
  const api: DesktopApi = {
    available: true,
    pickInput: vi.fn(async () => media.identity.canonicalPath),
    pickOutputDirectory: vi.fn(async () => '/output'),
    probeInput: vi.fn(async () => media), cancelProbe: vi.fn(async () => undefined),
    listPresets: vi.fn(async () => [{presetId: 'basic-transcode-v1', version: 1, title: '基础转换', evidenceStatus: '本地规格校验'}]),
    startJob: vi.fn(async () => 'job-1'), getJobSnapshot: vi.fn(async (id) => snapshot(id)),
    getCurrentJobSnapshot: vi.fn(async () => null), cancelJob: vi.fn(async (id) => snapshot(id, 'canceled', 5)),
    getJobLog: vi.fn(async () => ({text: 'actual log', truncated: false})), revealOutput: vi.fn(async () => undefined),
    subscribeSnapshots: vi.fn(async (listener) => {listeners.add(listener); return () => {listeners.delete(listener);};}),
    subscribeFileDrop: vi.fn(async () => () => undefined),
  };
  return {api, listeners, emit: (s: JobSnapshot) => {listeners.forEach((fn) => fn(s));}};
}
async function ready(api: DesktopApi) {
  render(<ProcessingView api={api} />);
  const user = userEvent.setup();
  await user.click(screen.getByRole('button', {name: '选择视频'}));
  await screen.findByText('原 视频.mp4');
  await user.click(screen.getByRole('button', {name: '选择输出目录'}));
  await waitFor(() => expect(screen.getByRole('button', {name: '开始处理'})).toBeEnabled());
  return user;
}
describe('desktop processing behavior', () => {
  it('partial_color_information_is_displayed_without_a_completeness_claim', async () => {
    const {api} = fakeApi();
    vi.mocked(api.probeInput).mockResolvedValue({...media, video:{...media.video,colorRange:'pc'}});
    await ready(api);
    expect(screen.getByText(/范围 pc/)).toBeVisible();
    expect(screen.getByText(/部分颜色信息缺失/)).toBeVisible();
  });
  it('start_is_single_while_pending', async () => {
    const {api} = fakeApi(); const pending = deferred<string>();
    vi.mocked(api.startJob).mockReturnValue(pending.promise);
    await ready(api);
    const start = screen.getByRole('button', {name: '开始处理'});
    fireEvent.click(start); fireEvent.click(start);
    expect(api.startJob).toHaveBeenCalledTimes(1);
    await act(async () => pending.resolve('job-1'));
  });
  it('old_snapshot_never_overwrites_new', async () => {
    const {api, emit} = fakeApi(); const user = await ready(api);
    await user.click(screen.getByRole('button', {name: '开始处理'}));
    await screen.findByText('处理中');
    act(() => emit({...snapshot('job-1', 'succeeded', 8), progress: 1, outputPath: '/output/result.mp4'}));
    act(() => emit(snapshot('job-1', 'running', 7)));
    expect(screen.getByText('处理完成')).toBeVisible();
    expect(screen.queryByText('处理中')).not.toBeInTheDocument();
    await user.click(screen.getByRole('button', {name: '在文件夹中显示'}));
    expect(api.revealOutput).toHaveBeenCalledWith('job-1');
  });
  it('importing_another_video_clearly_labels_the_previous_result', async () => {
    const {api, emit} = fakeApi(); const user = await ready(api);
    await user.click(screen.getByRole('button', {name: '开始处理'})); await screen.findByText('处理中');
    act(() => emit({...snapshot('job-1','succeeded',8), progress:1, outputPath:'/output/previous.mp4'}));
    vi.mocked(api.pickInput).mockResolvedValue('/input/另一个视频.mp4');
    vi.mocked(api.probeInput).mockResolvedValue({...media, identity:{...media.identity,canonicalPath:'/input/另一个视频.mp4',sha256:'b'.repeat(64)}});
    await user.click(screen.getByRole('button', {name:'选择视频'}));
    await screen.findByText('另一个视频.mp4');
    expect(screen.getByText('以下为上一次任务结果，当前视频尚未处理。')).toBeVisible();
  });
  it('previous_job_events_do_not_replace_new_job', async () => {
    const {api, emit} = fakeApi(); const user = await ready(api);
    await user.click(screen.getByRole('button', {name: '开始处理'}));
    await screen.findByText('处理中');
    act(() => emit(snapshot('job-1', 'canceled', 8)));
    vi.mocked(api.startJob).mockResolvedValue('job-2');
    await user.click(screen.getByRole('button', {name: '开始处理'}));
    await screen.findByText('处理中');
    act(() => emit({...snapshot('job-1', 'failed', 99), error: {code:'validation_failed', message:'旧任务错误', details:null}}));
    expect(screen.queryByText('旧任务错误')).not.toBeInTheDocument();
    expect(screen.getByText('处理中')).toBeVisible();
  });
  it('remount_has_one_subscription_and_restores_current_job', async () => {
    const {api, listeners} = fakeApi();
    vi.mocked(api.getCurrentJobSnapshot).mockResolvedValue(snapshot('restored'));
    const first = render(<ProcessingView api={api} />);
    await screen.findByText('处理中'); expect(listeners.size).toBe(1);
    first.unmount(); expect(listeners.size).toBe(0);
    render(<ProcessingView api={api} />);
    await screen.findByText('处理中'); expect(listeners.size).toBe(1);
    expect(api.getCurrentJobSnapshot).toHaveBeenCalledTimes(2);
  });
  it('late_subscription_is_removed_after_unmount', async () => {
    const {api} = fakeApi(); const pending = deferred<() => void>(); const off = vi.fn();
    vi.mocked(api.subscribeSnapshots).mockReturnValue(pending.promise);
    const view = render(<ProcessingView api={api} />); view.unmount();
    await act(async () => pending.resolve(off));
    expect(off).toHaveBeenCalledTimes(1);
  });
  it('probe_cancel_resets_import_state', async () => {
    const {api} = fakeApi(); const pending = deferred<MediaInfo>();
    vi.mocked(api.probeInput).mockReturnValue(pending.promise);
    vi.mocked(api.cancelProbe).mockImplementation(async () => {pending.reject({code:'canceled', message:'已取消检查'});});
    render(<ProcessingView api={api} />); const user = userEvent.setup();
    await user.click(screen.getByRole('button', {name:'选择视频'}));
    await user.click(await screen.findByRole('button', {name:'取消检查'}));
    await waitFor(() => expect(screen.queryByText('正在检查视频')).not.toBeInTheDocument());
    expect(screen.getByRole('button', {name:'选择视频'})).toBeEnabled();
    expect(screen.getByRole('button', {name:'开始处理'})).toBeDisabled();
  });
  it('failed_validation_shows_error_without_result_action', async () => {
    const {api, emit} = fakeApi(); const user = await ready(api);
    await user.click(screen.getByRole('button', {name:'开始处理'})); await screen.findByText('处理中');
    act(() => emit({...snapshot('job-1', 'failed', 9), error:{code:'validation_failed', message:'帧数校验失败', details:'expected 60, got 59'}}));
    expect(screen.getByText('帧数校验失败')).toBeVisible();
    expect(screen.queryByRole('button', {name:'在文件夹中显示'})).not.toBeInTheDocument();
  });
});
