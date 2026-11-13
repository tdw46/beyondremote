import {fail,digest,token,remoteId,stamp,parse} from './core.mjs';
// Connection tickets are short-lived, one-use and never contain account tokens.
export async function connectRoute(db,u,path,b){
 const source=remoteId(b.source_id),target=remoteId(b.target_id);
 if(!source||!target||source===target)fail('Choose another device');
 if(path.endsWith('/route')){
  const rows=(await db.prepare('SELECT remote_id,device_info FROM br_devices WHERE user_id=? AND remote_id IN(?,?)').bind(u.id,source,target).all()).results;
  if(!rows.some(d=>d.remote_id===source)||!rows.some(d=>d.remote_id===target))fail('Device is not in your account',403);
  const info=parse(rows.find(d=>d.remote_id===target).device_info);
  const privateIp=ip=>{if(typeof ip!=='string'||!/^\d{1,3}(\.\d{1,3}){3}$/.test(ip))return false;const a=ip.split('.').map(Number);return a.every(n=>n<=255)&&(a[0]===10||a[0]===172&&a[1]>=16&&a[1]<=31||a[0]===192&&a[1]===168);};
  const ready=info.account_connection==='ticket-v1'&&info.account_direct_port===21128&&typeof info.account_public_key==='string'&&/^[A-Za-z0-9+/]{43}=$/.test(info.account_public_key);
  let public_address='',public_port=21128;
  const publicIp=ip=>{if(typeof ip!=='string'||!/^\d{1,3}(\.\d{1,3}){3}$/.test(ip))return false;const [a,b,c,...rest]=ip.split('.').map(Number);return [a,b,c,...rest].every(n=>n<=255)&&!privateIp(ip)&&a>0&&a<224&&a!==127&&!(a===169&&b===254)&&!(a===100&&b>=64&&b<=127)&&!(a===192&&b===0)&&!(a===198&&(b===18||b===19||b===51&&c===100))&&!(a===203&&b===0&&c===113);};
  if(ready&&publicIp(info.account_public_address)&&Number.isInteger(info.account_public_port)&&info.account_public_port>=40000&&info.account_public_port<60000){
   const active=await db.prepare(`SELECT 1 FROM br_presence p JOIN br_sessions s ON s.token_hash=p.token_hash AND s.user_id=p.user_id AND s.native=1 AND julianday(s.expires_at)>julianday(?) WHERE p.user_id=? AND p.remote_id=? AND p.state='online-reported'`).bind(stamp(),u.id,target).first();
   if(active){public_address=info.account_public_address;public_port=info.account_public_port;}
  }
  if(!public_address&&ready&&info.wake_helper==='wake-v1'&&info.managed_server_running===true){
   const selected=await db.prepare('SELECT 1 FROM br_server_preferences WHERE user_id=? AND device_id=?').bind(u.id,target).first();
   const host=String(info.managed_server_public_host||'').split(':')[0];
   if(selected&&/^[a-zA-Z0-9.-]{1,253}$/.test(host))public_address=host;
  }
  return {port:21128,public_address,public_port,public_key:ready?info.account_public_key:'',addresses:ready&&Array.isArray(info.local_ips)?[...new Set(info.local_ips.filter(privateIp))].slice(0,4):[]};
 }
 if(path.endsWith('/grant')){
  const devices=(await db.prepare('SELECT remote_id FROM br_devices WHERE user_id=? AND remote_id IN(?,?)').bind(u.id,source,target).all()).results;
  if(!devices.some(d=>d.remote_id===source)||!devices.some(d=>d.remote_id===target))fail('Device is not in your account',403);
  const grant=token();const result=await db.batch([
   db.prepare('DELETE FROM br_connect_grants WHERE token_hash IN(SELECT token_hash FROM br_connect_grants WHERE expires_at<=? LIMIT 100)').bind(stamp()),
   db.prepare('INSERT INTO br_connect_grants(token_hash,user_id,source_token_hash,source_id,target_id,expires_at) SELECT ?,?,?,?,?,? WHERE (SELECT COUNT(*) FROM br_connect_grants WHERE user_id=? AND expires_at>? AND used_at IS NULL)<20').bind(await digest(grant),u.id,u.token_hash,source,target,new Date(Date.now()+60000).toISOString(),u.id,stamp())
  ]);if(!result[1].meta.changes)fail('Too many pending connections',429);return {grant,expires_in:60};
 }
 if(typeof b.grant!=='string'||!/^[a-f0-9]{64}$/.test(b.grant))fail('Invalid connection ticket',403);
 const row=await db.prepare(`UPDATE br_connect_grants SET used_at=? WHERE token_hash=? AND user_id=? AND source_id=? AND target_id=? AND used_at IS NULL AND expires_at>? AND EXISTS(SELECT 1 FROM br_sessions s WHERE s.token_hash=br_connect_grants.source_token_hash AND s.user_id=br_connect_grants.user_id AND s.native=1 AND s.expires_at>?) AND EXISTS(SELECT 1 FROM br_devices d WHERE d.user_id=br_connect_grants.user_id AND d.remote_id=br_connect_grants.target_id) RETURNING source_id,target_id`).bind(stamp(),await digest(b.grant),u.id,source,target,stamp(),stamp()).first();
 if(!row)fail('Connection permission expired; reconnect',403);
 return {ok:true,...row};
}
