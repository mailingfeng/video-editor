import {describe, expect, it} from 'vitest';
import type {JobSnapshot, QueueItem, QueueSnapshot} from '../../api/contracts';
import {emptyQueueState, mergeJob, mergeQueue} from './queueState';

const job = (jobId:string, version=1, state:JobSnapshot['state']='running'):JobSnapshot => ({jobId,version,state,progress:state==='succeeded'?1:0.25,startedAtMs:1,endedAtMs:null,outputPath:null,error:null,cleanupPending:false});
const item = (itemId:string, jobId:string|null=null):QueueItem => ({itemId,inputPath:`/${itemId}.mp4`,state:jobId?'started':'waiting',jobId,snapshot:jobId?job(jobId):null,media:null});
const queue = (version:number,items:QueueItem[],running=true):QueueSnapshot => ({version,items,running});

describe('queue version and job ownership',()=>{
  it('keeps per-job versions across interleaved events and an older full query',()=>{
    let s=mergeQueue(emptyQueueState(),queue(2,[item('a','A'),item('b','B')]));
    s=mergeJob(s,job('A',8)); s=mergeJob(s,job('B',3));
    s=mergeQueue(s,queue(1,[item('a','A')]));
    expect(s.queue.items).toHaveLength(2);
    expect(s.queue.items.map(i=>i.snapshot?.version)).toEqual([8,3]);
    s=mergeJob(s,job('A',7)); expect(s.queue.items[0].snapshot?.version).toBe(8);
  });
  it('merges events arriving before the structure and protects terminals',()=>{
    let s=mergeJob(emptyQueueState(),job('A',8,'succeeded'));
    s=mergeQueue(s,queue(2,[item('a','A')]));
    expect(s.queue.items[0].snapshot?.state).toBe('succeeded');
    s=mergeJob(s,job('A',99)); expect(s.queue.items[0].snapshot?.state).toBe('succeeded');
    s=mergeQueue(s,queue(3,[item('a','A')])); expect(s.queue.items[0].snapshot?.version).toBe(8);
  });
  it('drops unknown old jobs after query and never binds them to a reimported item',()=>{
    let s=mergeQueue(emptyQueueState(),queue(1,[item('a','old')]));
    s=mergeQueue(s,queue(2,[item('new')]));
    s=mergeJob(s,job('old',99,'failed'));
    s=mergeQueue(s,queue(2,[item('new')]),true);
    expect(s.pending).toEqual({}); expect(s.queue.items[0].snapshot).toBeNull();
  });
});
