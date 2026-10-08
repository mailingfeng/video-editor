import {useCallback,useEffect,useRef,useState} from 'react';
import type {DesktopApi} from '../../api/desktop';
import type {JobSnapshot,LogExcerpt,PresetSummary,QueueSnapshot} from '../../api/contracts';
import {emptyQueueState,mergeJob,mergeQueue,terminal,type QueueState} from './queueState';

function message(error:unknown):string {
  if (typeof error==='object' && error!==null && 'message' in error) return String(error.message);
  return typeof error==='string'?error:'操作未完成，请查看日志后重试。';
}

export function useProcessing(api:DesktopApi) {
  const [state,setState]=useState(emptyQueueState);
  const stateRef=useRef(state);
  const [presets,setPresets]=useState<PresetSummary[]>([]);
  const [outputDirectory,setOutputDirectory]=useState('');
  const [selectedId,setSelectedId]=useState<string|null>(null);
  const selectedRef=useRef<string|null>(null);
  const [historical,setHistorical]=useState<JobSnapshot|null>(null);
  const [log,setLog]=useState<LogExcerpt|null>(null);
  const [logLoading,setLogLoading]=useState(false);
  const [logError,setLogError]=useState<string|null>(null);
  const [error,setError]=useState<string|null>(null);
  const [importing,setImporting]=useState(false);
  const [starting,setStarting]=useState(false);
  const mounted=useRef(false);
  const epoch=useRef(0);
  const detailGeneration=useRef(0);
  const importLock=useRef(false);
  const startLock=useRef(false);

  const update=useCallback((next:QueueState)=>{
    stateRef.current=next;setState(next);
    if (!next.queue.items.some(i=>i.itemId===selectedRef.current)) {
      selectedRef.current=next.queue.items[0]?.itemId??null;
      setSelectedId(selectedRef.current);setLog(null);setLogLoading(false);setLogError(null);++detailGeneration.current;
    }
  },[]);
  const accept=useCallback((next:QueueSnapshot,query=false)=>{
    if (mounted.current) update(mergeQueue(stateRef.current,next,query));
  },[update]);
  const importPaths=useCallback(async(paths:string[])=>{
    if (stateRef.current.queue.running || startLock.current || importLock.current || !paths.length) return;
    importLock.current=true;setImporting(true);setError(null);const attempt=epoch.current;
    try {const next=await api.importPaths(paths);if(mounted.current && attempt===epoch.current) accept(next);}
    catch(e){if(mounted.current && attempt===epoch.current)setError(message(e));}
    finally{if(attempt===epoch.current){importLock.current=false;if(mounted.current)setImporting(false);}}
  },[api,accept]);

  useEffect(()=>{
    mounted.current=true;++epoch.current;let alive=true;
    const disposers:(()=>void)[]=[];
    const add=(off:()=>void)=>{if(alive)disposers.push(off);else off();};
    let queryRunning=false;let unknownDuringQuery=false;let subscriptionsReady=false;
    const query=async()=>{
      if(queryRunning){unknownDuringQuery=true;return;}
      queryRunning=true;
      try {
        do {
          unknownDuringQuery=false;
          const next=await api.getQueueSnapshot();
          if(!alive)return;
          accept(next,!unknownDuringQuery);
        } while(unknownDuringQuery && alive);
      } catch(e){if(alive)setError(message(e));}
      finally{queryRunning=false;}
    };
    if(api.available) {
      const subscribed=Promise.all([
        api.subscribeSnapshots(next=>{
          if(!alive)return;
          const known=stateRef.current.queue.items.some(i=>i.jobId===next.jobId);
          update(mergeJob(stateRef.current,next));
          if(!known && subscriptionsReady)void query();
        }).then(add),
        api.subscribeQueue(next=>{if(alive)accept(next);}).then(add),
      ]);
      void subscribed.then(async()=>{
        if(!alive)return;
        subscriptionsReady=true;
        await query();
        if(!alive)return;
        const restored=await api.getCurrentJobSnapshot();
        if(alive && restored && terminal(restored))setHistorical(restored);
      }).catch(e=>{if(alive)setError(message(e));});
      void api.subscribeFileDrop(paths=>{if(alive)void importPaths(paths);}).then(add).catch(e=>{if(alive)setError(message(e));});
      void api.listPresets().then(items=>{if(alive)setPresets(items);}).catch(e=>{if(alive)setError(message(e));});
    }
    return ()=>{
      alive=false;mounted.current=false;++epoch.current;++detailGeneration.current;
      importLock.current=false;startLock.current=false;
      disposers.forEach(off=>off());
    };
  },[api,accept,importPaths,update]);

  const busy=starting || state.queue.running;
  const selectedItem=state.queue.items.find(i=>i.itemId===selectedId)??null;
  const snapshot=selectedItem?.snapshot??(state.queue.items.length===0?historical:null);
  const previousResult=state.queue.items.length===0 && historical!==null;
  const jobForSelection=()=>{
    const selected=stateRef.current.queue.items.find(i=>i.itemId===selectedRef.current);
    return selected?.jobId ?? (stateRef.current.queue.items.length===0?historical?.jobId:null) ?? null;
  };
  function selectItem(id:string){selectedRef.current=id;setSelectedId(id);setLog(null);setLogLoading(false);setLogError(null);++detailGeneration.current;}
  async function selectInput(){
    if(busy || importing)return;const attempt=epoch.current;
    try{const path=await api.pickInput();if(mounted.current && attempt===epoch.current && path)await importPaths([path]);}
    catch(e){if(mounted.current && attempt===epoch.current)setError(message(e));}
  }
  async function selectFolder(){
    if(busy || importLock.current)return;const attempt=epoch.current;
    let acquired=false;
    try{
      const path=await api.pickInputFolder();
      if(!path || !mounted.current || attempt!==epoch.current || stateRef.current.queue.running || startLock.current || importLock.current)return;
      importLock.current=true;acquired=true;setImporting(true);setError(null);
      const next=await api.importFolder(path);
      if(mounted.current && attempt===epoch.current)accept(next);
    }catch(e){if(mounted.current && attempt===epoch.current)setError(message(e));}
    finally{if(acquired && attempt===epoch.current){importLock.current=false;if(mounted.current)setImporting(false);}}
  }
  async function selectOutput(){
    if(busy)return;const attempt=epoch.current;
    try{const path=await api.pickOutputDirectory();if(path && mounted.current && attempt===epoch.current && !stateRef.current.queue.running && !startLock.current)setOutputDirectory(path);}
    catch(e){if(mounted.current && attempt===epoch.current)setError(message(e));}
  }
  async function start(){
    if(startLock.current || importLock.current || stateRef.current.queue.running || !stateRef.current.queue.items.some(i=>i.state==='waiting') || !outputDirectory || !presets[0])return;
    startLock.current=true;setStarting(true);setError(null);const attempt=epoch.current;
    try{const next=await api.startBatch({outputDirectory,presetId:presets[0].presetId,metadata:{mode:'preserve'}});if(mounted.current && attempt===epoch.current)accept(next);}
    catch(e){if(mounted.current && attempt===epoch.current)setError(message(e));}
    finally{if(attempt===epoch.current){startLock.current=false;if(mounted.current)setStarting(false);}}
  }
  async function itemAction(id:string,action:'remove'|'cancel'){
    const attempt=epoch.current;
    try{const next=await (action==='remove'?api.removeItem(id):api.cancelItem(id));if(mounted.current && attempt===epoch.current)accept(next);}
    catch(e){if(mounted.current && attempt===epoch.current)setError(message(e));}
  }
  async function refreshLog(){
    const id=jobForSelection();if(!id)return;
    const generation=++detailGeneration.current;const attempt=epoch.current;
    setLogLoading(true);setLogError(null);
    try{const next=await api.getJobLog(id);if(mounted.current && attempt===epoch.current && generation===detailGeneration.current && id===jobForSelection())setLog(next);}
    catch(e){if(mounted.current && attempt===epoch.current && generation===detailGeneration.current && id===jobForSelection())setLogError(message(e));}
    finally{if(mounted.current && attempt===epoch.current && generation===detailGeneration.current)setLogLoading(false);}
  }
  async function reveal(itemId?:string){
    const item=stateRef.current.queue.items.find(i=>i.itemId===(itemId??selectedRef.current));
    const snap=item?.snapshot??(stateRef.current.queue.items.length===0?historical:null);
    if(snap?.state!=='succeeded')return;const attempt=epoch.current;
    try{await api.revealOutput(snap.jobId);}catch(e){if(mounted.current && attempt===epoch.current)setError(message(e));}
  }
  return {queue:state.queue,selectedItem,media:selectedItem?.media??null,snapshot,previousResult,presets,outputDirectory,log,logLoading,logError,error,importing,starting,busy,
    selectItem,selectInput,selectFolder,selectOutput,start,removeItem:(id:string)=>itemAction(id,'remove'),cancelItem:(id:string)=>itemAction(id,'cancel'),refreshLog,reveal};
}
