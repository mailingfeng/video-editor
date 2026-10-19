import {act, cleanup, renderHook, waitFor} from '@testing-library/react';
import {afterEach, expect, it, vi} from 'vitest';
import type {DesktopApi} from '../../api/desktop';
import fixture from '../../../tests/fixtures/custom-settings.json';
import type {CustomSettingsStatus, JobSnapshot, QueueSnapshot} from '../../api/contracts';
import {useProcessing} from './useProcessing';

afterEach(()=>{cleanup();vi.restoreAllMocks();localStorage.clear();});
function deferred<T>() {let resolve!:(v:T)=>void;let reject!:(e:unknown)=>void;const promise=new Promise<T>((r,j)=>{resolve=r;reject=j;});return {promise,resolve,reject};}
const snap=(jobId:string):JobSnapshot=>({jobId,version:1,state:'running',progress:null,startedAtMs:1,endedAtMs:null,outputPath:null,error:null,cleanupPending:false});
const q:QueueSnapshot={version:1,running:true,items:['a','b'].map(id=>({itemId:id,inputPath:`/${id}.mp4`,state:'started',jobId:id,snapshot:snap(id),media:null}))};
function fakeApi() {
  const jobs=new Set<(s:JobSnapshot)=>void>(); const queues=new Set<(s:QueueSnapshot)=>void>();
  const defaults:CustomSettingsStatus={settings:{...fixture,schemaVersion:1},recommended:{...fixture,schemaVersion:1},source:'default',notice:null};
  const api:DesktopApi={available:true,getCustomSettings:vi.fn(async()=>defaults),saveCustomSettings:vi.fn(async(settings:CustomSettingsStatus['settings'])=>({...defaults,settings,source:'saved' as const})),pickInput:vi.fn(async()=>'/a.mp4'),pickInputFolder:vi.fn(async()=>null),pickOutputDirectory:vi.fn(async()=>'/out'),
    probeInput:vi.fn(),cancelProbe:vi.fn(),startJob:vi.fn(),getJobSnapshot:vi.fn(),cancelJob:vi.fn(),
    getLicenseStatus:vi.fn(async()=>({expiresAtMs:1798732800000,effectiveTimeMs:1791630000000,expired:false,ntpAvailable:true})),
    listPresets:vi.fn(async()=>[{presetId:'basic-transcode-v1',version:1,title:'基础转换',evidenceStatus:'checked'}]),
    getCurrentJobSnapshot:vi.fn(async()=>null),getQueueSnapshot:vi.fn(async()=>q),importFolder:vi.fn(),importPaths:vi.fn(),removeItem:vi.fn(),resetItem:vi.fn(),startBatch:vi.fn(),cancelItem:vi.fn(),
    getJobLog:vi.fn(async(id)=>({text:`log ${id}`,truncated:false})),revealOutput:vi.fn(async()=>undefined),
    subscribeSnapshots:vi.fn(async(fn)=>{jobs.add(fn);return ()=>{jobs.delete(fn);};}),
    subscribeQueue:vi.fn(async(fn)=>{queues.add(fn);return ()=>{queues.delete(fn);};}),subscribeFileDrop:vi.fn(async()=>()=>undefined)};
  return {api,jobs,queues};
}
const modePresets=['content-variation-v1','repeat-combined-v2','custom-variation-v1'].map(presetId=>({presetId,version:1,title:presetId,evidenceStatus:''}));
it.each(['repeat-combined-v2','custom-variation-v1'])('remembers the chosen mode %s and saved parameters across a fresh mount',async(presetId)=>{
  const {api}=fakeApi();
  const idle={version:1,running:false,items:[{...q.items[0],state:'waiting' as const,jobId:null,snapshot:null}]};
  vi.mocked(api.getQueueSnapshot).mockResolvedValue(idle);
  vi.mocked(api.listPresets).mockResolvedValue(modePresets);
  let saved:CustomSettingsStatus={settings:{...fixture,schemaVersion:1},recommended:{...fixture,schemaVersion:1},source:'default',notice:null};
  vi.mocked(api.getCustomSettings).mockImplementation(async()=>saved);
  vi.mocked(api.saveCustomSettings).mockImplementation(async(settings)=>saved={...saved,settings,source:'saved'});
  const first=renderHook(()=>useProcessing(api));
  await waitFor(()=>expect(first.result.current.preset?.presetId).toBe('content-variation-v1'));
  await waitFor(()=>expect(first.result.current.custom.loading).toBe(false));
  await act(()=>first.result.current.selectOutput());
  act(()=>first.result.current.selectPreset('custom-variation-v1'));
  act(()=>first.result.current.custom.openDialog());
  act(()=>first.result.current.custom.setLevel('pitch',7));
  await act(()=>first.result.current.custom.save());
  act(()=>first.result.current.custom.openDialog());
  act(()=>first.result.current.custom.setLevel('pitch',9));
  act(()=>first.result.current.custom.cancel());
  act(()=>first.result.current.selectPreset(presetId));
  first.unmount();
  const next=renderHook(()=>useProcessing(api));
  await waitFor(()=>expect(next.result.current.preset?.presetId).toBe(presetId));
  await waitFor(()=>expect(next.result.current.custom.status?.settings.pitch).toBe(7));
  await waitFor(()=>expect(next.result.current.licenseAllowed).toBe(true));
  vi.mocked(api.startBatch).mockResolvedValue(q);
  await act(()=>next.result.current.start());
  expect(api.startBatch).toHaveBeenCalledWith({outputDirectory:'/out',presetId,metadata:{mode:'preserve'},...(presetId==='custom-variation-v1'?{customSettings:{...fixture,pitch:7}}:{})});
});
it('falls back from an unavailable stored mode and keeps the selected mode usable if storage fails',async()=>{
  localStorage.setItem('frameshift.presetId.v1','removed-mode');
  const {api}=fakeApi();vi.mocked(api.listPresets).mockResolvedValue(modePresets);
  vi.mocked(api.getQueueSnapshot).mockResolvedValue({...q,running:false});
  const hook=renderHook(()=>useProcessing(api));
  await waitFor(()=>expect(hook.result.current.preset?.presetId).toBe('content-variation-v1'));
  vi.spyOn(Storage.prototype,'setItem').mockImplementation(()=>{throw new Error('storage unavailable');});
  act(()=>hook.result.current.selectPreset('custom-variation-v1'));
  expect(hook.result.current.preset?.presetId).toBe('custom-variation-v1');
  expect(hook.result.current.error).toContain('下次启动需重新选择');
});
it('remembers the last chosen output directory across remounts and uses it for the next batch',async()=>{
  const {api}=fakeApi();
  vi.mocked(api.getQueueSnapshot).mockResolvedValue({version:1,running:false,items:[{...q.items[0],state:'waiting',jobId:null,snapshot:null}]});
  vi.mocked(api.pickOutputDirectory).mockResolvedValueOnce('/输出目录/旧位置').mockResolvedValueOnce('/输出目录/新 位置').mockResolvedValueOnce(null);
  const first=renderHook(()=>useProcessing(api));
  await waitFor(()=>expect(first.result.current.queue.version).toBe(1));
  await act(()=>first.result.current.selectOutput());await act(()=>first.result.current.selectOutput());
  await act(()=>first.result.current.selectOutput());first.unmount();
  const next=renderHook(()=>useProcessing(api));
  expect(next.result.current.outputDirectory).toBe('/输出目录/新 位置');
  await waitFor(()=>expect(next.result.current.licenseAllowed&&next.result.current.preset!==null).toBe(true));
  vi.mocked(api.startBatch).mockResolvedValue(q);
  await act(()=>next.result.current.start());
  expect(api.startBatch).toHaveBeenCalledWith({outputDirectory:'/输出目录/新 位置',presetId:'basic-transcode-v1',metadata:{mode:'preserve'}});
});
it('does not persist a directory picker response that arrives after unmount',async()=>{
  const {api}=fakeApi();vi.mocked(api.getQueueSnapshot).mockResolvedValue({...q,running:false});
  const pending=deferred<string|null>();
  vi.mocked(api.pickOutputDirectory).mockResolvedValueOnce('/output/kept').mockReturnValueOnce(pending.promise);
  const first=renderHook(()=>useProcessing(api));await waitFor(()=>expect(first.result.current.queue.version).toBe(1));
  await act(()=>first.result.current.selectOutput());
  let selecting:Promise<void>;act(()=>{selecting=first.result.current.selectOutput();});first.unmount();
  await act(async()=>{pending.resolve('/output/late');await selecting;});
  const next=renderHook(()=>useProcessing(api));expect(next.result.current.outputDirectory).toBe('/output/kept');
});
it('keeps the chosen directory usable and explains when local storage cannot save it',async()=>{
  const {api}=fakeApi();vi.mocked(api.getQueueSnapshot).mockResolvedValue({...q,running:false});
  vi.spyOn(Storage.prototype,'getItem').mockImplementation(()=>{throw new Error('storage unavailable');});
  vi.spyOn(Storage.prototype,'setItem').mockImplementation(()=>{throw new Error('storage unavailable');});
  const hook=renderHook(()=>useProcessing(api));await waitFor(()=>expect(hook.result.current.queue.version).toBe(1));
  expect(hook.result.current.outputDirectory).toBe('');
  await act(()=>hook.result.current.selectOutput());
  expect(hook.result.current.outputDirectory).toBe('/out');expect(hook.result.current.error).toContain('下次启动需重新选择');
});
it('keeps selection after reset and ignores old logs, job events and queue responses',async()=>{
  const {api,jobs,queues}=fakeApi();
  const complete:QueueSnapshot={...q,running:false,items:q.items.map(i=>({...i,snapshot:{...i.snapshot!,state:'succeeded',progress:1}}))};
  vi.mocked(api.getQueueSnapshot).mockResolvedValue(complete);
  const pending=deferred<{text:string;truncated:boolean}>();
  vi.mocked(api.getJobLog).mockReturnValue(pending.promise);
  const hook=renderHook(()=>useProcessing(api));
  await waitFor(()=>expect(hook.result.current.queue.items).toHaveLength(2));
  act(()=>hook.result.current.selectItem('b'));
  let loading:Promise<void>;act(()=>{loading=hook.result.current.refreshLog();});
  const reset:QueueSnapshot={...complete,version:2,items:[complete.items[0],{...complete.items[1],itemId:'new-b',state:'waiting',jobId:null,snapshot:null,media:null}]};
  vi.mocked(api.resetItem).mockImplementation(async()=>{queues.forEach(fn=>fn(reset));return reset;});
  await act(()=>hook.result.current.resetItem('b'));
  await act(async()=>{pending.resolve({text:'old b log',truncated:false});await loading;});
  act(()=>{jobs.forEach(fn=>fn({...snap('b'),state:'succeeded',version:99}));queues.forEach(fn=>fn(complete));});
  expect(hook.result.current.selectedItem?.itemId).toBe('new-b');
  expect(hook.result.current.selectedItem?.snapshot).toBeNull();
  expect(hook.result.current.log).toBeNull();
  expect(hook.result.current.logLoading).toBe(false);
});
it('blocks duplicate reset and start while pending and keeps the completed row on failure',async()=>{
  const {api}=fakeApi();
  const complete:QueueSnapshot={...q,running:false,items:[{...q.items[0],snapshot:{...snap('a'),state:'succeeded',progress:1}}]};
  vi.mocked(api.getQueueSnapshot).mockResolvedValue(complete);
  const pending=deferred<QueueSnapshot>();vi.mocked(api.resetItem).mockReturnValue(pending.promise);
  const hook=renderHook(()=>useProcessing(api));
  await waitFor(()=>expect(hook.result.current.selectedItem?.snapshot?.state).toBe('succeeded'));
  await act(()=>hook.result.current.selectOutput());
  let resetting:Promise<void>;
  act(()=>{resetting=hook.result.current.resetItem('a');void hook.result.current.resetItem('a');void hook.result.current.start();});
  expect(hook.result.current.importing).toBe(true);
  expect(api.resetItem).toHaveBeenCalledOnce();expect(api.startBatch).not.toHaveBeenCalled();
  await act(async()=>{pending.reject({message:'暂时无法重置'});await resetting;});
  expect(hook.result.current.selectedItem?.snapshot?.state).toBe('succeeded');
  expect(hook.result.current.error).toBe('暂时无法重置');expect(hook.result.current.importing).toBe(false);
});
it('subscribes before query, restores queue on remount and releases both listeners',async()=>{
  const {api,jobs,queues}=fakeApi(); const hook=renderHook(()=>useProcessing(api));
  await waitFor(()=>expect(hook.result.current.queue.items).toHaveLength(2));
  expect(jobs.size).toBe(1); expect(queues.size).toBe(1);
  expect(vi.mocked(api.subscribeQueue).mock.invocationCallOrder[0]).toBeLessThan(vi.mocked(api.getQueueSnapshot).mock.invocationCallOrder[0]);
  hook.unmount(); expect(jobs.size).toBe(0); expect(queues.size).toBe(0);
  const next=renderHook(()=>useProcessing(api)); await waitFor(()=>expect(next.result.current.queue.items).toHaveLength(2));
  expect(jobs.size).toBe(1); expect(queues.size).toBe(1);
});
it('disposes late subscription after unmount',async()=>{
  const {api}=fakeApi(); const pending=deferred<()=>void>();const off=vi.fn();
  vi.mocked(api.subscribeSnapshots).mockReturnValue(pending.promise);
  const hook=renderHook(()=>useProcessing(api));hook.unmount();
  await act(async()=>pending.resolve(off)); expect(off).toHaveBeenCalledOnce();
});
it('ignores a late log response after selecting another item',async()=>{
  const {api}=fakeApi();const pending=deferred<{text:string;truncated:boolean}>();
  vi.mocked(api.getJobLog).mockImplementation(async(id)=>id==='a'?pending.promise:{text:'log b',truncated:false});
  const hook=renderHook(()=>useProcessing(api));await waitFor(()=>expect(hook.result.current.selectedItem?.itemId).toBe('a'));
  let loading:Promise<void>;act(()=>{loading=hook.result.current.refreshLog();});
  act(()=>hook.result.current.selectItem('b'));
  await act(async()=>{pending.resolve({text:'late a',truncated:false});await loading;});
  expect(hook.result.current.log?.text).not.toBe('late a');
  await act(()=>hook.result.current.refreshLog()); expect(hook.result.current.log?.text).toBe('log b');
});
it('canceled folder selection leaves existing queue untouched',async()=>{
  const {api}=fakeApi();vi.mocked(api.getQueueSnapshot).mockResolvedValue({...q,running:false});
  const hook=renderHook(()=>useProcessing(api));await waitFor(()=>expect(hook.result.current.queue.items).toHaveLength(2));
  await act(()=>hook.result.current.selectFolder());expect(api.importFolder).not.toHaveBeenCalled();
  expect(hook.result.current.queue.items).toHaveLength(2);
});

