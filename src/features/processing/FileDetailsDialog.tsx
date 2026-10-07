import {useEffect,useId,useRef} from 'react';
import type {LogExcerpt,QueueItem} from '../../api/contracts';

interface FileDetailsDialogProps {
  kind:'media'|'logs';
  item:QueueItem;
  status:string;
  log:LogExcerpt|null;
  logLoading:boolean;
  logError:string|null;
  onClose:()=>void;
  onRefresh:()=>void;
}

export function FileDetailsDialog({kind,item,status,log,logLoading,logError,onClose,onRefresh}:FileDetailsDialogProps) {
  const dialog=useRef<HTMLDialogElement>(null);
  const titleId=useId();
  const pathId=useId();
  const media=item.media;
  const video=media?.video;
  const duration=video?video.durationTicks*video.timeBase.num/video.timeBase.den:0;
  const colors=video?[video.colorRange,video.colorSpace,video.colorPrimaries,video.colorTransfer]:[];
  const colorSummary=video && colors.some(Boolean)
    ? `范围 ${video.colorRange??'未知'} / 矩阵 ${video.colorSpace??'未知'} / 原色 ${video.colorPrimaries??'未知'} / 传递 ${video.colorTransfer??'未知'}${colors.some(value=>!value)?'（部分颜色信息缺失）':''}`
    : '颜色信息未提供';

  useEffect(()=>{
    const node=dialog.current;
    node?.showModal();
    return ()=>node?.close();
  },[]);

  function close(){dialog.current?.close();onClose();}

  return <dialog ref={dialog} className={`file-dialog ${kind==='logs'?'file-dialog-logs':''}`} aria-labelledby={titleId} aria-describedby={pathId} onCancel={event=>{event.preventDefault();close();}}>
    <div className="panel-head dialog-head"><h2 id={titleId}>{kind==='media'?'视频信息':'查看日志'}</h2><div className="dialog-actions">
      {kind==='logs' && <button className="btn" disabled={!item.jobId || logLoading} onClick={onRefresh}>{logLoading?'正在读取':'刷新日志'}</button>}
      <button className="btn quiet" onClick={close}>关闭</button>
    </div></div>
    <div className="dialog-file"><h3>{item.inputPath.split(/[\\/]/).pop()||item.inputPath}</h3><p id={pathId} className="small muted">{item.inputPath}</p></div>
    {kind==='media'?<div className="panel-body source-info">
      {media && video?<>
        <dl className="media-grid">
          <div><dt>文件大小</dt><dd>{(media.identity.sizeBytes/1024/1024).toFixed(1)} MB</dd></div>
          <div><dt>时长 / 容器</dt><dd>{duration.toFixed(2)} 秒 · {media.container}</dd></div>
          <div><dt>画面</dt><dd>{video.width} × {video.height}</dd></div>
          <div><dt>帧率 / 帧数</dt><dd>{(video.frameRate.num/video.frameRate.den).toFixed(2)} fps / {video.frameCount}</dd></div>
          <div><dt>视频编码</dt><dd>{video.codec} · {video.pixelFormat} · {video.bitDepth} bit</dd></div>
          <div><dt>音频</dt><dd>{media.audio?`${media.audio.codec} · ${media.audio.sampleRate/1000} kHz · ${media.audio.channels===1?'单声道':media.audio.channels===2?'立体声':`${media.audio.channels} 声道`}`:'无音轨'}</dd></div>
          <div><dt>标题</dt><dd>{media.title||'未提供'}</dd></div>
          <div><dt>备注</dt><dd>{media.comment||'未提供'}</dd></div>
        </dl><p className="helper">{colorSummary}</p>
      </>:<p className="helper">{item.snapshot?.state==='failed'?'未读取到视频源信息，请查看日志了解详情。':'待检查 · 完整媒体检查在任务开始后进行。'}</p>}
    </div>:<>
      <div className="report-summary"><div><span>状态</span><strong>{status}</strong></div><div><span>结果路径</span><strong>{item.snapshot?.outputPath??'—'}</strong></div></div>
      {item.snapshot?.error && <div className="error-detail dialog-notice" role="alert"><strong>{item.snapshot.error.message}</strong>{item.snapshot.error.details && <p>{item.snapshot.error.details}</p>}<small>{item.snapshot.error.code}</small></div>}
      {item.snapshot?.cleanupPending && <div className="notice warning dialog-notice">临时文件尚未完全清理，应用下次启动时会重试。</div>}
      {logError && <div className="notice error dialog-notice" role="alert">{logError}</div>}
      <pre className="log-body" aria-busy={logLoading}>{log?.text||(logLoading?'正在读取日志…':item.jobId?'暂无日志，可稍后刷新。':'尚未开始处理，暂无日志。')}</pre>
      {log?.truncated && <p className="helper log-footer">仅显示最近 64 KiB 日志。</p>}
    </>}
  </dialog>;
}
