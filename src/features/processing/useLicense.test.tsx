import {act, cleanup, renderHook} from '@testing-library/react';
import {afterEach, expect, it, vi} from 'vitest';
import {useLicense} from './useLicense';

afterEach(()=>{cleanup();vi.useRealTimers();});
const expiresAtMs=1798732800000;
const active={expiresAtMs,effectiveTimeMs:expiresAtMs-500,expired:false,ntpAvailable:true};

it('disables processing when an open window crosses the network deadline',async()=>{
  vi.useFakeTimers();vi.setSystemTime(expiresAtMs-100_000);
  const api={available:true,getLicenseStatus:vi.fn(async()=>active)};
  const hook=renderHook(()=>useLicense(api));
  await act(async()=>{});
  expect(hook.result.current.licenseAllowed).toBe(true);
  await act(async()=>vi.advanceTimersByTimeAsync(1000));
  expect(hook.result.current.licenseAllowed).toBe(false);
  expect(hook.result.current.licenseExpired).toBe(true);
});

it('rechecks on focus and retains expiry after a failed later refresh',async()=>{
  vi.useFakeTimers();vi.setSystemTime(expiresAtMs-100_000);
  const api={available:true,getLicenseStatus:vi.fn(async()=>({...active,effectiveTimeMs:expiresAtMs-100_000}))};
  const hook=renderHook(()=>useLicense(api));await act(async()=>{});
  api.getLicenseStatus.mockResolvedValueOnce({...active,expired:true,effectiveTimeMs:expiresAtMs});
  await act(async()=>window.dispatchEvent(new Event('focus')));
  expect(hook.result.current.licenseExpired).toBe(true);
  api.getLicenseStatus.mockRejectedValueOnce(new Error('IPC failed'));
  await act(async()=>vi.advanceTimersByTimeAsync(60_000));
  expect(hook.result.current.licenseAllowed).toBe(false);
  expect(hook.result.current.licenseHint).toContain('到期');
});

it('blocks a failed status query with a friendly retry message',async()=>{
  const api={available:true,getLicenseStatus:async()=>{throw new Error('IPC failed');}};
  const hook=renderHook(()=>useLicense(api));
  await act(async()=>{});
  expect(hook.result.current.licenseAllowed).toBe(false);
  expect(hook.result.current.licenseHint).toContain('请稍后重试');
});

it('removes timers and focus handlers after unmount',async()=>{
  vi.useFakeTimers();
  const api={available:true,getLicenseStatus:vi.fn(async()=>active)};
  const hook=renderHook(()=>useLicense(api));await act(async()=>{});
  hook.unmount();
  await act(async()=>{window.dispatchEvent(new Event('focus'));await vi.advanceTimersByTimeAsync(60_000);});
  expect(api.getLicenseStatus).toHaveBeenCalledTimes(1);
});