it('keeps a newer event arriving before the initial full query response',async()=>{
  const {api,jobs}=fakeApi();const pending=deferred<QueueSnapshot>();
  vi.mocked(api.getQueueSnapshot).mockReturnValueOnce(pending.promise).mockResolvedValue(q);
  const hook=renderHook(()=>useProcessing(api));await waitFor(()=>expect(api.getQueueSnapshot).toHaveBeenCalledOnce());
  act(()=>jobs.forEach(fn=>fn({...snap('a'),version:8,progress:0.75})));
  await act(async()=>pending.resolve(q));
  await waitFor(()=>expect(hook.result.current.queue.items[0].snapshot?.version).toBe(8));
  expect(hook.result.current.queue.items[0].snapshot?.progress).toBe(0.75);
  expect(hook.result.current.queue.items[1].snapshot?.version).toBe(1);
});

it('ignores a late import response after unmount',async()=>{
  const {api}=fakeApi();const pending=deferred<QueueSnapshot>();
  vi.mocked(api.getQueueSnapshot).mockResolvedValue({version:0,running:false,items:[]});
  vi.mocked(api.importPaths).mockReturnValue(pending.promise);
  const hook=renderHook(()=>useProcessing(api));await waitFor(()=>expect(api.getQueueSnapshot).toHaveBeenCalledOnce());
  let importing:Promise<void>;act(()=>{importing=hook.result.current.selectInput();});
  await waitFor(()=>expect(api.importPaths).toHaveBeenCalledOnce());hook.unmount();
  await act(async()=>{pending.resolve(q);await importing;});
  expect(api.startBatch).not.toHaveBeenCalled();
});

