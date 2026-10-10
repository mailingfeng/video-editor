import {useEffect, useState} from 'react';
import type {DesktopApi} from '../../api/desktop';
import type {LicenseStatus} from '../../api/contracts';

const expiredMessage='使用许可已于 2026-12-31 到期，暂时无法开始处理。请联系软件提供方更新许可。';
const failedMessage='暂时无法确认使用许可，请稍后重试或重新打开应用。';
interface Observation {status:LicenseStatus;observedAt:number}

export function useLicense(api:Pick<DesktopApi,'available'|'getLicenseStatus'>) {
  const [observation,setObservation]=useState<Observation|null>(null);
  const [failed,setFailed]=useState(false);
  const [,forceTick]=useState(0);
  useEffect(()=>{
    let alive=true;let checking=false;
    setObservation(null);setFailed(false);
    const refresh=async()=>{
      if(checking || !alive)return;
      checking=true;
      try {
        const status=await api.getLicenseStatus();
        if(alive){
          const now=performance.now();
          setObservation(previous=>({
            status:{...status,expired:status.expired || Boolean(previous?.status.expired),
              effectiveTimeMs:Math.max(status.effectiveTimeMs,previous ? previous.status.effectiveTimeMs+Math.max(0,now-previous.observedAt) : 0)},
            observedAt:now,
          }));
          setFailed(false);
        }
      } catch {if(alive)setFailed(true);}
      finally{checking=false;}
    };
    if(!api.available)return ()=>{alive=false;};
    void refresh();
    const networkTimer=window.setInterval(()=>void refresh(),60_000);
    const clockTimer=window.setInterval(()=>forceTick(value=>value+1),1000);
    const onFocus=()=>void refresh();
    window.addEventListener('focus',onFocus);
    return ()=>{alive=false;window.clearInterval(networkTimer);window.clearInterval(clockTimer);window.removeEventListener('focus',onFocus);};
  },[api]);
  const status=observation?.status;
  const effectiveTime=observation ? Math.max(Date.now(),observation.status.effectiveTimeMs+Math.max(0,performance.now()-observation.observedAt)) : 0;
  const licenseExpired=Boolean(status && (status.expired || effectiveTime>=status.expiresAtMs));
  const licenseAllowed=api.available && Boolean(status) && !failed && !licenseExpired;
  const licenseHint=!api.available ? null : licenseExpired ? expiredMessage : failed ? failedMessage : !status ? '正在检查使用许可，请稍候。' : null;
  const licenseNotice=licenseHint ?? (api.available && status && !status.ntpAvailable ? '暂时无法连接网络时间服务器，已使用本机时间及本次运行已确认的时间检查许可。' : null);
  const markLicenseExpired=()=>setObservation(previous=>previous ? {...previous,status:{...previous.status,expired:true}} : previous);
  return {licenseAllowed,licenseExpired,licenseHint,licenseNotice,markLicenseExpired};
}
