import {fail,digest,token,remoteId,stamp} from './core.mjs';
// Connection tickets are short-lived, one-use and never contain account tokens.
export async function connectRoute(db,u,path,b){
 const source=remoteId(b.source_id),target=remoteId(b.target_id);
 if(!source||!target||source===target)fail('Choose another device');
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
