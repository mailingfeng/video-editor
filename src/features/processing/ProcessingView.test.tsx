import {act,cleanup,fireEvent,render,screen,waitFor,within} from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import {afterEach,beforeAll,describe,expect,it,vi} from 'vitest';
import type {DesktopApi} from '../../api/desktop';
import type {JobSnapshot,MediaInfo,QueueItem,QueueSnapshot} from '../../api/contracts';
import {ProcessingView} from './ProcessingView';
afterEach(cleanup);
// jsdom does not implement the browser's native dialog lifecycle.
beforeAll(()=>{
  HTMLDialogElement.prototype.showModal=function(){this.open=true;};
  HTMLDialogElement.prototype.close=function(){this.open=false;};
});
const media:MediaInfo={identity:{canonicalPath:'/input/原 视频.mp4',sizeBytes:12345,modifiedNs:'123',sha256:'a'.repeat(64)},container:'mp4',title:null,comment:null,video:{streamIndex:0,codec:'h264',width:720,height:1280,bitDepth:8,pixelFormat:'yuv420p',frameRate:{num:30,den:1},timeBase:{num:1,den:15360},frameCount:60,startPts:0,durationTicks:30720,bitRate:null,colorRange:null,colorSpace:null,colorPrimaries:null,colorTransfer:null},audio:null};
const snapshot=(jobId='job-a',state:JobSnapshot['state']='running',version=2):JobSnapshot=>({jobId,state,version,progress:0.4,startedAtMs:123,endedAtMs:null,outputPath:null,error:null,cleanupPending:false});
const waiting=(itemId='a',inputPath=media.identity.canonicalPath):QueueItem=>({itemId,inputPath,state:'waiting',jobId:null,snapshot:null,media:null});
function deferred<T>(){let resolve!:(v:T)=>void;const promise=new Promise<T>(r=>{resolve=r;});return {promise,resolve};}
function fakeApi(initial:QueueItem[]=[]){
  let q:QueueSnapshot={version:1,running:false,items:initial};
  const listeners=new Set<(s:JobSnapshot)=>void>();const queues=new Set<(s:QueueSnapshot)=>void>();let drop:(paths:string[])=>void=()=>{};
  const api:DesktopApi={available:true,pickInput:vi.fn(async()=>media.identity.canonicalPath),pickInputFolder:vi.fn(async()=>'/input'),pickOutputDirectory:vi.fn(async()=>'/output'),probeInput:vi.fn(),cancelProbe:vi.fn(),startJob:vi.fn(),getJobSnapshot:vi.fn(),cancelJob:vi.fn(),
    getLicenseStatus:vi.fn(async()=>({expiresAtMs:1798732800000,effectiveTimeMs:1791630000000,expired:false,ntpAvailable:true})),
    listPresets:vi.fn(async()=>[{presetId:'basic-transcode-v1',version:1,title:'基础转换',evidenceStatus:'checked'}]),getCurrentJobSnapshot:vi.fn(async()=>null),
    getQueueSnapshot:vi.fn(async()=>q),importPaths:vi.fn(async(paths:string[])=>{q={...q,version:q.version+1,items:[...q.items,...paths.filter(path=>!q.items.some(i=>i.inputPath===path)).map((path,index)=>waiting(`added-${q.version}-${index}`,path))]};return q;}),
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
  it('immediately disables processing when the backend detects a newer expired clock',async()=>{
    const {api}=fakeApi([waiting('a')]);
    vi.mocked(api.startBatch).mockRejectedValue({code:'license_expired',message:'使用许可已于 2026-12-31 到期，请联系软件提供方更新许可。'});
    const user=await ready(api);
    await user.click(screen.getByRole('button',{name:'开始处理'}));
    await waitFor(()=>expect(screen.getByRole('button',{name:'开始处理'})).toBeDisabled());
    expect(screen.getAllByRole('alert').some(alert=>alert.textContent?.includes('到期'))).toBe(true);
  });
  it('disables processing and explains an expired license even with an output directory',async()=>{
    const {api}=fakeApi([waiting('a')]);
    Object.assign(api,{getLicenseStatus:vi.fn(async()=>({expiresAtMs:1798732800000,effectiveTimeMs:1798732800000,expired:true,ntpAvailable:true}))});
    const user=await ready(api);
    await waitFor(()=>expect(screen.getByRole('button',{name:'开始处理'})).toBeDisabled());
    expect(screen.getByRole('alert')).toHaveTextContent('使用许可已于 2026-12-31 到期');
    await user.hover(screen.getByRole('group',{name:'开始处理'}));
    expect(screen.getByRole('tooltip')).toHaveTextContent('请联系软件提供方更新许可');
    expect(api.startBatch).not.toHaveBeenCalled();
  });
  it('keeps processing disabled while license time is being checked',async()=>{
    const {api}=fakeApi([waiting('a')]);
    const pending=deferred<{expiresAtMs:number;effectiveTimeMs:number;expired:boolean;ntpAvailable:boolean}>();
    Object.assign(api,{getLicenseStatus:vi.fn(()=>pending.promise)});
    const user=await ready(api);
    expect(screen.getByRole('button',{name:'开始处理'})).toBeDisabled();
    await user.hover(screen.getByRole('group',{name:'开始处理'}));
    expect(screen.getByRole('tooltip')).toHaveTextContent('正在检查使用许可');
    await act(async()=>pending.resolve({expiresAtMs:1798732800000,effectiveTimeMs:1791630000000,expired:false,ntpAvailable:false}));
    await waitFor(()=>expect(screen.getByRole('button',{name:'开始处理'})).toBeEnabled());
    expect(screen.getByText(/已使用本机时间/)).toBeVisible();
  });
  it('does not guess the preset of a running batch restored after remount',async()=>{
    const item={...waiting('a'),state:'started' as const,jobId:'job-a',snapshot:snapshot()};
    const {api,emitQueue}=fakeApi([item]);
    emitQueue({version:2,running:true,items:[item]});
    render(<ProcessingView api={api}/>);
    const preset=await screen.findByRole('combobox',{name:'处理预设'});
    await waitFor(()=>expect(preset).toHaveValue(''));
    expect(preset).toBeDisabled();
    expect(screen.getByRole('option',{name:'本批设置已冻结，详见日志'})).toBeInTheDocument();
    expect(screen.getAllByText('批次设置已冻结')).toHaveLength(2);
  });
  it.each([
    ['sample-match-v1',1,'样本处理（实验）',/降低亮度、调整音频频谱/],
    ['sample-match-v2',2,'样本处理（相位实验）',/校准音频相位与延迟/],
    ['repeat-variant-v1',1,'重复处理（实验）',/每次加入不同的轻微画面扰动/],
    ['repeat-combined-v2',2,'组合变化（实验）',/几何、色调、细节与音频组合处理/],
    ['content-variation-v1',1,'内容变化（实验）',/声音变调、动态前景与模糊背景/],
  ] as const)('selects %s and freezes that preset during the batch',async(presetId,version,title,description)=>{
    const {api}=fakeApi([waiting('a')]);
    vi.mocked(api.listPresets).mockResolvedValue([
      {presetId:'basic-transcode-v1',version:1,title:'基础转换',evidenceStatus:'checked'},
      {presetId,version,title,evidenceStatus:'样本校准'},
    ]);
    const pending=deferred<QueueSnapshot>();
    vi.mocked(api.startBatch).mockReturnValue(pending.promise);
    const user=await ready(api);
    const preset=screen.getByRole('combobox',{name:'处理预设'});
    expect(preset).toHaveValue('basic-transcode-v1');
    await user.selectOptions(preset,presetId);
    expect(preset).toHaveValue(presetId);
    expect(screen.getByText(description)).toBeVisible();
    await user.click(screen.getByRole('button',{name:'开始处理'}));
    expect(api.startBatch).toHaveBeenCalledWith({outputDirectory:'/output',presetId,metadata:{mode:'preserve'}});
    expect(preset).toBeDisabled();
    fireEvent.change(preset,{target:{value:'basic-transcode-v1'}});
    expect(preset).toHaveValue(presetId);
    await act(async()=>pending.resolve({version:2,running:true,items:[{...waiting('a'),state:'started',jobId:'job-a',snapshot:snapshot()}]}));
    expect(preset).toBeDisabled();
  });
  it('explains the missing save location on hover and keyboard focus until one is selected',async()=>{
    const {api}=fakeApi([waiting('a')]);
    vi.mocked(api.pickOutputDirectory).mockResolvedValueOnce(null).mockResolvedValue('/output');
    render(<ProcessingView api={api}/>);const user=userEvent.setup();
    await screen.findByRole('listitem',{name:'原 视频.mp4'});
    const start=screen.getByRole('button',{name:'开始处理'});
    expect(start).toBeDisabled();
    fireEvent.click(start);expect(api.startBatch).not.toHaveBeenCalled();
    const hint=screen.getByRole('group',{name:'开始处理'});
    await user.hover(hint);
    expect(screen.getByRole('tooltip')).toHaveTextContent('请先选择保存位置，再开始处理视频。');
    await user.unhover(hint);expect(screen.queryByRole('tooltip')).not.toBeInTheDocument();
    act(()=>hint.focus());expect(screen.getByRole('tooltip')).toBeVisible();
    await user.keyboard('{Escape}');expect(screen.queryByRole('tooltip')).not.toBeInTheDocument();
    await user.click(screen.getByRole('button',{name:'选择输出目录'}));
    expect(start).toBeDisabled();
    await user.click(screen.getByRole('button',{name:'选择输出目录'}));
    await waitFor(()=>expect(start).toBeEnabled());
    await user.hover(hint);expect(screen.queryByRole('tooltip')).not.toBeInTheDocument();
    await user.click(start);
    expect(api.startBatch).toHaveBeenCalledWith({outputDirectory:'/output',presetId:'basic-transcode-v1',metadata:{mode:'preserve'}});
  });
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
    expect(screen.queryByRole('checkbox',{name:'自定义标题与备注'})).not.toBeInTheDocument();
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
    await user.click(within(row('b.mp4')).getByRole('button',{name:'查看 b.mp4 的日志'}));
    const dialog=screen.getByRole('dialog',{name:'查看日志'});
    expect(within(dialog).getByText('帧数校验失败')).toBeVisible();
    expect(await within(dialog).findByText('log job-b')).toBeVisible();
  });
  it('shows media details after check without claiming complete color information',async()=>{
    const {api}=fakeApi([{...waiting('a'),media:{...media,video:{...media.video,colorRange:'pc'}}}]);const user=await ready(api);
    await user.click(within(row('原 视频.mp4')).getByRole('button',{name:'查看 原 视频.mp4 的视频信息'}));
    const dialog=screen.getByRole('dialog',{name:'视频信息'});
    expect(within(dialog).getByText(/部分颜色信息缺失/)).toBeVisible();expect(within(dialog).getByText(/范围 pc/)).toBeVisible();
  });
  it('start stays single while pending and preserves each video metadata without custom fields',async()=>{
    const {api}=fakeApi([waiting('a')]);const pending=deferred<QueueSnapshot>();vi.mocked(api.startBatch).mockReturnValue(pending.promise);const user=await ready(api);
    expect(screen.queryByRole('checkbox',{name:'自定义标题与备注'})).not.toBeInTheDocument();
    expect(screen.queryByLabelText('标题')).not.toBeInTheDocument();
    expect(screen.queryByLabelText('备注')).not.toBeInTheDocument();
    const start=screen.getByRole('button',{name:'开始处理'});fireEvent.click(start);fireEvent.click(start);
    expect(api.startBatch).toHaveBeenCalledTimes(1);expect(api.startBatch).toHaveBeenCalledWith(expect.objectContaining({metadata:{mode:'preserve'}}));
    await act(async()=>pending.resolve({version:2,running:true,items:[{...waiting('a'),state:'started',jobId:'job-a',snapshot:snapshot()}]}));
  });
  it('preserves single file choosing and drag import',async()=>{
    const {api,drop}=fakeApi();const user=await ready(api);await user.click(screen.getByRole('button',{name:'选择视频'}));await screen.findByRole('listitem',{name:'原 视频.mp4'});expect(api.importPaths).toHaveBeenCalledWith([media.identity.canonicalPath]);
    await act(async()=>drop(['/another/拖入.mp4']));await screen.findByText('拖入.mp4');expect(api.importPaths).toHaveBeenCalledWith(['/another/拖入.mp4']);
  });
  it('labels historical result only with an empty queue and does not restart it',async()=>{
    const {api}=fakeApi();vi.mocked(api.getCurrentJobSnapshot).mockResolvedValue({...snapshot('old','succeeded',8),progress:1,outputPath:'/old.mp4'});await ready(api);
    await waitFor(()=>expect(screen.getByRole('button',{name:'开始处理'})).toBeDisabled());expect(api.startBatch).not.toHaveBeenCalled();
    await userEvent.click(screen.getByRole('button',{name:'日志与报告'}));
    expect(await screen.findByText('上次结果')).toBeVisible();
  });
  it('same-name paths remain distinguishable and folder failure keeps existing items',async()=>{
    const {api}=fakeApi([waiting('a','/one/video.mp4'),waiting('b','/two/video.mp4')]);vi.mocked(api.importFolder).mockRejectedValue({message:'所选文件夹当前层没有 MP4 文件'});const user=await ready(api);
    expect(screen.getByText('/one/video.mp4')).toBeVisible();expect(screen.getByText('/two/video.mp4')).toBeVisible();
    await user.click(screen.getByRole('button',{name:'选择文件夹'}));expect(await screen.findByText('所选文件夹当前层没有 MP4 文件')).toBeVisible();expect(screen.getAllByRole('listitem',{name:'video.mp4'})).toHaveLength(2);
  });
  it('opens the clicked file source information and keeps the other file out of the dialog',async()=>{
    const other={...media,identity:{...media.identity,canonicalPath:'/input/b.mp4'},title:'第二个视频',video:{...media.video,width:1920,height:1080}};
    const {api}=fakeApi([{...waiting('a'),media},{...waiting('b','/input/b.mp4'),media:other}]);const user=await ready(api);
    await user.click(within(row('b.mp4')).getByRole('button',{name:'查看 b.mp4 的视频信息'}));
    const dialog=screen.getByRole('dialog',{name:'视频信息'});
    expect(within(dialog).getByText('1920 × 1080')).toBeVisible();
    expect(within(dialog).getByText('第二个视频')).toBeVisible();
    expect(within(dialog).queryByText('720 × 1280')).not.toBeInTheDocument();
    await user.click(within(dialog).getByRole('button',{name:'关闭'}));
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });
  it('does not replace another file log with a response from a closed dialog',async()=>{
    const {api}=fakeApi(['a','b'].map(id=>({...waiting(id,`/input/${id}.mp4`),state:'started' as const,jobId:`job-${id}`,snapshot:snapshot(`job-${id}`)})));
    const pending=deferred<{text:string;truncated:boolean}>();
    vi.mocked(api.getJobLog).mockImplementation(async id=>id==='job-a'?pending.promise:{text:'转换记录 b',truncated:false});
    const user=await ready(api);
    await user.click(within(row('a.mp4')).getByRole('button',{name:'查看 a.mp4 的日志'}));
    expect(within(screen.getByRole('dialog')).getByText('正在读取日志…')).toBeVisible();
    await user.click(within(screen.getByRole('dialog')).getByRole('button',{name:'关闭'}));
    await user.click(within(row('b.mp4')).getByRole('button',{name:'查看 b.mp4 的日志'}));
    expect(await within(screen.getByRole('dialog')).findByText('转换记录 b')).toBeVisible();
    await act(async()=>pending.resolve({text:'迟到的记录 a',truncated:false}));
    expect(within(screen.getByRole('dialog')).queryByText('迟到的记录 a')).not.toBeInTheDocument();
    expect(within(screen.getByRole('dialog')).getByText('转换记录 b')).toBeVisible();
  });
  it('explains unavailable source details and logs for a waiting file',async()=>{
    const {api}=fakeApi([waiting('a')]);const user=await ready(api);
    await user.click(within(row('原 视频.mp4')).getByRole('button',{name:'查看 原 视频.mp4 的视频信息'}));
    expect(within(screen.getByRole('dialog')).getByText(/待检查/)).toBeVisible();
    await user.click(within(screen.getByRole('dialog')).getByRole('button',{name:'关闭'}));
    await user.click(within(row('原 视频.mp4')).getByRole('button',{name:'查看 原 视频.mp4 的日志'}));
    expect(within(screen.getByRole('dialog')).getByText('尚未开始处理，暂无日志。')).toBeVisible();
    expect(within(screen.getByRole('dialog')).getByRole('button',{name:'刷新日志'})).toBeDisabled();
  });
  it('shows a log read error inside the file dialog and allows refreshing it',async()=>{
    const {api}=fakeApi([{...waiting('a'),state:'started',jobId:'job-a',snapshot:snapshot()}]);
    vi.mocked(api.getJobLog).mockRejectedValueOnce({message:'日志暂时不可读'}).mockResolvedValue({text:'恢复后的记录',truncated:true});
    const user=await ready(api);await user.click(within(row('原 视频.mp4')).getByRole('button',{name:'查看 原 视频.mp4 的日志'}));
    const dialog=screen.getByRole('dialog',{name:'查看日志'});
    expect(await within(dialog).findByRole('alert')).toHaveTextContent('日志暂时不可读');
    await user.click(within(dialog).getByRole('button',{name:'刷新日志'}));
    expect(await within(dialog).findByText('恢复后的记录')).toBeVisible();
    expect(within(dialog).queryByText('日志暂时不可读')).not.toBeInTheDocument();
    expect(within(dialog).getByText('仅显示最近 64 KiB 日志。')).toBeVisible();
  });
});
