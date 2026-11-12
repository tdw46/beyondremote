import {HttpError,fail,body,digest,requireUser,passwordLogin,registerDevice,session,strategy,userPayload,remoteId,paged,stamp} from './core.mjs';
import {connectRoute} from './connect.mjs';
import {accountServers,selectServer} from './servers.mjs';
import {dataRoute,presence} from './data.mjs';
import {oauth,providers} from './oauth.mjs';
const baseHeaders={'Cache-Control':'no-store','X-Content-Type-Options':'nosniff','X-Robots-Tag':'noindex, nofollow','Referrer-Policy':'no-referrer','Content-Security-Policy':"default-src 'none'; frame-ancestors 'none'"};
const json=(data,status=200)=>new Response(typeof data==='string'?data:JSON.stringify(data),{status,headers:{'Content-Type':typeof data==='string'?'text/plain; charset=utf-8':'application/json; charset=utf-8'}});
async function handle(request,env){
 const url=new URL(request.url),path=url.pathname,method=request.method;
 if(env.ENVIRONMENT!=='staging'||env.DATABASE_ROLE!=='staging'||url.origin!==env.BASE_URL)fail('Preview unavailable',503);
 if(method==='OPTIONS')return new Response(null,{status:204,headers:{'Access-Control-Allow-Methods':'GET,POST,PUT,DELETE,OPTIONS','Access-Control-Allow-Headers':'Content-Type,Authorization'}});
 if(path==='/robots.txt'&&['GET','HEAD'].includes(method))return new Response(method==='HEAD'?null:'User-agent: *\nDisallow: /\n',{headers:{'Content-Type':'text/plain; charset=utf-8','Cache-Control':'public, max-age=3600'}});
 if(method==='GET'&&path==='/api/beyondremote/ping')return json({ok:true,service:'Beyond Remote Sync API',db:'d1',providers:providers(env),sync_capabilities:{presence:'events-v1',sysinfo:false}});
 if(method==='GET'&&path==='/api/login-options')return json(providers(env).map(x=>'oidc/'+x));
 if(!path.startsWith('/api/')&&!path.startsWith('/auth/'))fail('Not found',404);
 const db=env.DB.withSession('first-primary');
 const b=['POST','PUT','DELETE'].includes(method)?await body(request):{};
 if(path==='/api/login'&&method==='POST'){const u=await passwordLogin(db,env,request,b);await registerDevice(db,u,b);return json(await session(db,u));}
 if((path==='/api/oidc/auth'&&method==='POST')||(path==='/api/oidc/auth-query'&&method==='GET')||(/^\/auth\/(github|google)\/(start|callback)$/.test(path)&&method==='GET')){const r=await oauth(db,env,request,path,b);return r instanceof Response?r:json(r);}
 if(path==='/api/logout'&&method==='POST'){
  const raw=request.headers.get('Authorization')?.match(/^Bearer (.+)$/i)?.[1]||'';if(raw&&raw.length<=512){const hash=await digest(raw.trim());await db.batch([db.prepare("UPDATE br_presence SET state='offline-reported',updated_at=? WHERE token_hash=? AND state<>'offline-reported'").bind(stamp(),hash),db.prepare('DELETE FROM br_sessions WHERE token_hash=?').bind(hash)]);}return json('');
 }
 const u=await requireUser(db,request);
 if(method==='GET'&&path==='/api/v1/connection-servers')return json({devices:await accountServers(db,u)});
 if(method==='POST'&&path==='/api/v1/connection-servers/select')return json(await selectServer(db,u,b));
 if(method==='POST'&&['/api/v1/connect/grant','/api/v1/connect/authorize','/api/v1/connect/route'].includes(path))return json(await connectRoute(db,u,path,b));
 if(path==='/api/currentUser'&&method==='POST'){await registerDevice(db,u,b);return json({...userPayload(u),...await strategy(db,u,env,b.modified_at)});}
 if(path==='/api/heartbeat'&&method==='POST'){await registerDevice(db,u,b);return json(await strategy(db,u,env,b.modified_at));}
 if(method==='POST'&&['/api/v1/presence/open','/api/v1/presence/event'].includes(path))return json(await presence(db,u,env,path,b));
 if(method==='POST'&&path==='/api/beyondremote/device-auth'){
  const target=await requireUser(db,request,String(b.target_access_token||''));if(u.id!==target.id)fail('Forbidden',403);
  const sourceId=remoteId(b.source_id),targetId=remoteId(b.target_id);if(!sourceId||!targetId)fail('Missing device auth fields');
  const rows=(await db.prepare('SELECT remote_id FROM br_devices WHERE user_id=? AND remote_id IN(?,?)').bind(u.id,sourceId,targetId).all()).results;
  if(!rows.some(r=>r.remote_id===sourceId)||!rows.some(r=>r.remote_id===targetId))fail('Device is not registered to this account',403);
  return json({ok:true,source_id:sourceId,target_id:targetId});
 }
 if(path==='/api/users'&&method==='GET')return json(paged([userPayload(u)]));
 return json(await dataRoute(db,u,env,method,path,b));
}
export default {async fetch(request,env){
 let r;try{r=await handle(request,env);}catch(e){r=json({error:e instanceof HttpError?e.message:'Internal server error'},e instanceof HttpError?e.status:500);if(!(e instanceof HttpError))console.error(JSON.stringify({event:'api_error',path:new URL(request.url).pathname}));}
 const headers=new Headers(r.headers);for(const [k,v] of Object.entries(baseHeaders))if(!headers.has(k))headers.set(k,v);
 if(request.headers.get('Origin')===env.BASE_URL)headers.set('Access-Control-Allow-Origin',env.BASE_URL);
 return new Response(request.method==='HEAD'?null:r.body,{status:r.status,headers});
}};
