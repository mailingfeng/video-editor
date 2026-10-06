import {act, cleanup, renderHook, waitFor} from '@testing-library/react';
import {afterEach, expect, it, vi} from 'vitest';
import type {DesktopApi} from '../../api/desktop';
import type {JobSnapshot, QueueSnapshot} from '../../api/contracts';
import {useProcessing} from './useProcessing';

afterEach(cleanup);
function deferred<T>() {let resolve!:(v:T)=>void;const promise=new Promise<T>(r=>{resolve=r;});return {promise,resolve};}
const snap=(jobId:string):JobSnapshot=>({jobId,version:1,state:'running',progress:null,startedAtMs:1,endedAtMs:null,outputPath:null,error:null,cleanupPending:false});
const q:QueueSnapshot={version:1,running:true,items:['a','b'].map(id=>({itemId:id,inputPath:`/${id}.mp4`,state:'started',jobId:id,snapshot:snap(id),media:null}))};
function fakeApi() {
  const jobs=new Set<(s:JobSnapshot)=>void>(); const queues=new Set<(s:QueueSnapshot)=>void>();
  const api:DesktopApi={available:true,pickInput:vi.fn(async()=>'/a.mp4'),pickInputFolder:vi.fn(async()=>null),pickOutputDirectory:vi.fn(async()=>'/out'),
    probeInput:vi.fn(),cancelProbe:vi.fn(),startJob:vi.fn(),getJobSnapshot:vi.fn(),cancelJob:vi.fn(),
    listPresets:vi.fn(async()=>[{presetId:'basic-transcode-v1',version:1,title:'基础转换',evidenceStatus:'checked'}]),
    getCurrentJobSnapshot:vi.fn(async()=>null),getQueueSnapshot:vi.fn(async()=>q),importFolder:vi.fn(),importPaths:vi.fn(),removeItem:vi.fn(),startBatch:vi.fn(),cancelItem:vi.fn(),
    getJobLog:vi.fn(async(id)=>({text:`log ${id}`,truncated:false})),revealOutput:vi.fn(async()=>undefined),
    subscribeSnapshots:vi.fn(async(fn)=>{jobs.add(fn);return ()=>{jobs.delete(fn);};}),
    subscribeQueue:vi.fn(async(fn)=>{queues.add(fn);return ()=>{queues.delete(fn);};}),subscribeFileDrop:vi.fn(async()=>()=>undefined)};
  return {api,jobs,queues};
}
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
