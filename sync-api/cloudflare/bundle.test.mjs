import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync,mkdtempSync,rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {Miniflare,convertV4MiniflareOptions} from 'miniflare';
import bcrypt from 'bcryptjs';
test('built Worker runs password auth, presence and private denial inside workerd',async()=>{
 const dir=mkdtempSync(join(tmpdir(),'br-bundle-'));
 const mf=new Miniflare({...convertV4MiniflareOptions({compatibilityDate:'2026-11-10',compatibilityFlags:['nodejs_compat'],modules:true,script:readFileSync('.cloudflare/output/v0/workers/default/bundle/index.js','utf8'),bindings:{ENVIRONMENT:'staging',DATABASE_ROLE:'staging',BASE_URL:'https://api.test',ALLOW_SIGNUP:'0',ACCOUNT_SERVER_SELECTION:'1',GITHUB_CLIENT_ID:'mock',GITHUB_CLIENT_SECRET:'mock',GOOGLE_CLIENT_ID:'mock-google',GOOGLE_CLIENT_SECRET:'mock-google'},d1Databases:{DB:'test'},ratelimits:{LOGIN_LIMIT:{namespace_id:'21301',simple:{limit:100,period:60}}},outboundService:request=>{const u=new URL(request.url);if(u.hostname==='github.com'&&u.pathname==='/login/oauth/access_token')return Response.json({access_token:'fixture'});if(u.hostname==='api.github.com'&&u.pathname==='/user')return Response.json({id:55,login:'fixture-github'});if(u.hostname==='api.github.com'&&u.pathname==='/user/emails')return Response.json([]);if(u.hostname==='oauth2.googleapis.com'&&u.pathname==='/token')return Response.json({access_token:'google-fixture'});if(u.hostname==='openidconnect.googleapis.com'&&u.pathname==='/v1/userinfo')return Response.json({sub:'google-55',email:'same@example.test',email_verified:true,name:'Google fixture'});throw Error('Unexpected origin request');}}),resourcePersistencePath:dir});
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
 await db.prepare("INSERT INTO br_users(provider,provider_user_id,name,email) VALUES('google','google-55','fixture-google','same@example.test')").run();
 const googleStart=await mf.dispatchFetch('https://api.test/api/oidc/auth',{method:'POST',body:JSON.stringify({op:'google'})});const googlePending=await googleStart.json();
 const googleRedirect=await mf.dispatchFetch(googlePending.url,{redirect:'manual'});const googleURL=new URL(googleRedirect.headers.get('location'));
 assert.equal(googleURL.origin,'https://accounts.google.com');assert.equal(googleURL.searchParams.get('scope'),'openid email profile');
 const googleState=googleURL.searchParams.get('state');
 const wrongProvider=await mf.dispatchFetch('https://api.test/auth/github/callback?state='+googleState+'&code=fixture-code');assert.equal(wrongProvider.status,400);
 const googleFinish=await mf.dispatchFetch('https://api.test/auth/google/callback?state='+googleState+'&code=fixture-code');assert.equal(googleFinish.status,200);
 const googleClaim=await mf.dispatchFetch('https://api.test/api/oidc/auth-query?code='+googlePending.code);assert.equal((await googleClaim.json()).user.name,'fixture-google');
 const googleReplay=await mf.dispatchFetch('https://api.test/auth/google/callback?state='+googleState+'&code=fixture-code');assert.equal(googleReplay.status,400);
 const legacyCode='e'.repeat(64);await db.prepare("INSERT INTO br_auth_requests(code,op,expires_at,auth_body) VALUES(?,'google','2099-01-01',?)").bind(legacyCode,JSON.stringify({access_token:'must-not-deliver'})).run();
 assert.equal((await mf.dispatchFetch('https://api.test/api/oidc/auth-query?code='+legacyCode)).status,404);
 const denied=await mf.dispatchFetch('https://api.test/api/peers');assert.equal(denied.status,401);assert.equal(denied.headers.get('Cache-Control'),'no-store');
 }finally{await mf.dispose();rmSync(dir,{recursive:true,force:true});}
});
