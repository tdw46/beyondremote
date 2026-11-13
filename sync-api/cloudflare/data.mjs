import {fail,object,parse,remoteId,stamp,paged,platform,tagColor,registerDevice,strategy,token} from './core.mjs';
import {wakeProfiles} from './wake.mjs';
const bounded=rows=>{if(rows.length>10000)fail('Account list requires pagination',413);return rows;};
const list=b=>{if(!Array.isArray(b)||b.length>500||b.some(x=>typeof x!=='string'||x.length>256))fail('Invalid list');return b;};
const savePeer=(db,uid,p,merge)=>{
 const id=String(p.id||'').trim();if(!id||id.length>128)fail('Missing peer id');
 // json_set preserves explicit nulls and replaces nested values like PHP array_replace.
 const entries=Object.entries(p);if(entries.length>80)fail('Too many peer fields');
 const expression=merge?'json_set(br_personal_peers.data,'+entries.map(()=>"?,json(?)").join(',')+')':'excluded.data';
 const args=[uid,id,JSON.stringify(p),stamp()];
 if(merge)for(const [key,value] of entries){if(!/^[a-zA-Z0-9_]{1,80}$/.test(key))fail('Invalid peer field');args.push('$.'+key,JSON.stringify(value));}
 return db.prepare(`INSERT INTO br_personal_peers(user_id,peer_id,data,updated_at) VALUES(?,?,?,?) ON CONFLICT(user_id,peer_id) DO UPDATE SET data=${expression},updated_at=excluded.updated_at`).bind(...args);
};
export async function presence(db,u,env,path,b){
 const id=remoteId(b.id);if(!id)fail('Missing device id');
 if(path.endsWith('/open')){
  await registerDevice(db,u,b);const generation=token(),state=['online-reported','offline-reported','unknown'].includes(b.state)?b.state:'online-reported';
  await db.prepare("INSERT INTO br_presence(user_id,remote_id,generation,token_hash,sequence,state,payload,updated_at) VALUES(?,?,?,?,0,?,'{}',?) ON CONFLICT(user_id,remote_id) DO UPDATE SET generation=excluded.generation,token_hash=excluded.token_hash,sequence=0,state=excluded.state,payload='{}',updated_at=excluded.updated_at").bind(u.id,id,generation,u.token_hash,state,stamp()).run();
  return {generation,sequence:0,...await strategy(db,u,env,b.modified_at)};
 }
 if(!['online-reported','offline-reported','unknown'].includes(b.state)||!Number.isSafeInteger(b.sequence)||b.sequence<1||b.sequence>2147483647||typeof b.generation!=='string'||b.generation.length!==64)fail('Invalid presence event');
 const payload=JSON.stringify(object(b.payload));
 const r=await db.prepare('UPDATE br_presence SET sequence=?,state=?,payload=?,updated_at=? WHERE user_id=? AND remote_id=? AND generation=? AND token_hash=? AND sequence<? RETURNING sequence').bind(b.sequence,b.state,payload,stamp(),u.id,id,b.generation,u.token_hash,b.sequence).first();
 if(!r){const current=await db.prepare('SELECT sequence,state,payload FROM br_presence WHERE user_id=? AND remote_id=? AND generation=? AND token_hash=?').bind(u.id,id,b.generation,u.token_hash).first();if(!current||current.sequence!==b.sequence||current.state!==b.state||current.payload!==payload)fail('Stale presence generation or sequence',409);}
 return {accepted:true,sequence:b.sequence,...await strategy(db,u,env,b.modified_at)};
}
export async function dataRoute(db,u,env,method,path,b){
 if(path==='/api/ab'&&method==='GET'||path==='/api/ab/get'&&method==='POST'){
  const row=await db.prepare('SELECT data,updated_at FROM br_legacy_address_books WHERE user_id=?').bind(u.id).first();return {data:row?.data||'{"tags":[],"peers":[],"tag_colors":"{}"}',updated_at:row?Date.parse(row.updated_at):Date.now()};
 }
 if(path==='/api/ab'&&method==='POST'){
  if(typeof b.data!=='string'||b.data.length>60000)fail('Invalid address book');let data;try{data=JSON.parse(b.data);}catch{fail('Invalid address book');}
  const peers=Array.isArray(data.peers)?data.peers:[],tags=Array.isArray(data.tags)?data.tags:[];if(peers.length+tags.length>500)fail('Address book batch too large',413);
  let colors=data.tag_colors||{};try{if(typeof colors==='string')colors=JSON.parse(colors);}catch{fail('Invalid tag colors');}
  const statements=[db.prepare('INSERT INTO br_legacy_address_books(user_id,data,updated_at) VALUES(?,?,?) ON CONFLICT(user_id) DO UPDATE SET data=excluded.data,updated_at=excluded.updated_at WHERE br_legacy_address_books.data<>excluded.data').bind(u.id,b.data,stamp())];
  for(const peer of peers)statements.push(savePeer(db,u.id,object(peer),true));
  for(const name of list(tags))statements.push(db.prepare('INSERT OR IGNORE INTO br_personal_tags(user_id,name,color,updated_at) VALUES(?,?,?,?)').bind(u.id,name,Number(colors[name]??tagColor(name)),stamp()));
  await db.batch(statements);return '';
 }
 if(method==='POST'&&path==='/api/ab/settings')return {max_peer_one_ab:0};
 if(method==='POST'&&path==='/api/ab/personal')return {guid:'personal'};
 if(method==='POST'&&path==='/api/ab/shared/profiles'||method==='GET'&&path==='/api/device-group/accessible')return paged([]);
 if(method==='POST'&&path==='/api/ab/peers'){
  const rows=bounded((await db.prepare('SELECT p.data,d.device_info FROM br_personal_peers p LEFT JOIN br_devices d ON d.user_id=p.user_id AND d.remote_id=p.peer_id WHERE p.user_id=? ORDER BY p.peer_id LIMIT 10001').bind(u.id).all()).results);
  return paged(rows.map(r=>{const p=parse(r.data),i=parse(r.device_info);return {...p,platform:p.platform||platform(i.os),hostname:p.hostname||i.name||'',alias:p.alias||i.name||''};}));
 }
 if(method==='GET'&&path==='/api/peers'){
  const rows=bounded((await db.prepare('SELECT d.remote_id,d.device_info,p.data,s.state,s.updated_at FROM br_devices d LEFT JOIN br_personal_peers p ON p.user_id=d.user_id AND p.peer_id=d.remote_id LEFT JOIN br_presence s ON s.user_id=d.user_id AND s.remote_id=d.remote_id WHERE d.user_id=? ORDER BY d.last_seen_at DESC LIMIT 10001').bind(u.id).all()).results);
  return paged(rows.map(r=>{const i=parse(r.device_info),p=parse(r.data),common={local_ips:i.local_ips||[],managed_server_public_host:i.managed_server_public_host||'',managed_server_key:i.managed_server_key||''};return {id:r.remote_id,wake_enabled:wakeProfiles(i).length>0,connection_protocol:i.account_connection==='ticket-v1'?'ticket-v1':'legacy',status:1,user:u.name,user_name:u.display_name||u.name,note:'',...common,presence:{state:r.state||'unknown',reported_at:r.updated_at||null,reachability:'check-on-connect'},info:{username:'',os:i.os||'',device_name:p.alias||i.name||'',...common}};}));
 }
 if(method==='POST'&&/^\/api\/ab\/tags\/[^/]+$/.test(path))return (await db.prepare('SELECT name,color FROM br_personal_tags WHERE user_id=? ORDER BY name LIMIT 10001').bind(u.id).all()).results;
 if((method==='POST'&&/^\/api\/ab\/peer\/add\/[^/]+$/.test(path))||(method==='PUT'&&/^\/api\/ab\/peer\/update\/[^/]+$/.test(path))){await savePeer(db,u.id,object(b),method==='PUT').run();return '';}
 if(method==='DELETE'&&/^\/api\/ab\/peer\/[^/]+$/.test(path)){const ids=list(b);if(ids.length)await db.batch(ids.map(id=>db.prepare('DELETE FROM br_personal_peers WHERE user_id=? AND peer_id=?').bind(u.id,id)));return '';}
 if(method==='POST'&&/^\/api\/ab\/tag\/add\/[^/]+$/.test(path)){
  const name=String(b.name||'').trim();if(!name||name.length>256||!Number.isSafeInteger(Number(b.color??tagColor(name))))fail('Invalid tag');
  await db.prepare('INSERT INTO br_personal_tags(user_id,name,color,updated_at) VALUES(?,?,?,?) ON CONFLICT(user_id,name) DO UPDATE SET color=excluded.color,updated_at=excluded.updated_at WHERE br_personal_tags.color<>excluded.color').bind(u.id,name,Number(b.color??tagColor(name)),stamp()).run();return '';
 }
 if(method==='PUT'&&/^\/api\/ab\/tag\/update\/[^/]+$/.test(path)){if(typeof b.name!=='string'||!Number.isSafeInteger(b.color))fail('Invalid tag');await db.prepare('UPDATE br_personal_tags SET color=?,updated_at=? WHERE user_id=? AND name=? AND color<>?').bind(b.color,stamp(),u.id,b.name,b.color).run();return '';}
 if(method==='PUT'&&/^\/api\/ab\/tag\/rename\/[^/]+$/.test(path)){
  if(!b.old||!b.new||typeof b.old!=='string'||typeof b.new!=='string'||b.new.length>256)fail('Missing tag name');
  await db.batch([
   db.prepare('UPDATE br_personal_tags SET name=?,updated_at=? WHERE user_id=? AND name=?').bind(b.new,stamp(),u.id,b.old),
   db.prepare("UPDATE br_personal_peers SET data=json_set(data,'$.tags',json((SELECT json_group_array(CASE WHEN value=? THEN ? ELSE value END) FROM json_each(br_personal_peers.data,'$.tags')))),updated_at=? WHERE user_id=? AND EXISTS(SELECT 1 FROM json_each(br_personal_peers.data,'$.tags') WHERE value=?)").bind(b.old,b.new,stamp(),u.id,b.old)
  ]);return '';
 }
 if(method==='DELETE'&&/^\/api\/ab\/tag\/[^/]+$/.test(path)){
  const names=JSON.stringify(list(b));await db.batch([
   db.prepare('DELETE FROM br_personal_tags WHERE user_id=? AND name IN(SELECT value FROM json_each(?))').bind(u.id,names),
   db.prepare("UPDATE br_personal_peers SET data=json_set(data,'$.tags',json((SELECT json_group_array(value) FROM json_each(br_personal_peers.data,'$.tags') WHERE value NOT IN(SELECT value FROM json_each(?))))),updated_at=? WHERE user_id=? AND EXISTS(SELECT 1 FROM json_each(br_personal_peers.data,'$.tags') WHERE value IN(SELECT value FROM json_each(?)))").bind(names,stamp(),u.id,names)
  ]);return '';
 }
 fail('Not found',404);
}
