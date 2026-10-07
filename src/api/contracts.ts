export type JobState = 'probing'|'preparing'|'running'|'validating'|'committing'|'succeeded'|'failed'|'canceling'|'canceled';
export type ErrorCode = 'unsupported_input'|'damaged_media'|'tool_missing'|'output_permission'|'disk_full'|'process_exit'|'validation_failed'|'output_conflict'|'input_changed'|'busy'|'canceled'|'cleanup_pending';
export interface AppError {code:ErrorCode;message:string;details:string|null}
export interface Rational {num:number;den:number}
export interface FileIdentity {canonicalPath:string;sizeBytes:number;modifiedNs:string;sha256:string}
export interface VideoInfo {streamIndex:number;codec:string;width:number;height:number;bitDepth:number;pixelFormat:string;frameRate:Rational;timeBase:Rational;frameCount:number;startPts:number;durationTicks:number;bitRate:number|null;colorRange:string|null;colorSpace:string|null;colorPrimaries:string|null;colorTransfer:string|null}
export interface AudioInfo {streamIndex:number;codec:string;sampleRate:number;channels:number;timeBase:Rational;startPts:number;durationTicks:number;bitRate:number|null}
export interface MediaInfo {identity:FileIdentity;container:string;video:VideoInfo;audio:AudioInfo|null;title:string|null;comment:string|null}
export type MetadataRequest = {mode:'preserve'}|{mode:'override';title:string|null;comment:string|null};
export interface StartJobRequest {inputPath:string;outputDirectory:string;presetId:string;metadata:MetadataRequest}
export interface PresetSummary {presetId:string;version:number;title:string;evidenceStatus:string}
export interface JobSnapshot {jobId:string;version:number;state:JobState;progress:number|null;startedAtMs:number;endedAtMs:number|null;outputPath:string|null;error:AppError|null;cleanupPending:boolean}
export interface LogExcerpt {text:string;truncated:boolean}
export interface BatchSettings {outputDirectory:string;presetId:string;metadata:MetadataRequest}
export interface QueueItem {itemId:string;inputPath:string;state:'waiting'|'started'|'canceled';jobId:string|null;snapshot:JobSnapshot|null;media:MediaInfo|null}
export interface QueueSnapshot {version:number;running:boolean;items:QueueItem[]}
