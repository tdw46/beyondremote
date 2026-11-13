import {fail,token,digest,stamp,session,object,parse,registerDevice,limitLogin,body} from './core.mjs';
export const providers=env=>['github','google'].filter(p=>env[p.toUpperCase()+'_CLIENT_ID']&&env[p.toUpperCase()+'_CLIENT_SECRET']);
async function providerJSON(url,init={}){
 const r=await fetch(url,{...init,redirect:'manual',signal:AbortSignal.timeout(15000)});if(!r.ok)fail('OAuth provider request failed',400);
 return body(r);
}
export async function oauth(db,env,request,path,b){
 const url=new URL(request.url);
 if(path==='/api/oidc/auth'){
  const op=String(b.op||'').toLowerCase();if(!providers(env).includes(op))fail('OAuth provider is not configured');
  await limitLogin(db,env,request,'oauth');const code=token();
  await db.prepare('INSERT INTO br_auth_requests(code,op,remote_id,uuid,device_info,expires_at,created_at,native) VALUES(?,?,?,?,?,?,?,1)').bind(code,op,String(b.id||'').slice(0,128),String(b.uuid||'').slice(0,256),JSON.stringify(object(b.deviceInfo)),new Date(Date.now()+600000).toISOString(),stamp()).run();
  return {code,url:env.BASE_URL+'/auth/'+op+'/start?code='+code};
 }
 if(path==='/api/oidc/auth-query'){
  const code=url.searchParams.get('code')||'';if(code.length!==64)fail('No authed oidc is found',404);
  // Three bounded deliveries tolerate a lost response; the random code expires in ten minutes.
  const r=await db.prepare('UPDATE br_auth_requests SET delivered=delivered+1 WHERE native=1 AND code=? AND julianday(expires_at)>julianday(?) AND auth_body IS NOT NULL AND delivered<3 RETURNING auth_body').bind(code,stamp()).first();
  if(!r)fail('No authed oidc is found',404);return parse(r.auth_body);
 }
 const match=path.match(/^\/auth\/(github|google)\/(start|callback)$/);if(!match)fail('Not found',404);
 const [,provider,action]=match;if(!providers(env).includes(provider))fail('OAuth provider is not configured');
 const client=env[provider.toUpperCase()+'_CLIENT_ID'],secret=env[provider.toUpperCase()+'_CLIENT_SECRET'];
 const redirect=env.BASE_URL+'/auth/'+provider+'/callback';
 if(action==='start'){
  const code=url.searchParams.get('code')||'',nonce=token();if(code.length!==64)fail('OAuth request not found',404);
  const state=code+'.'+nonce;
  const r=await db.prepare('UPDATE br_auth_requests SET state_hash=? WHERE native=1 AND code=? AND op=? AND claimed=0 AND julianday(expires_at)>julianday(?) RETURNING code').bind(await digest(state),code,provider,stamp()).first();if(!r)fail('OAuth request not found',404);
  const args=new URLSearchParams({client_id:client,redirect_uri:redirect,state,scope:provider==='github'?'read:user user:email':'openid email profile',response_type:'code'});
  return new Response(null,{status:302,headers:{Location:(provider==='github'?'https://github.com/login/oauth/authorize?':'https://accounts.google.com/o/oauth2/v2/auth?')+args}});
 }
 const state=url.searchParams.get('state')||'',code=url.searchParams.get('code')||'';if(!/^[a-f0-9]{64}\.[a-f0-9]{64}$/.test(state)||!code||code.length>2048)fail('Invalid OAuth state');
 const req=await db.prepare('UPDATE br_auth_requests SET claimed=1 WHERE native=1 AND code=? AND op=? AND state_hash=? AND claimed=0 AND julianday(expires_at)>julianday(?) RETURNING *').bind(state.split('.')[0],provider,await digest(state),stamp()).first();if(!req)fail('Invalid or expired OAuth state');
 const exchange=await providerJSON(provider==='github'?'https://github.com/login/oauth/access_token':'https://oauth2.googleapis.com/token',{method:'POST',headers:{Accept:'application/json','Content-Type':'application/x-www-form-urlencoded'},body:new URLSearchParams({client_id:client,client_secret:secret,code,redirect_uri:redirect,grant_type:'authorization_code'})});
 if(!exchange.access_token)fail('OAuth login failed');
 const headers={Authorization:'Bearer '+exchange.access_token,Accept:'application/json','User-Agent':'BeyondRemote'};
 const profile=await providerJSON(provider==='github'?'https://api.github.com/user':'https://openidconnect.googleapis.com/v1/userinfo',{headers});
 const id=String(provider==='github'?profile.id:profile.sub);if(!id||id==='undefined')fail('OAuth identity missing');
 let email='';if(provider==='github'){const emails=await providerJSON('https://api.github.com/user/emails',{headers});email=emails.find(e=>e.primary&&e.verified)?.email||'';}else if(profile.email_verified===true)email=profile.email||'';
 let u=await db.prepare('SELECT * FROM br_users WHERE provider=? AND provider_user_id=?').bind(provider,id).first();
 if(u&&u.status!==1)fail('Unauthorized',401);
 if(!u){if(env.ALLOW_SIGNUP!=='1')fail('Signup unavailable on this preview',403);const base=String(profile.login||email.split('@')[0]||provider+'-'+id).toLowerCase().replace(/[^a-z0-9_.-]+/g,'-').slice(0,100)||provider+'-'+id;
  for(let n=0;n<20&&!u;n++){
   await db.prepare('INSERT INTO br_users(provider,provider_user_id,name,display_name,email,avatar) VALUES(?,?,?,?,?,?) ON CONFLICT DO NOTHING').bind(provider,id,n?base+'-'+(n+1):base,String(profile.name||base),email,String(profile.avatar_url||profile.picture||'')).run();
   u=await db.prepare('SELECT * FROM br_users WHERE provider=? AND provider_user_id=?').bind(provider,id).first();
  }if(!u)fail('Could not create account',409);
 }
 await registerDevice(db,u,{id:req.remote_id,uuid:req.uuid,deviceInfo:parse(req.device_info)});
 const auth=await session(db,u);await db.prepare('UPDATE br_auth_requests SET auth_body=? WHERE code=?').bind(JSON.stringify(auth),req.code).run();
 return new Response('<!doctype html><title>BeyondRemote</title><h1>Signed in</h1><p>Return to BeyondRemote to continue.</p>',{headers:{'Content-Type':'text/html; charset=utf-8'}});
}
