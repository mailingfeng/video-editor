import { useState } from 'react';
import type { DesktopApi } from '../../api/desktop';
import type { JobState, QueueItem } from '../../api/contracts';
import { useProcessing } from './useProcessing';
import './processing.css';

const labels: Record<JobState, string> = {probing:'检查视频', preparing:'准备任务', running:'处理中', validating:'校验结果', committing:'保存结果', succeeded:'处理完成', failed:'处理失败', canceling:'正在取消', canceled:'已取消'};
const stages = ['检查', '转换', '校验', '保存'];
const stageIndex: Record<JobState, number> = {probing:0, preparing:0, running:1, validating:2, committing:3, succeeded:4, failed:-1, canceling:-1, canceled:-1};
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
  const video = p.media?.video;
  const snap = p.snapshot;
  const previousResult = p.previousResult;
  const name = p.selectedItem?.inputPath.split(/[\\/]/).pop();
  const duration = video ? video.durationTicks * video.timeBase.num / video.timeBase.den : 0;
  const activeStage = snap ? stageIndex[snap.state] : -1;
  const colorFields = video ? [video.colorRange, video.colorSpace, video.colorPrimaries, video.colorTransfer] : [];
  const colorSummary = video && colorFields.some(Boolean)
    ? `范围 ${video.colorRange ?? '未知'} / 矩阵 ${video.colorSpace ?? '未知'} / 原色 ${video.colorPrimaries ?? '未知'} / 传递 ${video.colorTransfer ?? '未知'}${colorFields.some((value) => !value) ? '（部分颜色信息缺失）' : ''}`
    : '颜色信息未提供';
  const percent = (item:QueueItem) => item.snapshot?.progress == null ? null : item.snapshot.state === 'succeeded' ? 100 : Math.min(99, Math.floor(item.snapshot.progress * 100));
  const status = (item:QueueItem) => item.snapshot ? labels[item.snapshot.state] : item.state === 'canceled' ? '已取消' : '待处理';
  const canCancel = (item:QueueItem) => item.state === 'waiting' || Boolean(item.snapshot && !['succeeded','failed','canceled','canceling','committing'].includes(item.snapshot.state));
  const progress = snap?.progress == null ? null : snap.state === 'succeeded' ? 100 : Math.min(99,Math.floor(snap.progress*100));
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
      <div className="page-heading"><div><h1>{tab === 'processing' ? '视频处理' : '日志与报告'}</h1><p className="subtitle">{tab === 'processing' ? '选择视频或文件夹，同时处理两个文件并分别校验保存。' : '查看选中文件的处理记录与结果。'}</p></div><span className={`ready ${p.busy ? 'active' : ''}`}><i/>{p.busy ? '任务进行中' : '准备就绪'}</span></div>
      {tab === 'processing' && <div className="launch-bar"><div className="toolbar-output"><span className="muted">输出目录</span><button className="btn output-selector" disabled={!api.available || p.busy} onClick={() => void p.selectOutput()} aria-label="选择输出目录" title={p.outputDirectory}><Icon name="folder"/><span>{p.outputDirectory || '选择保存位置'}</span></button></div><span className="toolbar-preset muted">基础转换 v1</span><button className="btn primary" disabled={!api.available || p.busy || p.importing || !p.queue.items.some(item => item.state === 'waiting') || !p.outputDirectory || p.presets.length === 0} onClick={() => void p.start(override ? {mode:'override', title:title || null, comment:comment || null} : {mode:'preserve'})}><Icon name="play"/>{p.starting ? '正在启动' : '开始处理'}</button></div>}
      {!api.available && <div className="notice">请使用桌面应用选择本地视频。此页面仅展示界面。</div>}
      {p.error && <div className="notice error" role="alert">{p.error}</div>}
      {tab === 'processing' ? <div className="workspace">
        <div className="configuration">
          <section className="panel input-panel"><div className="panel-head"><div><h2>待处理文件</h2><small>当前层 MP4 · 按文件独立校验</small></div><span className="badge">{p.queue.items.length} 个文件 · 并发 2</span></div>
            <div className="import-toolbar"><button className="btn" disabled={!api.available || p.busy || p.importing} onClick={() => void p.selectInput()}><Icon name="film"/>选择视频</button><button className="btn primary" disabled={!api.available || p.busy || p.importing} onClick={() => void p.selectFolder()}><Icon name="folder"/>{p.importing ? '正在导入' : '选择文件夹'}</button></div>
            {p.queue.items.length ? <ul className="file-queue" aria-label="待处理文件列表">{p.queue.items.map(item => {
              const fileName = item.inputPath.split(/[\\/]/).pop() || item.inputPath;
              const value = percent(item);
              return <li key={item.itemId} className={`queue-row ${item.itemId === p.selectedItem?.itemId ? 'selected' : ''}`} aria-label={fileName}>
                <button className="queue-select" aria-label={`查看 ${fileName}`} aria-pressed={item.itemId === p.selectedItem?.itemId} onClick={() => p.selectItem(item.itemId)}><span className="file-symbol"><Icon name="film"/></span><span className="queue-name"><strong>{fileName}</strong><span className="queue-path" title={item.inputPath}>{item.inputPath}</span></span></button>
                <div className="queue-status"><span className={`phase phase-${item.snapshot?.state ?? item.state}`}>{status(item)}</span>{value !== null && <strong>{value}%</strong>}</div>
                <div className="progress-track" role="progressbar" aria-label={`${fileName} 处理进度`} aria-valuemin={0} aria-valuemax={100} aria-valuenow={value ?? undefined} aria-valuetext={status(item)}><span style={{transform:`scaleX(${value === null ? 0 : value / 100})`}}/></div>
                <div className="queue-actions">{canCancel(item) && <button className="btn quiet" aria-label={`取消 ${fileName}`} onClick={() => void p.cancelItem(item.itemId)}>取消</button>}{item.snapshot?.state === 'succeeded' && <button className="btn quiet" aria-label={`显示 ${fileName} 的结果`} onClick={() => void p.reveal(item.itemId)}><Icon name="folder"/>显示结果</button>}<button className="btn quiet" aria-label={`移除 ${fileName}`} disabled={p.busy || p.importing} onClick={() => void p.removeItem(item.itemId)}>移除</button></div>
              </li>;
            })}</ul> : <div className="empty"><Icon name="upload"/><h3>添加需要处理的视频</h3><p>拖入 MP4 文件，或选择文件夹批量添加。</p></div>}
            {p.selectedItem && <div className="source-details selected-details"><div className="file-heading"><div><h3 title={p.selectedItem.inputPath}>{name}</h3><p className="small muted">{video && p.media ? `${(p.media.identity.sizeBytes/1024/1024).toFixed(1)} MB · ${duration.toFixed(2)} 秒` : '待检查 · 完整媒体检查在任务开始后进行'}</p></div>{video && <span className="valid"><Icon name="check"/>已检查</span>}</div>
              {p.media && video && <><dl className="media-grid"><div><dt>画面</dt><dd>{video.width} × {video.height}</dd></div><div><dt>帧率 / 帧数</dt><dd>{(video.frameRate.num/video.frameRate.den).toFixed(2)} fps / {video.frameCount}</dd></div><div><dt>视频编码</dt><dd>{video.codec} · {video.pixelFormat}</dd></div><div><dt>音频</dt><dd>{p.media.audio ? `${p.media.audio.codec} · ${p.media.audio.sampleRate / 1000} kHz · ${p.media.audio.channels === 1 ? '单声道' : '立体声'}` : '无音轨'}</dd></div></dl><p className="helper">{colorSummary}</p></>}
            </div>}
          </section>
          <section className="panel settings"><div className="panel-head"><h2>处理设置</h2><span className="badge">基础转换 v1</span></div><div className="panel-body">
            <label className="field"><span>处理预设</span><select value={p.presets[0]?.presetId ?? ''} disabled aria-label="处理预设"><option value={p.presets[0]?.presetId ?? ''}>基础转换</option></select></label><p className="helper preset-description">H.264 / MP4，保留画面尺寸与帧率；有音轨时转为 AAC 48 kHz。</p>
            <p className="helper">本批共用以上设置。默认分别保留每个视频的标题与备注。自动生成新文件，不覆盖已有文件。</p>
            <label className="checkbox"><input type="checkbox" checked={override} disabled={p.busy} onChange={(e) => setOverride(e.target.checked)}/>自定义标题与备注</label>
            {override && <div className="metadata-fields"><label className="field"><span>标题</span><input value={title} maxLength={2000} disabled={p.busy} onChange={(e) => setTitle(e.target.value)}/></label><label className="field"><span>备注</span><textarea value={comment} maxLength={8000} disabled={p.busy} onChange={(e) => setComment(e.target.value)}/></label></div>}
          </div></section>
        </div>
        <aside className="panel job-panel" aria-label="任务进度"><div className="panel-head"><h2>处理进度</h2><span className="badge">完整校验</span></div><div className="panel-body">
          {previousResult && <div className="notice">上次结果</div>}
          <div className={`job-status ${snap?.state === 'succeeded' ? 'success' : snap?.state === 'failed' ? 'failure' : ''}`} aria-live="polite"><span className="job-symbol"><Icon name={snap?.state === 'succeeded' ? 'check' : 'film'}/></span><h2>{snap ? labels[snap.state] : p.selectedItem?.state === 'canceled' ? '已取消' : '等待开始'}</h2><p>{snap?.state === 'succeeded' ? '规格与完整解码校验通过，结果已保存。' : snap?.state === 'validating' ? '检查帧数、时长、音画同步与完整解码。' : snap?.state === 'committing' ? '正在安全保存已验证的结果。' : snap?.state === 'canceled' ? '任务已停止，临时文件已执行清理。' : snap?.state === 'failed' ? '未生成可用结果，请查看错误和日志。' : '处理过程在本地运行，原视频保持不变。'}</p></div>
          <div className="progress-label"><span>任务进度</span><strong>{progress !== null ? `${progress}%` : '—'}</strong></div><div className="progress-track" role="progressbar" aria-label={`${name ?? '选中文件'} 详情进度`} aria-valuemin={0} aria-valuemax={100} aria-valuenow={progress ?? undefined}><span style={{transform:`scaleX(${(progress ?? 0) / 100})`}}/></div>
          <ol className="stages">{stages.map((stage, index) => <li key={stage} className={activeStage > index ? 'done' : activeStage === index ? 'current' : ''}><span>{activeStage > index ? <Icon name="check"/> : index + 1}</span>{stage}</li>)}</ol>
          {snap?.error && <div className="error-detail" role="alert"><strong>{snap.error.message}</strong><p>{snap.error.details}</p><small>{snap.error.code}</small></div>}
          {snap?.cleanupPending && <div className="notice warning">临时文件尚未完全清理，应用下次启动时会重试。</div>}
          {snap?.state === 'succeeded' && snap.outputPath && <div className="result"><span className="small muted">已保存到</span><p>{snap.outputPath}</p><button className="btn primary full" onClick={() => void p.reveal()}><Icon name="folder"/>在文件夹中显示</button></div>}
          <div className="task-note"><h3>结果校验</h3><p>保持尺寸、帧率、帧数和时长。完成前会检查编码、音频规格与完整解码。</p></div>
          {snap && <button className="btn quiet full" onClick={() => {setTab('logs'); void p.refreshLog();}}><Icon name="report"/>查看任务日志</button>}
        </div></aside>
      </div> : <section className="panel"><div className="panel-head"><div><h2>任务记录</h2><small>{snap ? `任务 ${snap.jobId}` : '尚无任务记录'}</small></div><button className="btn" disabled={!snap} onClick={() => void p.refreshLog()}>刷新日志</button></div><div className="report-summary"><div><span>状态</span><strong>{snap ? labels[snap.state] : p.selectedItem?.state === 'canceled' ? '已取消' : '等待开始'}</strong></div><div><span>结果路径</span><strong>{snap?.outputPath ?? '—'}</strong></div></div><pre className="log-body">{p.log?.text || '执行任务后，此处显示媒体工具、转换参数与校验记录。'}</pre>{p.log?.truncated && <p className="helper log-footer">仅显示最近 64 KiB 日志。</p>}</section>}
    </main>
    <footer className="app-footer"><span>帧序 · 基础转换</span><span>本地处理 · 独立保存 · 完整校验</span></footer>
  </div>;
}
