import bcrypt from 'bcryptjs';
export class HttpError extends Error { constructor(message,status=400){super(message);this.status=status;} }
export const fail=(message,status=400)=>{throw new HttpError(message,status);};
export const stamp=()=>new Date().toISOString();
export const token=()=>Array.from(crypto.getRandomValues(new Uint8Array(32)),x=>x.toString(16).padStart(2,'0')).join('');
export const digest=async value=>Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256',new TextEncoder().encode(value))),x=>x.toString(16).padStart(2,'0')).join('');
export const object=value=>value&&typeof value==='object'&&!Array.isArray(value)?value:{};
export const parse=value=>value?JSON.parse(value):{};
export const remoteId=value=>String(value||'').split('@')[0].trim().slice(0,128);
export function text(value,max=256){if(typeof value!=='string'||value.length>max)fail('Invalid text');return value;}
export const paged=data=>({total:data.length,data});
export const info=()=>({email_verification:false,email_alarm_notification:false,login_device_whitelist:[],other:{}});
export const userPayload=u=>({name:u.name,display_name:u.display_name||u.name,avatar:u.avatar||'',email:u.email||'',note:'',status:u.status,is_admin:!!u.is_admin,verifier:'',info:info()});
export async function body(request){
 if(Number(request.headers.get('Content-Length')||0)>65536)fail('Request too large',413);
 const reader=request.body?.getReader();if(!reader)return {};
 const chunks=[];let size=0;try{for(;;){const {done,value}=await reader.read();if(done)break;size+=value.length;if(size>65536){await reader.cancel();fail('Request too large',413);}chunks.push(value);}}finally{reader.releaseLock();}
 const bytes=new Uint8Array(size);let at=0;for(const chunk of chunks){bytes.set(chunk,at);at+=chunk.length;}
 try{return JSON.parse(new TextDecoder().decode(bytes)||'{}');}catch{fail('Invalid JSON');}
}
export async function requireUser(db,request,provided){
 const raw=provided??request.headers.get('Authorization')?.match(/^Bearer (.+)$/i)?.[1]??'';
 if(!raw||raw.length>512)fail('Unauthorized',401);
 const hash=await digest(raw.trim());
 const user=await db.prepare('SELECT u.*,s.token_hash FROM br_sessions s JOIN br_users u ON u.id=s.user_id WHERE s.native=1 AND s.token_hash=? AND julianday(s.expires_at)>julianday(?) AND u.status=1').bind(hash,stamp()).first();
 if(!user)fail('Unauthorized',401);return user;
}
export async function limitLogin(db,env,request,key){
 if(!await env.LOGIN_LIMIT.limit({key:request.headers.get('CF-Connecting-IP')||'local'}).then(x=>x.success))fail('Please wait before trying again',429);
 const window=Math.floor(Date.now()/3600000);
 const bump=(key,max)=>db.prepare('INSERT INTO br_auth_limits(key,window,attempts) VALUES(?,?,1) ON CONFLICT(key) DO UPDATE SET attempts=CASE WHEN window=excluded.window THEN attempts+1 ELSE 1 END,window=excluded.window WHERE window<>excluded.window OR attempts<? RETURNING attempts').bind(key,window,max).first();
 const global=await bump('global',300);if(!global)fail('Please wait before trying again',429);
 if(global.attempts===1)await db.batch([
  db.prepare("DELETE FROM br_auth_limits WHERE key IN(SELECT key FROM br_auth_limits WHERE window<? LIMIT 500)").bind(window),
  db.prepare("DELETE FROM br_auth_requests WHERE code IN(SELECT code FROM br_auth_requests WHERE native=1 AND julianday(expires_at)<julianday(?) LIMIT 500)").bind(stamp()),
  db.prepare("DELETE FROM br_sessions WHERE token_hash IN(SELECT token_hash FROM br_sessions WHERE native=1 AND julianday(expires_at)<julianday(?) LIMIT 500)").bind(stamp())
 ]);
 if(!await bump(key,30))fail('Please wait before trying again',429);
}
export async function session(db,user){
 const raw=token();await db.prepare('INSERT INTO br_sessions(token_hash,user_id,expires_at,created_at,last_seen_at,native) VALUES(?,?,?,?,?,1)').bind(await digest(raw),user.id,new Date(Date.now()+90*86400000).toISOString(),stamp(),stamp()).run();return {type:'access_token',access_token:raw,user:userPayload(user)};
}
export async function passwordLogin(db,env,request,b){
 const name=text(b.username,128).trim(),password=text(b.password,1024);if(!name||!password)fail('Username and password are required');
 await limitLogin(db,env,request,'name:'+await digest(name));
 let u=await db.prepare("SELECT * FROM br_users WHERE provider='password' AND provider_user_id=?").bind(name).first();
 if(!u){
  if(env.ALLOW_SIGNUP!=='1')fail('Signup unavailable on this preview',403);
  const hash=await bcrypt.hash(password,12);
  await db.prepare("INSERT INTO br_users(provider,provider_user_id,name,password_hash) VALUES('password',?,?,?) ON CONFLICT DO NOTHING").bind(name,name,hash).run();
  u=await db.prepare("SELECT * FROM br_users WHERE provider='password' AND provider_user_id=?").bind(name).first();
 }
 if(!u||u.status!==1||!u.password_hash||!await bcrypt.compare(password,u.password_hash.replace(/^\$2y\$/,'$2b$')))fail('Invalid username or password',401);
 return u;
}
export function deviceInfo(value){const v={...object(value)};if(v.managed_server_running!==true||!String(v.managed_server_public_host||'').trim()||!String(v.managed_server_key||'').trim()){delete v.managed_server_running;delete v.managed_server_public_host;delete v.managed_server_key;}return v;}
export function platform(os){os=String(os||'').toLowerCase();if(/mac|darwin/.test(os))return 'Mac OS';if(/win/.test(os))return 'Windows';if(/ios|iphone|ipad/.test(os))return 'iOS';if(/android/.test(os))return 'Android';if(/linux/.test(os))return 'Linux';return os;}
export function tagColor(name){let n=0;for(const ch of new TextEncoder().encode(name))n=(n*31+ch)&0xffffff;return (0xff000000+n)|0;}
export async function registerDevice(db,u,b){
 const id=remoteId(b.id);if(!id)return;
 const existing=await db.prepare('SELECT d.device_info,p.data FROM br_devices d LEFT JOIN br_personal_peers p ON p.user_id=d.user_id AND p.peer_id=d.remote_id WHERE d.user_id=? AND d.remote_id=?').bind(u.id,id).first();
 const data=deviceInfo(Object.keys(object(b.deviceInfo)).length?b.deviceInfo:parse(existing?.device_info));
 const old=parse(existing?.data),name=String(data.name||'');
 const peer={id,username:'',hostname:name||old.hostname||'',platform:platform(data.os||old.platform),alias:old.alias||name,tags:[...new Set([...(Array.isArray(old.tags)?old.tags:[]),'My devices'])],note:old.note||'Signed in with Beyond Remote',same_server:true,local_ips:data.local_ips||[],managed_server_public_host:data.managed_server_public_host||'',managed_server_key:data.managed_server_key||''};
 // SQL merge preserves concurrently edited aliases/tags; only supplied device fields change.
 const saved={...old,...peer};
 await db.batch([
  db.prepare('INSERT INTO br_devices(user_id,remote_id,uuid,device_info,last_seen_at) VALUES(?,?,?,?,?) ON CONFLICT(user_id,remote_id) DO UPDATE SET uuid=CASE WHEN excluded.uuid<>\'\' THEN excluded.uuid ELSE br_devices.uuid END,device_info=excluded.device_info,last_seen_at=excluded.last_seen_at WHERE (excluded.uuid<>\'\' AND br_devices.uuid<>excluded.uuid) OR br_devices.device_info<>excluded.device_info').bind(u.id,id,String(b.uuid||'').slice(0,256),JSON.stringify(data),stamp()),
  db.prepare("INSERT INTO br_personal_peers(user_id,peer_id,data,updated_at) VALUES(?,?,?,?) ON CONFLICT(user_id,peer_id) DO UPDATE SET data=json_patch(br_personal_peers.data,json_remove(excluded.data,'$.alias','$.tags','$.note')),updated_at=excluded.updated_at WHERE json_extract(br_personal_peers.data,'$.hostname') IS NOT json_extract(excluded.data,'$.hostname') OR json_extract(br_personal_peers.data,'$.platform') IS NOT json_extract(excluded.data,'$.platform') OR json_extract(br_personal_peers.data,'$.local_ips') IS NOT json_extract(excluded.data,'$.local_ips') OR json_extract(br_personal_peers.data,'$.managed_server_public_host') IS NOT json_extract(excluded.data,'$.managed_server_public_host') OR json_extract(br_personal_peers.data,'$.managed_server_key') IS NOT json_extract(excluded.data,'$.managed_server_key')").bind(u.id,id,JSON.stringify(saved),stamp()),
  db.prepare('INSERT OR IGNORE INTO br_personal_tags(user_id,name,color,updated_at) VALUES(?,?,?,?)').bind(u.id,'My devices',tagColor('My devices'),stamp())
 ]);
}
export async function strategy(db,u,env,modified=0){
 const opts={...object(env.CLIENT_CONFIG_JSON?parse(env.CLIENT_CONFIG_JSON):env.CLIENT_CONFIG_OPTIONS)};
 const row=await db.prepare("SELECT d.device_info FROM br_devices d LEFT JOIN br_presence p ON p.user_id=d.user_id AND p.remote_id=d.remote_id WHERE d.user_id=? AND json_extract(d.device_info,'$.managed_server_running')=1 AND COALESCE(json_extract(d.device_info,'$.managed_server_public_host'),'')<>'' AND COALESCE(p.state,'unknown')<>'offline-reported' ORDER BY COALESCE(p.updated_at,d.last_seen_at) DESC LIMIT 1").bind(u.id).first();
 if(row){const v=parse(row.device_info),host=String(v.managed_server_public_host).trim();opts['custom-rendezvous-server']=host.includes(':')?host:host+':21116';opts['relay-server']=host.includes(':')?host:host+':21117';opts.key=String(v.managed_server_key||'');}
 // Never send the staging client back to production through an inherited option.
 opts['api-server']=env.BASE_URL;
 const str=JSON.stringify(opts).replace(/\//g,'\\/').replace(/[\u007f-\uffff]/g,c=>'\\u'+c.charCodeAt(0).toString(16).padStart(4,'0'));
 let crc=0xffffffff;for(const b of new TextEncoder().encode(str)){crc^=b;for(let n=0;n<8;n++)crc=(crc>>>1)^((crc&1)?0xedb88320:0);}crc=(crc^0xffffffff)>>>0;
 return {modified_at:crc,...(Number(modified)!==crc?{strategy:{config_options:opts}}:{}),sync_capabilities:{presence:'events-v1',sysinfo:false}};
}
