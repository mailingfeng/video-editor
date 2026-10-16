import {useEffect,useId,useRef} from 'react';
import type {CustomLevelKey,CustomVariationSettings} from '../../api/contracts';

type Setting={key:CustomLevelKey;title:string;description:string;needsForeground?:boolean};
const groups:{title:string;settings:Setting[]}[]=[
 {title:'布局',settings:[
  {key:'foregroundScale',title:'前景缩放',description:'保留完整画面，调整前景与同源背景的布局。'},
  {key:'backgroundBlur',title:'背景模糊',description:'柔化前景周围的同源背景。',needsForeground:true},
  {key:'dynamicOffset',title:'动态位移',description:'让前景在背景范围内缓慢移动。',needsForeground:true},
  {key:'cropScale',title:'裁切缩放',description:'轻微裁切后恢复画面尺寸，可能裁掉边缘字幕。'},
 ]},
 {title:'画面',settings:[
  {key:'hue',title:'色相变化',description:'让画面色相随播放缓慢变化。'},
  {key:'saturation',title:'饱和度变化',description:'调整主画面的色彩浓度。'},
  {key:'tone',title:'明暗对比',description:'微调主画面的亮度与对比度。'},
  {key:'noise',title:'颗粒噪声',description:'为画面增加细小颗粒。'},
  {key:'softness',title:'画面柔化',description:'柔化主画面细节。'},
 ]},
 {title:'声音',settings:[
  {key:'pitch',title:'变调',description:'改变音高，独立于视频播放速度。'},
  {key:'timbre',title:'音色变化',description:'调整声音的频谱与动态。'},
  {key:'volume',title:'音量起伏',description:'让音量在播放时缓慢起伏。'},
 ]},
 {title:'节奏',settings:[{key:'speed',title:'变速',description:'每个视频随机加速或减速，整段速度固定，默认保持音高。'}]},
 {title:'编码',settings:[{key:'encoding',title:'编码微调',description:'调整输出编码策略；关闭时仍进行基础转码。'}]},
];
interface Props {
 draft:CustomVariationSettings;recommended:CustomVariationSettings;busy:boolean;saving:boolean;error:string|null;
 onLevelChange:(key:CustomLevelKey,value:number)=>void;onRestore:()=>void;onCancel:()=>void;onSave:()=>Promise<boolean>;
}
export function CustomSettingsDialog({draft,recommended,busy,saving,error,onLevelChange,onRestore,onCancel,onSave}:Props){
 const dialog=useRef<HTMLDialogElement>(null);const id=useId();
 useEffect(()=>{
  const node=dialog.current;const previous=document.activeElement;
  node?.showModal();
  return()=>{node?.close();if(previous instanceof HTMLElement&&document.contains(previous))previous.focus();};
 },[]);
 function cancel(){if(!saving)onCancel();}
 return <dialog ref={dialog} className="custom-settings-dialog" aria-labelledby={`${id}-title`} aria-describedby={`${id}-description`} onCancel={event=>{event.preventDefault();cancel();}}>
  <header className="panel-head"><h2 id={`${id}-title`}>自定义变化设置</h2><button type="button" className="btn quiet" disabled={saving} onClick={cancel}>关闭</button></header>
  <div className="custom-settings-body">
   <p id={`${id}-description`} className="helper custom-intro">每项 0～9 档，0 关闭，9 更强。推荐档位偏温和，可按视频调整。同批共用档位，每个视频独立生成变化。</p>
   {busy&&<p className="notice warning" role="status">任务进行中，设置暂不可修改。</p>}
   {error&&<p className="notice error" role="alert">{error}</p>}
   {groups.map(group=><section key={group.title} className="custom-group" aria-labelledby={`${id}-${group.title}`}><h3 id={`${id}-${group.title}`}>{group.title}</h3>
    {group.settings.map(setting=>{
     const dependent=Boolean(setting.needsForeground&&draft.foregroundScale===0);
     const disabled=busy||saving||dependent;const inputId=`${id}-${setting.key}`;
     return <div className={`custom-setting${dependent?' inactive':''}`} key={setting.key}>
      <div className="custom-setting-description"><label htmlFor={inputId}>{setting.title}</label><p id={`${inputId}-description`} className="helper">{dependent?'前景缩放关闭时不生效，档位已保留。':setting.description}</p></div>
      <div className="custom-level-control"><div className="custom-level-caption"><span className="helper">推荐 {recommended[setting.key]} 档</span><output htmlFor={inputId}>{draft[setting.key]} 档{draft[setting.key]===0?' · 关闭':''}</output></div>
       <div className="custom-slider"><input id={inputId} type="range" min="0" max="9" step="1" value={draft[setting.key]} disabled={disabled} aria-describedby={`${inputId}-description`} aria-valuetext={`${draft[setting.key]} 档${draft[setting.key]===0?'，关闭':''}`} onChange={event=>{if(!disabled)onLevelChange(setting.key,Number(event.target.value));}}/><span className="recommended-tick" style={{left:`${recommended[setting.key]/9*100}%`}} aria-hidden="true"/></div>
      </div>
     </div>;
    })}
   </section>)}
   <p className="helper">没有音轨的视频会跳过声音设置。变速会改变时长；裁切缩放可能影响贴边内容。</p>
  </div>
  <footer className="custom-settings-footer"><button type="button" className="btn" disabled={busy||saving} onClick={onRestore}>恢复推荐</button><div><button type="button" className="btn" disabled={saving} onClick={cancel}>取消</button><button type="button" className="btn primary" disabled={busy||saving} onClick={()=>{if(!busy&&!saving)void onSave();}}>{saving?'正在保存':'保存设置'}</button></div></footer>
 </dialog>;
}
