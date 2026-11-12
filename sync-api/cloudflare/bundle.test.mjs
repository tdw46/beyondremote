import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync,mkdtempSync,rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {Miniflare,convertV4MiniflareOptions} from 'miniflare';
import bcrypt from 'bcryptjs';
test('built Worker runs password auth, presence and private denial inside workerd',async()=>{
 const dir=mkdtempSync(join(tmpdir(),'br-bundle-'));
 const mf=new Miniflare({...convertV4MiniflareOptions({compatibilityDate:'2026-11-10',compatibilityFlags:['nodejs_compat'],modules:true,script:readFileSync('.cloudflare/output/v0/workers/default/bundle/index.js','utf8'),bindings:{ENVIRONMENT:'staging',DATABASE_ROLE:'staging',BASE_URL:'https://api.test',ALLOW_SIGNUP:'0',GITHUB_CLIENT_ID:'mock',GITHUB_CLIENT_SECRET:'mock'},d1Databases:{DB:'test'},ratelimits:{LOGIN_LIMIT:{namespace_id:'21301',simple:{limit:100,period:60}}},outboundService:request=>{const u=new URL(request.url);if(u.hostname==='github.com'&&u.pathname==='/login/oauth/access_token')return Response.json({access_token:'fixture'});if(u.hostname==='api.github.com'&&u.pathname==='/user')return Response.json({id:55,login:'fixture-github'});if(u.hostname==='api.github.com'&&u.pathname==='/user/emails')return Response.json([]);throw Error('Unexpected origin request');}}),resourcePersistencePath:dir});
 try{
 const db=await mf.getD1Database('DB');for(const sql of readFileSync('schema.sql','utf8').split(';').filter(x=>x.trim()))await db.prepare(sql).run();
 await db.prepare("INSERT INTO br_users(provider_user_id,name,password_hash) VALUES('fixture','fixture',?)").bind(await bcrypt.hash('local synthetic fixture',4)).run();
 const login=await mf.dispatchFetch('https://api.test/api/login',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({username:'fixture',password:'local synthetic fixture',id:'desktop'})});assert.equal(login.status,200);const auth=await login.json();
 const open=await mf.dispatchFetch('https://api.test/api/v1/presence/open',{method:'POST',headers:{Authorization:'Bearer '+auth.access_token,'Content-Type':'application/json'},body:JSON.stringify({id:'desktop',state:'offline-reported'})});assert.equal(open.status,200);assert.equal((await db.prepare('SELECT state FROM br_presence').first()).state,'offline-reported');
 await db.prepare("INSERT INTO br_users(provider,provider_user_id,name) VALUES('github','55','fixture-github')").run();
 const authRequest=await mf.dispatchFetch('https://api.test/api/oidc/auth',{method:'POST',body:JSON.stringify({op:'github'})});const pending=await authRequest.json();
 const start=await mf.dispatchFetch(pending.url,{redirect:'manual'});const state=new URL(start.headers.get('location')).searchParams.get('state');
 const finish=await mf.dispatchFetch('https://api.test/auth/github/callback?state='+state+'&code=fixture-code');assert.equal(finish.status,200);
 const claim=await mf.dispatchFetch('https://api.test/api/oidc/auth-query?code='+pending.code);assert.equal(claim.status,200);assert.equal((await claim.json()).user.name,'fixture-github');
 const denied=await mf.dispatchFetch('https://api.test/api/peers');assert.equal(denied.status,401);assert.equal(denied.headers.get('Cache-Control'),'no-store');
 }finally{await mf.dispose();rmSync(dir,{recursive:true,force:true});}
});
