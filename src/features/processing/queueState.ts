import type {JobSnapshot, QueueSnapshot} from '../../api/contracts';

export const terminal = (s:JobSnapshot|null) => !s || ['succeeded','failed','canceled'].includes(s.state);
export interface QueueState {queue:QueueSnapshot;pending:Record<string,JobSnapshot>}
export const emptyQueueState = ():QueueState => ({queue:{version:-1,running:false,items:[]},pending:{}});

function newest(previous:JobSnapshot|null|undefined,next:JobSnapshot|null|undefined):JobSnapshot|null {
  if (!next) return previous ?? null;
  if (!previous) return next;
  return terminal(previous) || previous.version >= next.version ? previous : next;
}

export function mergeJob(state:QueueState,next:JobSnapshot):QueueState {
  const index=state.queue.items.findIndex(item=>item.jobId===next.jobId);
  if (index<0) return {...state,pending:{...state.pending,[next.jobId]:newest(state.pending[next.jobId],next)!}};
  const previous=state.queue.items[index];
  const snapshot=newest(previous.snapshot,next);
  if (snapshot===previous.snapshot) return state;
  const items=[...state.queue.items];items[index]={...previous,snapshot};
  return {...state,queue:{...state.queue,items}};
}

export function mergeQueue(state:QueueState,next:QueueSnapshot,discardUnknown=false):QueueState {
  const structure=next.version>=state.queue.version?next:state.queue;
  const previousJobs=new Map(state.queue.items.filter(i=>i.jobId).map(i=>[i.jobId,i.snapshot]));
  const incomingJobs=new Map(next.items.filter(i=>i.jobId).map(i=>[i.jobId,i.snapshot]));
  const pending={...state.pending};
  const items=structure.items.map(item=>{
    if (!item.jobId) return item;
    const snapshot=newest(newest(previousJobs.get(item.jobId),incomingJobs.get(item.jobId)),pending[item.jobId]);
    delete pending[item.jobId];
    return {...item,snapshot};
  });
  return {queue:{...structure,items},pending:discardUnknown?{}:pending};
}
