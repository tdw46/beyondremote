import {fail,parse,remoteId,stamp,digest,token} from './core.mjs';

export function privateIp(ip){
 if(typeof ip!=='string'||!/^\d{1,3}(\.\d{1,3}){3}$/.test(ip))return false;
 const a=ip.split('.').map(Number);
 return a.every(n=>n<=255)&&(a[0]===10||a[0]===172&&a[1]>=16&&a[1]<=31||a[0]===192&&a[1]===168);
}
export function wakeProfiles(info){
 if(info.wake_enabled!==true||!Array.isArray(info.wake_profiles))return [];
 return info.wake_profiles.filter(p=>p&&typeof p.mac==='string'&&/^[0-9a-f]{2}(:[0-9a-f]{2}){5}$/i.test(p.mac)&&!(parseInt(p.mac.slice(0,2),16)&1)&&p.mac!=='00:00:00:00:00:00'&&privateIp(p.ip)&&Number.isInteger(p.prefix)&&p.prefix>=16&&p.prefix<=30).slice(0,4).map(({mac,ip,prefix})=>({mac,ip,prefix}));
}
const network=(ip,prefix)=>((ip.split('.').reduce((n,v)=>(n*256+Number(v))>>>0,0))&(~0<<(32-prefix)))>>>0;
const sameNetwork=(profiles,ips)=>profiles.some(p=>Array.isArray(ips)&&ips.some(ip=>privateIp(ip)&&network(ip,p.prefix)===network(p.ip,p.prefix)));
export async function wakeRoute(db,u,path,b){
 const source=remoteId(b.source_id),target=remoteId(b.target_id);
 if(!source||!target||source===target)fail('Choose another device');
 const targetRow=await db.prepare('SELECT device_info FROM br_devices WHERE user_id=? AND remote_id=?').bind(u.id,target).first();
 const profiles=wakeProfiles(parse(targetRow?.device_info));
 if(!profiles.length)fail('Enable wake on that computer first',409);
 if(path.endsWith('/prepare')){
  const owned=await db.prepare('SELECT 1 FROM br_devices WHERE user_id=? AND remote_id=?').bind(u.id,source).first();
  if(!owned)fail('Device is not in your account',403);
  // Reuse the selected, reachable home computer. There is no hosted wake queue or polling.
  const row=await db.prepare(`SELECT d.remote_id,d.device_info FROM br_server_preferences f
   JOIN br_devices d ON d.user_id=f.user_id AND d.remote_id=f.device_id
   JOIN br_presence p ON p.user_id=d.user_id AND p.remote_id=d.remote_id
   JOIN br_sessions s ON s.token_hash=p.token_hash AND s.user_id=d.user_id
   WHERE f.user_id=? AND d.remote_id<>? AND p.state='online-reported' AND s.native=1 AND s.expires_at>?`).bind(u.id,target,stamp()).first();
  const info=parse(row?.device_info);
  if(!row||info.wake_helper!=='wake-v1'||info.managed_server_running!==true||!sameNetwork(profiles,info.local_ips)||!/^([A-Za-z0-9+/]{43}=)$/.test(info.account_public_key||''))fail('Keep another home computer awake as your connection server',409);
  const grant=token();
  const result=await db.batch([
   db.prepare('DELETE FROM br_wake_grants WHERE token_hash IN(SELECT token_hash FROM br_wake_grants WHERE expires_at<=? LIMIT 100)').bind(stamp()),
   // Includes consumed grants: repeated clicks cannot create a packet flood.
   db.prepare(`INSERT INTO br_wake_grants(token_hash,user_id,source_token_hash,source_id,target_id,helper_id,expires_at)
    SELECT ?,?,?,?,?,?,? WHERE (SELECT COUNT(*) FROM br_wake_grants WHERE user_id=? AND expires_at>?)<3`).bind(await digest(grant),u.id,u.token_hash,source,target,row.remote_id,new Date(Date.now()+60000).toISOString(),u.id,stamp())
  ]);
  if(!result[1].meta.changes)fail('Wait a minute before trying wake again',429);
  const addresses=[...new Set((info.local_ips||[]).filter(privateIp))].slice(0,4);
  // Host is used only for the pinned-key encrypted control channel, never as a UDP destination.
  const host=String(info.managed_server_public_host||'').split(':')[0];
  if(/^[a-zA-Z0-9.-]{1,253}$/.test(host)&&!addresses.includes(host))addresses.push(host);
  return {grant,helper_id:row.remote_id,addresses,port:21128,public_key:info.account_public_key,expires_in:60};
 }
 const helper=remoteId(b.helper_id);
 if(!helper||typeof b.grant!=='string'||!/^[a-f0-9]{64}$/.test(b.grant))fail('Invalid wake request',403);
 const r=await db.prepare(`UPDATE br_wake_grants SET used_at=? WHERE token_hash=? AND user_id=? AND source_id=? AND target_id=? AND helper_id=? AND used_at IS NULL AND expires_at>?
 AND EXISTS(SELECT 1 FROM br_sessions s WHERE s.token_hash=br_wake_grants.source_token_hash AND s.user_id=br_wake_grants.user_id AND s.native=1 AND s.expires_at>?)
 AND EXISTS(SELECT 1 FROM br_devices d WHERE d.user_id=br_wake_grants.user_id AND d.remote_id=br_wake_grants.source_id)
 AND EXISTS(SELECT 1 FROM br_presence p JOIN br_devices d ON d.user_id=p.user_id AND d.remote_id=p.remote_id WHERE p.user_id=br_wake_grants.user_id AND p.remote_id=br_wake_grants.helper_id AND p.token_hash=? AND p.state='online-reported' AND json_extract(d.device_info,'$.wake_helper')='wake-v1' AND json_extract(d.device_info,'$.managed_server_running')=1 AND EXISTS(SELECT 1 FROM br_server_preferences f WHERE f.user_id=d.user_id AND f.device_id=d.remote_id))
 AND EXISTS(SELECT 1 FROM br_devices d WHERE d.user_id=br_wake_grants.user_id AND d.remote_id=br_wake_grants.target_id AND d.device_info=?) RETURNING target_id`).bind(stamp(),await digest(b.grant),u.id,source,target,helper,stamp(),stamp(),u.token_hash,targetRow.device_info).first();
 if(!r)fail('Wake permission expired; try again',403);
 return {profiles};
}