it('late_settings_do_not_override_active_batch',async()=>{
 localStorage.setItem('frameshift.presetId.v1','content-variation-v1');
 const {api,queues}=fakeApi();const pending=deferred<CustomSettingsStatus>();vi.mocked(api.getCustomSettings).mockReturnValue(pending.promise);
 const active={presetId:'custom-variation-v1',customSettings:{...fixture,schemaVersion:1 as const,speed:9}};
 vi.mocked(api.listPresets).mockResolvedValue([{presetId:'custom-variation-v1',version:1,title:'自定义变化',evidenceStatus:''},{presetId:'content-variation-v1',version:1,title:'内容变化',evidenceStatus:''}]);
 vi.mocked(api.getQueueSnapshot).mockResolvedValue({...q,version:8,activeSettings:active});
 const hook=renderHook(()=>useProcessing(api));await waitFor(()=>expect(hook.result.current.preset?.presetId).toBe('custom-variation-v1'));
 act(()=>queues.forEach(fn=>fn({...q,version:2,activeSettings:{presetId:'content-variation-v1'}})));
 await act(async()=>pending.resolve({settings:{...fixture,schemaVersion:1},recommended:{...fixture,schemaVersion:1},source:'saved',notice:null}));
 expect(hook.result.current.queue.activeSettings).toEqual(active);expect(hook.result.current.preset?.presetId).toBe('custom-variation-v1');
});
it('legacy_modes_do_not_wait_for_custom_loading',async()=>{
 const {api}=fakeApi();const pending=deferred<CustomSettingsStatus>();vi.mocked(api.getCustomSettings).mockReturnValue(pending.promise);
 vi.mocked(api.getQueueSnapshot).mockResolvedValue({version:1,running:false,items:[{...q.items[0],jobId:null,snapshot:null,state:'waiting'}]});
 vi.mocked(api.startBatch).mockResolvedValue(q);
 const hook=renderHook(()=>useProcessing(api));await waitFor(()=>expect(hook.result.current.licenseAllowed).toBe(true));
 await waitFor(()=>expect(hook.result.current.preset).not.toBeNull());await act(()=>hook.result.current.selectOutput());
 expect(hook.result.current.custom.loading).toBe(true);await act(()=>hook.result.current.start());
 expect(api.startBatch).toHaveBeenCalledWith({outputDirectory:'/out',presetId:'basic-transcode-v1',metadata:{mode:'preserve'}});
});
it('custom_uses_saved_levels_and_blocks_start_while_draft_is_open',async()=>{
 const {api}=fakeApi();vi.mocked(api.listPresets).mockResolvedValue([{presetId:'custom-variation-v1',version:1,title:'自定义变化',evidenceStatus:''}]);
 vi.mocked(api.getQueueSnapshot).mockResolvedValue({version:1,running:false,items:[{...q.items[0],jobId:null,snapshot:null,state:'waiting'}]});
 vi.mocked(api.startBatch).mockRejectedValue({code:'invalid_settings',message:'设置无效'});
 const hook=renderHook(()=>useProcessing(api));await waitFor(()=>expect(hook.result.current.custom.status).not.toBeNull());await waitFor(()=>expect(hook.result.current.licenseAllowed).toBe(true));
 await act(()=>hook.result.current.selectOutput());act(()=>hook.result.current.custom.openDialog());
 await act(()=>hook.result.current.start());expect(api.startBatch).not.toHaveBeenCalled();
 act(()=>hook.result.current.custom.cancel());await act(()=>hook.result.current.start());
 expect(api.startBatch).toHaveBeenCalledWith({outputDirectory:'/out',presetId:'custom-variation-v1',metadata:{mode:'preserve'},customSettings:fixture});
 expect(hook.result.current.custom.status?.settings).toEqual(fixture);
});
it('unknown_active_preset_never_falls_back_to_a_remembered_mode',async()=>{
 localStorage.setItem('frameshift.presetId.v1','basic-transcode-v1');
 const {api}=fakeApi();vi.mocked(api.getQueueSnapshot).mockResolvedValue({...q,activeSettings:{presetId:'future-mode'}});
 const hook=renderHook(()=>useProcessing(api));await waitFor(()=>expect(hook.result.current.queue.activeSettings?.presetId).toBe('future-mode'));
 await waitFor(()=>expect(hook.result.current.presets).toHaveLength(1));expect(hook.result.current.preset).toBeNull();
});
it('opening_a_draft_blocks_a_start_in_the_same_event',async()=>{
 const {api}=fakeApi();vi.mocked(api.getQueueSnapshot).mockResolvedValue({version:1,running:false,items:[{...q.items[0],jobId:null,snapshot:null,state:'waiting'}]});
 const hook=renderHook(()=>useProcessing(api));await waitFor(()=>expect(hook.result.current.custom.status).not.toBeNull());await waitFor(()=>expect(hook.result.current.licenseAllowed).toBe(true));
 await act(()=>hook.result.current.selectOutput());await act(async()=>{hook.result.current.custom.openDialog();await hook.result.current.start();});
 expect(api.startBatch).not.toHaveBeenCalled();expect(hook.result.current.custom.open).toBe(true);
});
