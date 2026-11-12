import {fail,parse,stamp} from './core.mjs';

export async function accountServers(db,u){
 const rows=(await db.prepare(`SELECT d.remote_id,d.device_info,p.device_id AS selected_id,
  s.state, CASE WHEN a.token_hash IS NOT NULL THEN 1 ELSE 0 END AS active_session
  FROM br_devices d LEFT JOIN br_server_preferences p ON p.user_id=d.user_id
  LEFT JOIN br_presence s ON s.user_id=d.user_id AND s.remote_id=d.remote_id
  LEFT JOIN br_sessions a ON a.token_hash=s.token_hash AND a.user_id=d.user_id
   AND a.native=1 AND julianday(a.expires_at)>julianday(?)
  WHERE d.user_id=? ORDER BY d.remote_id LIMIT 101`).bind(stamp(),u.id).all()).results;
 if(rows.length>100)fail('Too many devices',413);
 return rows.map(r=>{
  const i=parse(r.device_info),desktop=/windows|mac|darwin|linux/i.test(String(i.os||''));
  const configured=i.managed_server_running===true&&typeof i.managed_server_public_host==='string'&&i.managed_server_public_host.trim()&&typeof i.managed_server_key==='string'&&i.managed_server_key.trim();
  return {id:r.remote_id,name:i.name||r.remote_id,desktop,selected:r.selected_id===r.remote_id,
   state:configured&&r.active_session===1&&r.state==='online-reported'?'reported-ready':configured?'unconfirmed':'setup-required',
   // These fields are private to the authenticated owner, and only current reports qualify.
   ...(configured&&r.active_session===1&&r.state==='online-reported'?{host:i.managed_server_public_host.trim(),key:i.managed_server_key.trim()}:{}),
   reachability:'check-on-connect'};
 });
}

export async function selectServer(db,u,b){
 const id=b.device_id;
 if(typeof id!=='string'||!id||id.length>128||id.includes('@'))fail('Choose a device');
 const row=await db.prepare('SELECT device_info FROM br_devices WHERE user_id=? AND remote_id=?').bind(u.id,id).first();
 if(!row)fail('Device is not in your account',403);
 if(!/windows|mac|darwin|linux/i.test(String(parse(row.device_info).os||'')))fail('Choose a desktop computer');
 await db.prepare('INSERT INTO br_server_preferences(user_id,device_id,updated_at) VALUES(?,?,?) ON CONFLICT(user_id) DO UPDATE SET device_id=excluded.device_id,updated_at=excluded.updated_at WHERE device_id<>excluded.device_id').bind(u.id,id,stamp()).run();
 return {selected_device_id:id};
}
