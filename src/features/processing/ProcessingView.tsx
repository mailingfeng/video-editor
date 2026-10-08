import { useState } from 'react';
import type { DesktopApi } from '../../api/desktop';
import type { JobState, QueueItem } from '../../api/contracts';
import { useProcessing } from './useProcessing';
import { FileDetailsDialog } from './FileDetailsDialog';
import './processing.css';

const labels: Record<JobState, string> = {probing:'检查视频', preparing:'准备任务', running:'处理中', validating:'校验结果', committing:'保存结果', succeeded:'处理完成', failed:'处理失败', canceling:'正在取消', canceled:'已取消'};
function Icon({name}: {name:'film'|'folder'|'report'|'upload'|'check'|'play'}) {
  const paths = {film:<><rect x="3" y="3" width="18" height="18" rx="2"/><path d="M7 3v18M17 3v18M3 7h4M3 12h4M3 17h4M17 7h4M17 12h4M17 17h4M10 9l5 3-5 3z"/></>, folder:<path d="M3 7V5h6l2 2h10v13H3zM3 10h18"/>, report:<path d="M14 3H5v18h14V8zM14 3v5h5M8 12h8M8 16h5"/>, upload:<><path d="M12 16V4m-5 5 5-5 5 5M4 16v5h16v-5"/></>, check:<path d="m5 12 4 4 10-10"/>, play:<path d="m8 5 11 7-11 7z"/>};
  return <svg className="icon" viewBox="0 0 24 24" aria-hidden="true">{paths[name]}</svg>;
}
export function ProcessingView({api}: {api: DesktopApi}) {
  const p = useProcessing(api);
  const [tab, setTab] = useState<'processing'|'logs'>('processing');
  const [override, setOverride] = useState(false);
  const [title, setTitle] = useState('');
  const [comment, setComment] = useState('');
  const [details, setDetails] = useState<{itemId:string;kind:'media'|'logs'}|null>(null);
  const detailsItem = p.queue.items.find(item => item.itemId === details?.itemId);
  const snap = p.snapshot;
  const previousResult = p.previousResult;
  const percent = (item:QueueItem) => item.snapshot?.progress == null ? null : item.snapshot.state === 'succeeded' ? 100 : Math.min(99, Math.floor(item.snapshot.progress * 100));
  const status = (item:QueueItem) => item.snapshot ? labels[item.snapshot.state] : item.state === 'canceled' ? '已取消' : '待处理';
  const canCancel = (item:QueueItem) => item.state === 'waiting' || Boolean(item.snapshot && !['succeeded','failed','canceled','canceling','committing'].includes(item.snapshot.state));
  function openDetails(itemId:string,kind:'media'|'logs') {
    p.selectItem(itemId);
    setDetails({itemId,kind});
    if(kind==='logs')void p.refreshLog();
  }
  return <div className="app-shell">
    <header className="topbar">
      <div className="brand"><span className="brand-mark"><Icon name="film"/></span><strong>帧序</strong></div>
      <nav aria-label="主导航">
        <button className={`nav-button ${tab === 'processing' ? 'active' : ''}`} aria-current={tab === 'processing' ? 'page' : undefined} onClick={() => setTab('processing')}><Icon name="film"/>视频处理</button>
        <button className={`nav-button ${tab === 'logs' ? 'active' : ''}`} aria-current={tab === 'logs' ? 'page' : undefined} onClick={() => {setTab('logs'); void p.refreshLog();}}><Icon name="report"/>日志与报告</button>
      </nav>
      <span className="app-label">本地视频工作台</span>
    </header>
    <main className="content">
      {tab === 'logs' && <div className="page-heading"><div><h1>日志与报告</h1><p className="subtitle">查看选中文件的处理记录与结果。</p></div><span className={`ready ${p.busy ? 'active' : ''}`}><i/>{p.busy ? '任务进行中' : '准备就绪'}</span></div>}
      {tab === 'processing' && <div className="launch-bar"><div className="toolbar-output"><span className="muted">输出目录</span><button className="btn output-selector" disabled={!api.available || p.busy} onClick={() => void p.selectOutput()} aria-label="选择输出目录" title={p.outputDirectory}><Icon name="folder"/><span>{p.outputDirectory || '选择保存位置'}</span></button></div><span className="toolbar-preset muted">基础转换 v1</span><span className={`ready ${p.busy ? 'active' : ''}`} role="status"><i/>{p.busy ? '任务进行中' : '准备就绪'}</span><button className="btn primary" disabled={!api.available || p.busy || p.importing || !p.queue.items.some(item => item.state === 'waiting') || !p.outputDirectory || p.presets.length === 0} onClick={() => void p.start(override ? {mode:'override', title:title || null, comment:comment || null} : {mode:'preserve'})}><Icon name="play"/>{p.starting ? '正在启动' : '开始处理'}</button></div>}
      {!api.available && <div className="notice">请使用桌面应用选择本地视频。此页面仅展示界面。</div>}
      {p.error && <div className="notice error" role="alert">{p.error}</div>}
      {tab === 'processing' ? <div className="workspace">
        <div className="configuration">
          <section className="panel input-panel"><div className="panel-head"><div><h2>待处理文件</h2><small>当前层视频 · 按文件独立校验</small></div><span className="badge">{p.queue.items.length} 个文件 · 并发 2</span></div>
            <div className="import-toolbar"><button className="btn" disabled={!api.available || p.busy || p.importing} onClick={() => void p.selectInput()}><Icon name="film"/>选择视频</button><button className="btn primary" disabled={!api.available || p.busy || p.importing} onClick={() => void p.selectFolder()}><Icon name="folder"/>{p.importing ? '正在导入' : '选择文件夹'}</button></div>
            {p.queue.items.length ? <ul className="file-queue" aria-label="待处理文件列表">{p.queue.items.map(item => {
              const fileName = item.inputPath.split(/[\\/]/).pop() || item.inputPath;
              const value = percent(item);
              return <li key={item.itemId} className={`queue-row ${item.itemId === p.selectedItem?.itemId ? 'selected' : ''}`} aria-label={fileName}>
                <button className="queue-select" aria-label={`查看 ${fileName}`} aria-pressed={item.itemId === p.selectedItem?.itemId} onClick={() => p.selectItem(item.itemId)}><span className="file-symbol"><Icon name="film"/></span><span className="queue-name"><strong>{fileName}</strong><span className="queue-path" title={item.inputPath}>{item.inputPath}</span></span></button>
                <div className="queue-status"><span className={`phase phase-${item.snapshot?.state ?? item.state}`}>{status(item)}</span>{value !== null && <strong>{value}%</strong>}</div>
                <div className="progress-track" role="progressbar" aria-label={`${fileName} 处理进度`} aria-valuemin={0} aria-valuemax={100} aria-valuenow={value ?? undefined} aria-valuetext={status(item)}><span style={{transform:`scaleX(${value === null ? 0 : value / 100})`}}/></div>
                <div className="queue-actions">{canCancel(item) && <button className="btn quiet" aria-label={`取消 ${fileName}`} onClick={() => void p.cancelItem(item.itemId)}>取消</button>}{item.snapshot?.state === 'succeeded' && <button className="btn quiet" aria-label={`显示 ${fileName} 的结果`} onClick={() => void p.reveal(item.itemId)}><Icon name="folder"/>显示结果</button>}<button className="btn quiet" aria-label={`查看 ${fileName} 的视频信息`} onClick={() => openDetails(item.itemId,'media')}><Icon name="film"/>视频信息</button><button className="btn quiet" aria-label={`查看 ${fileName} 的日志`} onClick={() => openDetails(item.itemId,'logs')}><Icon name="report"/>查看日志</button><button className="btn quiet" aria-label={`移除 ${fileName}`} disabled={p.busy || p.importing} onClick={() => void p.removeItem(item.itemId)}>移除</button></div>
              </li>;
            })}</ul> : <div className="empty"><Icon name="upload"/><h3>添加需要处理的视频</h3><p>支持 MP4、MOV、M4V、MKV、WebM，也可选择文件夹批量添加。</p></div>}
          </section>
          <section className="panel settings"><div className="panel-head"><h2>处理设置</h2><span className="badge">基础转换 v1</span></div><div className="panel-body">
            <label className="field"><span>处理预设</span><select value={p.presets[0]?.presetId ?? ''} disabled aria-label="处理预设"><option value={p.presets[0]?.presetId ?? ''}>基础转换</option></select></label><p className="helper preset-description">输出 H.264 / MP4，保留画面尺寸、原始帧与时间轴，支持可变帧率；音频转为 AAC 48 kHz，保留 1–8 声道。</p>
            <p className="helper">本批共用以上设置。默认分别保留每个视频的标题与备注。自动生成新文件，不覆盖已有文件。</p>
            <label className="checkbox"><input type="checkbox" checked={override} disabled={p.busy} onChange={(e) => setOverride(e.target.checked)}/>自定义标题与备注</label>
            {override && <div className="metadata-fields"><label className="field"><span>标题</span><input value={title} maxLength={2000} disabled={p.busy} onChange={(e) => setTitle(e.target.value)}/></label><label className="field"><span>备注</span><textarea value={comment} maxLength={8000} disabled={p.busy} onChange={(e) => setComment(e.target.value)}/></label></div>}
          </div></section>
        </div>
      </div> : <section className="panel"><div className="panel-head"><div><h2>任务记录</h2><small>{snap ? `任务 ${snap.jobId}` : '尚无任务记录'}</small></div><button className="btn" disabled={!snap || p.logLoading} onClick={() => void p.refreshLog()}>刷新日志</button></div>{previousResult && <div className="notice">上次结果</div>}<div className="report-summary"><div><span>状态</span><strong>{snap ? labels[snap.state] : p.selectedItem?.state === 'canceled' ? '已取消' : '等待开始'}</strong></div><div><span>结果路径</span><strong>{snap?.outputPath ?? '—'}</strong></div></div>{p.logError && <div className="notice error" role="alert">{p.logError}</div>}<pre className="log-body" aria-busy={p.logLoading}>{p.log?.text || (p.logLoading ? '正在读取日志…' : '执行任务后，此处显示媒体工具、转换参数与校验记录。')}</pre>{p.log?.truncated && <p className="helper log-footer">仅显示最近 64 KiB 日志。</p>}</section>}
    </main>
    {details && detailsItem && <FileDetailsDialog key={`${details.itemId}-${details.kind}`} kind={details.kind} item={detailsItem} status={status(detailsItem)} log={p.log} logLoading={p.logLoading} logError={p.logError} onClose={() => setDetails(null)} onRefresh={() => void p.refreshLog()}/>}
    <footer className="app-footer"><span>帧序 · 基础转换</span><span>本地处理 · 独立保存 · 完整校验</span></footer>
  </div>;
}
