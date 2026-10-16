import {useCallback,useEffect,useRef,useState} from 'react';
import type {DesktopApi} from '../../api/desktop';
import type {CustomLevelKey,CustomSettingsStatus,CustomVariationSettings} from '../../api/contracts';

type SettingsState={status:CustomSettingsStatus|null;loading:boolean;saving:boolean;open:boolean;draft:CustomVariationSettings|null;error:string|null};
const initial=():SettingsState=>({status:null,loading:true,saving:false,open:false,draft:null,error:null});
function saveError(error:unknown):string {
  const code=typeof error==='object'&&error!==null&&'code' in error?error.code:null;
  if(code==='busy')return '任务进行中，设置暂不可修改。';
  if(code==='invalid_settings')return '处理设置无效，请恢复推荐设置后重试。';
  if(code==='disk_full')return '空间不足，未保存设置。请释放空间后重试。';
  return '设置保存失败，请检查应用数据目录后重试。';
}
export function useCustomSettings(api:DesktopApi,busy:boolean) {
  const [state,setState]=useState(initial);
  const current=useRef(state);
  const busyRef=useRef(busy);busyRef.current=busy;
  const mounted=useRef(false);
  const generation=useRef(0);
  const revision=useRef(0);
  const update=useCallback((patch:Partial<SettingsState>)=>{current.current={...current.current,...patch};setState(current.current);},[]);
  useEffect(()=>{
    mounted.current=true;const attempt=++generation.current;const readRevision=++revision.current;
    update({...initial(),loading:api.available});
    if(api.available)void api.getCustomSettings().then(status=>{
      if(mounted.current&&attempt===generation.current&&readRevision===revision.current)update({status,loading:false,error:null});
    }).catch(()=>{
      if(mounted.current&&attempt===generation.current&&readRevision===revision.current)update({loading:false,error:'无法加载自定义设置，请重新打开应用后重试。'});
    });
    return()=>{mounted.current=false;++generation.current;++revision.current;};
  },[api,update]);
  function openDialog(){
    const s=current.current;
    if(!mounted.current||!api.available||busyRef.current||s.loading||s.saving||!s.status)return;
    update({open:true,draft:{...s.status.settings},error:null});
  }
  function setLevel(key:CustomLevelKey,value:number){
    const s=current.current;
    if(busyRef.current||s.saving||!s.open||!s.draft||!Number.isInteger(value)||value<0||value>9||!Object.hasOwn(s.draft,key)||(key as string)==='schemaVersion')return;
    update({draft:{...s.draft,[key]:value},error:null});
  }
  function restoreRecommended(){
    const s=current.current;
    if(!busyRef.current&&!s.saving&&s.open&&s.status)update({draft:{...s.status.recommended},error:null});
  }
  function cancel(){if(current.current.saving)return;update({open:false,draft:null,error:null});}
  async function save():Promise<boolean>{
    const s=current.current;
    if(!mounted.current||!api.available||busyRef.current||s.saving||!s.open||!s.draft)return false;
    const draft={...s.draft};const attempt=generation.current;const request=++revision.current;
    update({saving:true,error:null});
    try{
      const status=await api.saveCustomSettings(draft);
      if(!mounted.current||attempt!==generation.current||request!==revision.current)return false;
      update({status,open:false,draft:null,saving:false,error:null});return true;
    }catch(error){
      if(mounted.current&&attempt===generation.current&&request===revision.current)update({saving:false,error:saveError(error)});
      return false;
    }
  }
  // Starting can be invoked in the same event as opening/saving a draft.
  // Read the ref here so those calls observe the synchronous edit lock.
  const getSnapshot=useCallback(()=>current.current,[]);
  return {...state,openDialog,setLevel,restoreRecommended,cancel,save,getSnapshot};
}
