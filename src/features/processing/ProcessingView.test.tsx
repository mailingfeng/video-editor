import {act,cleanup,fireEvent,render,screen,waitFor,within} from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import {afterEach,describe,expect,it,vi} from 'vitest';
import type {DesktopApi} from '../../api/desktop';
import type {JobSnapshot,MediaInfo,QueueItem,QueueSnapshot} from '../../api/contracts';
import {ProcessingView} from './ProcessingView';
afterEach(cleanup);
const media:MediaInfo={identity:{canonicalPath:'/input/原 视频.mp4',sizeBytes:12345,modifiedNs:'123',sha256:'a'.repeat(64)},container:'mp4',title:null,comment:null,video:{streamIndex:0,codec:'h264',width:720,height:1280,bitDepth:8,pixelFormat:'yuv420p',frameRate:{num:30,den:1},timeBase:{num:1,den:15360},frameCount:60,startPts:0,durationTicks:30720,bitRate:null,colorRange:null,colorSpace:null,colorPrimaries:null,colorTransfer:null},audio:null};
const snapshot=(jobId='job-a',state:JobSnapshot['state']='running',version=2):JobSnapshot=>({jobId,state,version,progress:0.4,startedAtMs:123,endedAtMs:null,outputPath:null,error:null,cleanupPending:false});
const waiting=(itemId='a',inputPath=media.identity.canonicalPath):QueueItem=>({itemId,inputPath,state:'waiting',jobId:null,snapshot:null,media:null});
function deferred<T>(){let resolve!:(v:T)=>void;const promise=new Promise<T>(r=>{resolve=r;});return {promise,resolve};}
function fakeApi(initial:QueueItem[]=[]){
  let q:QueueSnapshot={version:1,running:false,items:initial};
  const listeners=new Set<(s:JobSnapshot)=>void>();const queues=new Set<(s:QueueSnapshot)=>void>();let drop:(paths:string[])=>void=()=>{};
  const api:DesktopApi={available:true,pickInput:vi.fn(async()=>media.identity.canonicalPath),pickInputFolder:vi.fn(async()=>'/input'),pickOutputDirectory:vi.fn(async()=>'/output'),probeInput:vi.fn(),cancelProbe:vi.fn(),startJob:vi.fn(),getJobSnapshot:vi.fn(),cancelJob:vi.fn(),
    listPresets:vi.fn(async()=>[{presetId:'basic-transcode-v1',version:1,title:'基础转换',evidenceStatus:'checked'}]),getCurrentJobSnapshot:vi.fn(async()=>null),
    getQueueSnapshot:vi.fn(async()=>q),importPaths:vi.fn(async(paths:string[])=>{q={...q,version:q.version+1,items:[...q.items,...paths.filter(path=>!q.items.some(i=>i.inputPath===path)).map((path,index)=>waiting(`added-${index}`,path))]};return q;}),
    importFolder:vi.fn(async()=>{q={...q,version:q.version+1,items:[waiting('a'),waiting('b','/input/b.mp4'),waiting('c','/input/c.mp4')]};return q;}),
    removeItem:vi.fn(async(id)=>{q={...q,version:q.version+1,items:q.items.filter(i=>i.itemId!==id)};return q;}),
    startBatch:vi.fn(async()=>{q={...q,version:q.version+1,running:true,items:q.items.map((i,index)=>index<2?{...i,state:'started',jobId:`job-${i.itemId}`,snapshot:snapshot(`job-${i.itemId}`)}:i)};return q;}),
    cancelItem:vi.fn(async(id)=>{q={...q,version:q.version+1,items:q.items.map(i=>i.itemId===id?{...i,state:i.jobId?'started':'canceled',snapshot:i.jobId?snapshot(i.jobId,'canceling',5):null}:i)};return q;}),
    getJobLog:vi.fn(async(id)=>({text:`log ${id}`,truncated:false})),revealOutput:vi.fn(async()=>undefined),
    subscribeSnapshots:vi.fn(async(fn)=>{listeners.add(fn);return ()=>{listeners.delete(fn);};}),subscribeQueue:vi.fn(async(fn)=>{queues.add(fn);return ()=>{queues.delete(fn);};}),subscribeFileDrop:vi.fn(async(fn)=>{drop=fn;return ()=>{drop=()=>{};};})};
  return {api,emit:(s:JobSnapshot)=>{listeners.forEach(fn=>fn(s));},emitQueue:(next:QueueSnapshot)=>{q=next;queues.forEach(fn=>fn(next));},drop:(paths:string[])=>drop(paths)};
}
async function ready(api:DesktopApi){render(<ProcessingView api={api}/>);const user=userEvent.setup();await screen.findByRole('button',{name:'选择文件夹'});await user.click(screen.getByRole('button',{name:'选择输出目录'}));return user;}
const row=(name:string)=>screen.getByRole('listitem',{name});
describe('batch desktop interface',()=>{
  it('imports folder candidates before probing and removes only the selected row',async()=>{
    const {api}=fakeApi();const user=await ready(api);await user.click(screen.getByRole('button',{name:'选择文件夹'}));
    await screen.findByText('b.mp4');expect(api.probeInput).not.toHaveBeenCalled();
    expect(within(row('原 视频.mp4')).getByText('待处理')).toBeVisible();
    await user.click(within(row('b.mp4')).getByRole('button',{name:'移除 b.mp4'}));
    expect(api.removeItem).toHaveBeenCalledWith('b');expect(screen.queryByText('b.mp4')).not.toBeInTheDocument();expect(screen.getByText('c.mp4')).toBeVisible();
  });
  it('shows two independent progresses and freezes imports and shared settings',async()=>{
    const {api,emit}=fakeApi([waiting('a'),waiting('b','/input/b.mp4'),waiting('c','/input/c.mp4')]);const user=await ready(api);
    await user.click(screen.getByRole('button',{name:'开始处理'}));
    await waitFor(()=>expect(screen.getByRole('button',{name:'选择文件夹'})).toBeDisabled());
    act(()=>emit({...snapshot('job-a','validating',6),progress:0.999}));
    act(()=>emit({...snapshot('job-b','running',4),progress:0.25}));
    expect(within(row('原 视频.mp4')).getByText('校验结果')).toBeVisible();expect(within(row('b.mp4')).getByText('25%')).toBeVisible();
    expect(within(row('c.mp4')).getByText('待处理')).toBeVisible();expect(screen.queryByText('100%')).not.toBeInTheDocument();
    expect(screen.getByRole('checkbox',{name:'自定义标题与备注'})).toBeDisabled();
    expect(screen.getByRole('button',{name:'移除 c.mp4'})).toBeDisabled();expect(screen.getByRole('button',{name:'选择输出目录'})).toBeDisabled();
  });
  it('cancels one waiting item without changing others',async()=>{
    const {api}=fakeApi([waiting('a'),waiting('b','/input/b.mp4'),waiting('c','/input/c.mp4')]);const user=await ready(api);await user.click(screen.getByRole('button',{name:'开始处理'}));
    await user.click(within(row('c.mp4')).getByRole('button',{name:'取消 c.mp4'}));
    expect(api.cancelItem).toHaveBeenCalledWith('c');expect(within(row('c.mp4')).getByText('已取消')).toBeVisible();
    expect(within(row('b.mp4')).getByText('处理中')).toBeVisible();
  });
  it('does not show completion until terminal and reveals the correct output',async()=>{
    const {api,emit}=fakeApi([waiting('a'),waiting('b','/input/b.mp4')]);const user=await ready(api);await user.click(screen.getByRole('button',{name:'开始处理'}));
    act(()=>emit({...snapshot('job-b','succeeded',8),progress:1,outputPath:'/output/b.mp4'}));act(()=>emit(snapshot('job-b','running',99)));
    expect(within(row('b.mp4')).getByText('100%')).toBeVisible();
    await user.click(within(row('b.mp4')).getByRole('button',{name:'显示 b.mp4 的结果'}));expect(api.revealOutput).toHaveBeenCalledWith('job-b');
  });
  it('unknown percent shows phase and failures keep their own error and log',async()=>{
    const failed={...snapshot('job-b','failed',9),progress:null,error:{code:'validation_failed' as const,message:'帧数校验失败',details:'expected 60, got 59'}};
    const {api}=fakeApi([{...waiting('a'),state:'started',jobId:'job-a',snapshot:{...snapshot(),progress:null}}, {...waiting('b','/input/b.mp4'),state:'started',jobId:'job-b',snapshot:failed}]);const user=await ready(api);
    expect(within(row('原 视频.mp4')).getByText('处理中')).toBeVisible();expect(within(row('原 视频.mp4')).queryByText(/%/)).not.toBeInTheDocument();
    await user.click(within(row('b.mp4')).getByRole('button',{name:'查看 b.mp4'}));expect(screen.getByText('帧数校验失败')).toBeVisible();
    await user.click(screen.getByRole('button',{name:'查看任务日志'}));expect(await screen.findByText('log job-b')).toBeVisible();expect(screen.queryByRole('button',{name:'在文件夹中显示'})).not.toBeInTheDocument();
  });
  it('shows media details after check without claiming complete color information',async()=>{
    const {api}=fakeApi([{...waiting('a'),media:{...media,video:{...media.video,colorRange:'pc'}}}]);await ready(api);
    expect(screen.getByText(/部分颜色信息缺失/)).toBeVisible();expect(screen.getByText(/范围 pc/)).toBeVisible();
  });
  it('start stays single while pending and override applies to the batch',async()=>{
    const {api}=fakeApi([waiting('a')]);const pending=deferred<QueueSnapshot>();vi.mocked(api.startBatch).mockReturnValue(pending.promise);const user=await ready(api);
    await user.click(screen.getByRole('checkbox',{name:'自定义标题与备注'}));await user.type(screen.getByLabelText('标题'),'batch title');
    const start=screen.getByRole('button',{name:'开始处理'});fireEvent.click(start);fireEvent.click(start);
    expect(api.startBatch).toHaveBeenCalledTimes(1);expect(api.startBatch).toHaveBeenCalledWith(expect.objectContaining({metadata:{mode:'override',title:'batch title',comment:null}}));
    await act(async()=>pending.resolve({version:2,running:true,items:[{...waiting('a'),state:'started',jobId:'job-a',snapshot:snapshot()}]}));
  });
  it('preserves single file choosing and drag import',async()=>{
    const {api,drop}=fakeApi();const user=await ready(api);await user.click(screen.getByRole('button',{name:'选择视频'}));await screen.findByRole('listitem',{name:'原 视频.mp4'});expect(api.importPaths).toHaveBeenCalledWith([media.identity.canonicalPath]);
    await act(async()=>drop(['/another/拖入.mp4']));await screen.findByText('拖入.mp4');expect(api.importPaths).toHaveBeenCalledWith(['/another/拖入.mp4']);
  });
  it('labels historical result only with an empty queue and does not restart it',async()=>{
    const {api}=fakeApi();vi.mocked(api.getCurrentJobSnapshot).mockResolvedValue({...snapshot('old','succeeded',8),progress:1,outputPath:'/old.mp4'});await ready(api);
    expect(await screen.findByText('上次结果')).toBeVisible();expect(screen.getByRole('button',{name:'开始处理'})).toBeDisabled();expect(api.startBatch).not.toHaveBeenCalled();
  });
  it('same-name paths remain distinguishable and folder failure keeps existing items',async()=>{
    const {api}=fakeApi([waiting('a','/one/video.mp4'),waiting('b','/two/video.mp4')]);vi.mocked(api.importFolder).mockRejectedValue({message:'所选文件夹当前层没有 MP4 文件'});const user=await ready(api);
    expect(screen.getByText('/one/video.mp4')).toBeVisible();expect(screen.getByText('/two/video.mp4')).toBeVisible();
    await user.click(screen.getByRole('button',{name:'选择文件夹'}));expect(await screen.findByText('所选文件夹当前层没有 MP4 文件')).toBeVisible();expect(screen.getAllByRole('listitem',{name:'video.mp4'})).toHaveLength(2);
  });
});
