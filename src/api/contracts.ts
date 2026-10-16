export type JobState = 'probing'|'preparing'|'running'|'validating'|'committing'|'succeeded'|'failed'|'canceling'|'canceled';
export type ErrorCode = 'invalid_settings'|'unsupported_input'|'damaged_media'|'tool_missing'|'output_permission'|'disk_full'|'process_exit'|'validation_failed'|'output_conflict'|'input_changed'|'busy'|'canceled'|'cleanup_pending'|'license_expired';
export interface LicenseStatus {expiresAtMs:number;effectiveTimeMs:number;expired:boolean;ntpAvailable:boolean}
export interface AppError {code:ErrorCode;message:string;details:string|null}
export interface Rational {num:number;den:number}
export interface FileIdentity {canonicalPath:string;sizeBytes:number;modifiedNs:string;sha256:string}
export interface VideoTimeline {variableFrameRate:boolean;timestampSha256:string}
export interface VideoInfo {streamIndex:number;codec:string;width:number;height:number;bitDepth:number;pixelFormat:string;frameRate:Rational;timeBase:Rational;frameCount:number;startPts:number;durationTicks:number;timeline?:VideoTimeline;bitRate:number|null;colorRange:string|null;colorSpace:string|null;colorPrimaries:string|null;colorTransfer:string|null}
export interface AudioInfo {streamIndex:number;codec:string;sampleRate:number;channels:number;channelLayout?:string;timeBase:Rational;startPts:number;durationTicks:number;bitRate:number|null}
export interface MediaInfo {identity:FileIdentity;container:string;video:VideoInfo;audio:AudioInfo|null;title:string|null;comment:string|null}
export type MetadataRequest = {mode:'preserve'}|{mode:'override';title:string|null;comment:string|null};
export interface StartJobRequest {inputPath:string;outputDirectory:string;presetId:string;metadata:MetadataRequest;customSettings?:CustomVariationSettings}
export interface PresetSummary {presetId:string;version:number;title:string;evidenceStatus:string}
export interface JobSnapshot {jobId:string;version:number;state:JobState;progress:number|null;startedAtMs:number;endedAtMs:number|null;outputPath:string|null;error:AppError|null;cleanupPending:boolean}
export interface LogExcerpt {text:string;truncated:boolean}
export interface BatchSettings {outputDirectory:string;presetId:string;metadata:MetadataRequest;customSettings?:CustomVariationSettings}
export interface QueueItem {itemId:string;inputPath:string;state:'waiting'|'started'|'canceled';jobId:string|null;snapshot:JobSnapshot|null;media:MediaInfo|null}
export interface QueueSnapshot {version:number;running:boolean;items:QueueItem[];activeSettings?:ActiveProcessingSettings}

export interface CustomVariationSettings {schemaVersion:1;foregroundScale:number;backgroundBlur:number;dynamicOffset:number;cropScale:number;hue:number;saturation:number;tone:number;noise:number;softness:number;pitch:number;timbre:number;volume:number;speed:number;encoding:number}
export type CustomLevelKey = Exclude<keyof CustomVariationSettings, 'schemaVersion'>;
export interface CustomSettingsStatus {settings:CustomVariationSettings;recommended:CustomVariationSettings;source:'default'|'saved'|'recovered';notice:string|null}
export interface ActiveProcessingSettings {presetId:string;customSettings?:CustomVariationSettings}
