import {act,cleanup,renderHook,waitFor} from '@testing-library/react';
import {afterEach,expect,it,vi} from 'vitest';
import type {DesktopApi} from '../../api/desktop';
import type {CustomSettingsStatus,CustomVariationSettings} from '../../api/contracts';
import fixture from '../../../tests/fixtures/custom-settings.json';
import {useCustomSettings} from './useCustomSettings';
afterEach(cleanup);
const settings:CustomVariationSettings={...fixture,schemaVersion:1};
const status:CustomSettingsStatus={settings,recommended:settings,source:'default',notice:null};
function deferred<T>(){let resolve!:(v:T)=>void;let reject!:(e:unknown)=>void;const promise=new Promise<T>((r,j)=>{resolve=r;reject=j;});return {promise,resolve,reject};}
function api(){return {available:true,getCustomSettings:vi.fn(async()=>status),saveCustomSettings:vi.fn(async(s:CustomVariationSettings)=>({...status,settings:s,source:'saved' as const}))} as unknown as DesktopApi;}
it('cancel_does_not_save',async()=>{
 const native=api();const {result}=renderHook(()=>useCustomSettings(native,false));
 await waitFor(()=>expect(result.current.loading).toBe(false));
 act(()=>result.current.openDialog());act(()=>result.current.setLevel('foregroundScale',9));
 expect(result.current.status?.settings.foregroundScale).toBe(2);
 act(()=>result.current.restoreRecommended());expect(result.current.draft?.foregroundScale).toBe(2);
 act(()=>result.current.cancel());expect(result.current.open).toBe(false);expect(native.saveCustomSettings).not.toHaveBeenCalled();
});
it('failed_save_keeps_draft',async()=>{
 const native=api();vi.mocked(native.saveCustomSettings).mockRejectedValue({code:'output_permission',message:'raw filter=secret'});
 const {result}=renderHook(()=>useCustomSettings(native,false));await waitFor(()=>expect(result.current.loading).toBe(false));
 act(()=>result.current.openDialog());act(()=>result.current.setLevel('speed',9));
 await act(async()=>{expect(await result.current.save()).toBe(false);});
 expect(result.current.open).toBe(true);expect(result.current.draft?.speed).toBe(9);expect(result.current.status?.settings.speed).toBe(0);
 expect(result.current.error).not.toContain('secret');
});
it('double_save_and_cancel_are_blocked_until_native_success',async()=>{
 const native=api();const pending=deferred<CustomSettingsStatus>();vi.mocked(native.saveCustomSettings).mockReturnValue(pending.promise);
 const {result}=renderHook(()=>useCustomSettings(native,false));await waitFor(()=>expect(result.current.loading).toBe(false));
 act(()=>result.current.openDialog());act(()=>result.current.setLevel('speed',4));
 let first!:Promise<boolean>;act(()=>{first=result.current.save();void result.current.save();result.current.cancel();});
 expect(native.saveCustomSettings).toHaveBeenCalledOnce();expect(result.current.open).toBe(true);expect(result.current.status?.settings.speed).toBe(0);
 await act(async()=>{pending.resolve({...status,source:'saved',settings:{...settings,speed:4}});expect(await first).toBe(true);});
 expect(result.current.open).toBe(false);expect(result.current.status?.settings.speed).toBe(4);
});
it('external_busy_preserves_draft_and_rejects_invalid_levels',async()=>{
 const native=api();const hook=renderHook(({busy})=>useCustomSettings(native,busy),{initialProps:{busy:false}});
 await waitFor(()=>expect(hook.result.current.loading).toBe(false));act(()=>hook.result.current.openDialog());
 for(const value of [-1,1.5,10,NaN])act(()=>hook.result.current.setLevel('speed',value));
 expect(hook.result.current.draft?.speed).toBe(0);act(()=>hook.result.current.setLevel('speed',6));
 hook.rerender({busy:true});act(()=>hook.result.current.setLevel('speed',9));
 await act(async()=>{expect(await hook.result.current.save()).toBe(false);});
 expect(hook.result.current.draft?.speed).toBe(6);expect(native.saveCustomSettings).not.toHaveBeenCalled();
 act(()=>hook.result.current.cancel());expect(hook.result.current.open).toBe(false);
});
it('old_load_and_save_responses_do_not_cross_api_mount_generations',async()=>{
 const old=api();const load=deferred<CustomSettingsStatus>();vi.mocked(old.getCustomSettings).mockReturnValue(load.promise);
 const next=api();vi.mocked(next.getCustomSettings).mockResolvedValue({...status,settings:{...settings,speed:3}});
 const hook=renderHook(({native})=>useCustomSettings(native,false),{initialProps:{native:old}});
 hook.rerender({native:next});await waitFor(()=>expect(hook.result.current.status?.settings.speed).toBe(3));
 await act(async()=>load.resolve(status));expect(hook.result.current.status?.settings.speed).toBe(3);
 const save=deferred<CustomSettingsStatus>();vi.mocked(next.saveCustomSettings).mockReturnValue(save.promise);
 act(()=>hook.result.current.openDialog());let saving!:Promise<boolean>;act(()=>{saving=hook.result.current.save();});hook.unmount();
 await act(async()=>{save.resolve(status);expect(await saving).toBe(false);});
});
