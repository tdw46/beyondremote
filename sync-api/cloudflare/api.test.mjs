import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync,mkdtempSync,rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {Miniflare,convertV4MiniflareOptions} from 'miniflare';
import bcrypt from 'bcryptjs';
import worker from './worker.mjs';
import {digest} from './core.mjs';

test('native API privacy, compatibility, concurrency and bounded writes',async t=>{
 const dir=mkdtempSync(join(tmpdir(),'br-native-test-'));
 const mf=new Miniflare({...convertV4MiniflareOptions({modules:true,script:'export default {fetch(){return new Response("test")}}',compatibilityDate:'2026-11-10',d1Databases:{DB:'test'}}),resourcePersistencePath:dir});
 try{
 const db=await mf.getD1Database('DB');
 for(const sql of readFileSync(new URL('./schema.sql',import.meta.url),'utf8').split(';').map(x=>x.trim()).filter(Boolean))await db.prepare(sql).run();
 const password='Synthetic test password only!'; const hash=(await bcrypt.hash(password,4)).replace('$2b$','$2y$');
 for(let i=1;i<=102;i++)await db.prepare("INSERT INTO br_users(id,provider_user_id,name,password_hash) VALUES(?,?,?,?)").bind(i,'test'+i,'test'+i,hash).run();
 const meter={statements:0,rowsRead:0,rowsWritten:0};
 const count=r=>{meter.statements++;meter.rowsRead+=r.meta?.rows_read||0;meter.rowsWritten+=r.meta?.rows_written||0;return r;};
 const wrapStatement=raw=>({raw,bind(...v){return wrapStatement(raw.bind(...v));},async first(col){const r=count(await raw.all());return col?r.results[0]?.[col]??null:r.results[0]??null;},async all(){return count(await raw.all());},async run(){return count(await raw.run());}});
 const measuredDB={withSession(){return this;},prepare(sql){return wrapStatement(db.prepare(sql));},async batch(statements){return (await db.batch(statements.map(s=>s.raw))).map(count);}};
 const env={DB:measuredDB,ENVIRONMENT:'staging',DATABASE_ROLE:'staging',BASE_URL:'https://api.test',ALLOW_SIGNUP:'0',LOGIN_LIMIT:{limit:async()=>({success:true})}};
 const call=async(path,{method='POST',token,body={}}={})=>{const r=await worker.fetch(new Request('https://api.test'+path,{method,headers:{'Content-Type':'application/json',...(token?{Authorization:'Bearer '+token}:{})},...(['GET','HEAD'].includes(method)?{}:{body:JSON.stringify(body)})}),env);let text=await r.text();return {status:r.status,headers:r.headers,data:text?JSON.parse(text):null};};
 const login=async n=>{const r=await call('/api/login',{body:{username:'test'+n,password,id:'device'+n,deviceInfo:{name:'Computer '+n,os:'Darwin'}}});assert.equal(r.status,200);return r.data.access_token;};
 const one=await login(1),two=await login(2);
 await t.test('legacy bcrypt and native session separation',async()=>{
 assert.equal((await call('/api/currentUser',{token:one})).data.name,'test1');
 await db.prepare('INSERT INTO br_sessions(token_hash,user_id,expires_at) VALUES(?,?,?)').bind(await digest('legacy'),1,'2099-01-01').run();
 assert.equal((await call('/api/currentUser',{token:'legacy'})).status,401);
 assert.equal((await call('/api/currentUser')).status,401);
 });
 await t.test('owner scoped data and device authentication',async()=>{
 const r=await call('/api/peers',{method:'GET',token:one});assert.equal(r.data.total,1);assert.equal(r.data.data[0].id,'device1');assert.equal(r.data.data[0].connection_protocol,'legacy');
 await call('/api/currentUser',{token:one,body:{id:'device1',deviceInfo:{account_connection:'ticket-v1'}}});
 assert.equal((await call('/api/peers',{method:'GET',token:one})).data.data[0].connection_protocol,'ticket-v1');
 assert.equal((await call('/api/beyondremote/device-auth',{token:one,body:{source_id:'device1',target_id:'device2',target_access_token:two}})).status,403);
 assert.equal((await call('/api/beyondremote/device-auth',{token:one,body:{source_id:'device1',target_id:'device1',target_access_token:one}})).status,200);
 });
 await t.test('event generations, sequencing, idempotency and stale rejection',async()=>{
 const open=await call('/api/v1/presence/open',{token:one,body:{id:'device1'}});assert.equal(open.status,200);
 const event={id:'device1',generation:open.data.generation,sequence:2,state:'offline-reported'};
 assert.equal((await call('/api/v1/presence/event',{token:one,body:event})).status,200);
 assert.equal((await call('/api/v1/presence/event',{token:one,body:event})).status,200);
 assert.equal((await call('/api/v1/presence/event',{token:one,body:{...event,sequence:1,state:'online-reported'}})).status,409);
 assert.equal((await call('/api/v1/presence/event',{token:two,body:event})).status,409);
 await call('/api/v1/presence/open',{token:one,body:{id:'device1'}});
 assert.equal((await call('/api/v1/presence/event',{token:one,body:{...event,sequence:3}})).status,409);
 });
 await t.test('concurrent partial peer edits preserve fields and explicit null',async()=>{
 await call('/api/ab/peer/add/personal',{token:one,body:{id:'friend',alias:'Original',note:'Keep',tags:['Blue'],custom:{a:1}}});
 const responses=await Promise.all([call('/api/ab/peer/update/personal',{method:'PUT',token:one,body:{id:'friend',alias:'Changed'}}),call('/api/ab/peer/update/personal',{method:'PUT',token:one,body:{id:'friend',note:null,custom:{b:2}}})]);
 assert.ok(responses.every(r=>r.status===200));
 const p=(await call('/api/ab/peers',{token:one})).data.data.find(p=>p.id==='friend');assert.equal(p.alias,'Changed');assert.equal(p.note,null);assert.deepEqual(p.custom,{b:2});assert.deepEqual(p.tags,['Blue']);
 });
 await t.test('tag rename and deletion remain owner scoped',async()=>{
 await call('/api/ab/tag/add/personal',{token:one,body:{name:'Blue',color:7}});
 assert.equal((await call('/api/ab/tag/rename/personal',{method:'PUT',token:one,body:{old:'Blue',new:'Green'}})).status,200);
 let p=(await call('/api/ab/peers',{token:one})).data.data.find(p=>p.id==='friend');assert.deepEqual(p.tags,['Green']);
 assert.equal((await call('/api/ab/tag/personal',{method:'DELETE',token:one,body:['Green']})).status,200);
 p=(await call('/api/ab/peers',{token:one})).data.data.find(p=>p.id==='friend');assert.deepEqual(p.tags,[]);
 });
 await t.test('100 concurrent accounts cannot cross-read or cross-write',async()=>{
 const tokens=Array.from({length:100},(_,i)=>'concurrency-token-'+i);
 await db.batch(await Promise.all(tokens.map(async(raw,i)=>db.prepare('INSERT INTO br_sessions(token_hash,user_id,expires_at,native) VALUES(?,?,?,1)').bind(await digest(raw),i+3,'2099-01-01'))));
 const start=performance.now();
 const results=await Promise.all(tokens.map(async(raw,i)=>{
 const add=await call('/api/ab/peer/add/personal',{token:raw,body:{id:'same-id',alias:'owner'+i}});assert.equal(add.status,200);
 const read=await call('/api/ab/peers',{token:raw});assert.deepEqual(read.data.data.map(p=>p.alias),['owner'+i]);return true;
 }));assert.equal(results.length,100);console.log(JSON.stringify({localConcurrentAccounts:100,requests:200,elapsedMs:Math.round(performance.now()-start),hostedRequests:0}));
 });
 await t.test('authenticated reads and unchanged registration do not touch timestamps',async()=>{
 const before=await db.prepare('SELECT * FROM br_sessions WHERE token_hash=?').bind(await digest(one)).first();
 const deviceBefore=await db.prepare('SELECT * FROM br_devices WHERE user_id=1').first();
 await Promise.all(Array.from({length:20},()=>call('/api/peers',{method:'GET',token:one})));
 await call('/api/currentUser',{token:one,body:{id:'device1'}});
 assert.deepEqual(await db.prepare('SELECT * FROM br_sessions WHERE token_hash=?').bind(await digest(one)).first(),before);
 assert.deepEqual(await db.prepare('SELECT * FROM br_devices WHERE user_id=1').first(),deviceBefore);
 });
 await t.test('OAuth binds stable provider identity and consumes state once',async()=>{
 const providerId='synthetic-github-id';await db.prepare("INSERT INTO br_users(id,provider,provider_user_id,name) VALUES(103,'github',?,'github-existing')").bind(providerId).run();
 env.GITHUB_CLIENT_ID='synthetic-client';env.GITHUB_CLIENT_SECRET='synthetic-secret';
 const pending=await call('/api/oidc/auth',{body:{op:'github',id:'github-device'}});assert.equal(pending.status,200);
 const start=await worker.fetch(new Request(pending.data.url),env);assert.equal(start.status,302);
 const state=new URL(start.headers.get('location')).searchParams.get('state');
 assert.equal((await call('/auth/github/callback?state='+state.slice(0,-1)+'z&code=provider-code',{method:'GET'})).status,400);
 const savedFetch=globalThis.fetch;
 try{
 globalThis.fetch=async(url)=>new Response(JSON.stringify(url==='https://github.com/login/oauth/access_token'?{access_token:'mock'}:url==='https://api.github.com/user'?{id:providerId,login:'different-name',name:'Synthetic'}:[{primary:true,verified:true,email:'synthetic@example.test'}]),{headers:{'Content-Type':'application/json'}});
 const finish=await worker.fetch(new Request('https://api.test/auth/github/callback?state='+state+'&code=provider-code'),env);assert.equal(finish.status,200);
 assert.equal((await call('/auth/github/callback?state='+state+'&code=provider-code',{method:'GET'})).status,400);
 const claimed=await call('/api/oidc/auth-query?code='+pending.data.code,{method:'GET'});assert.equal(claimed.status,200);assert.equal(claimed.data.user.name,'github-existing');
 for(let n=0;n<2;n++)assert.equal((await call('/api/oidc/auth-query?code='+pending.data.code,{method:'GET'})).status,200);
 assert.equal((await call('/api/oidc/auth-query?code='+pending.data.code,{method:'GET'})).status,404);
 assert.equal((await call('/api/currentUser',{token:claimed.data.access_token})).data.name,'github-existing');
 }finally{globalThis.fetch=savedFetch;}
 });
 await t.test('direct routes are private, bounded, owned and read-only',async()=>{
 const key='A'.repeat(43)+'=';
 await call('/api/currentUser',{token:one,body:{id:'lan-target',deviceInfo:{account_connection:'ticket-v1',account_direct_port:21128,account_public_key:key,local_ips:['192.168.1.2','127.0.0.1','169.254.1.1','8.8.8.8','10.0.0.2','172.16.0.1','10.0.0.2','10.0.0.3','10.0.0.4']}}});
 const fields={source_id:'device1',target_id:'lan-target'};
 Object.assign(meter,{statements:0,rowsRead:0,rowsWritten:0});
 const r=await call('/api/v1/connect/route',{token:one,body:fields});assert.equal(r.status,200);
 assert.deepEqual(r.data,{port:21128,public_key:key,addresses:['192.168.1.2','10.0.0.2','172.16.0.1','10.0.0.3']});assert.equal(meter.rowsWritten,0);
 console.log(JSON.stringify({routeOperations:{...meter}}));
 assert.equal((await call('/api/v1/connect/route',{body:fields})).status,401);
 assert.equal((await call('/api/v1/connect/route',{token:two,body:fields})).status,403);
 assert.equal((await call('/api/v1/connect/route',{token:one,body:{...fields,source_id:'device2'}})).status,403);
 await call('/api/currentUser',{token:one,body:{id:'lan-target',deviceInfo:{account_connection:'ticket-v1',account_direct_port:22,account_public_key:key,local_ips:['192.168.1.2']}}});
 assert.deepEqual((await call('/api/v1/connect/route',{token:one,body:fields})).data.addresses,[]);
 });
 await t.test('measure operation cost of representative actions',async()=>{
 const costs=[];
 const measure=async(name,path,options)=>{Object.assign(meter,{statements:0,rowsRead:0,rowsWritten:0});const r=await call(path,options);assert.equal(r.status,200);costs.push({name,...meter});return r;};
 await measure('read own peers','/api/peers',{method:'GET',token:one});
 await measure('unchanged current user','/api/currentUser',{token:one,body:{id:'device1'}});
 const opened=await measure('open generation','/api/v1/presence/open',{token:one,body:{id:'device1'}});
 const event={id:'device1',generation:opened.data.generation,sequence:1,state:'offline-reported'};
 await measure('presence change','/api/v1/presence/event',{token:one,body:event});
 await measure('duplicate event','/api/v1/presence/event',{token:one,body:event});
 await call('/api/currentUser',{token:one,body:{id:'cost-target'}});
 const fields={source_id:'device1',target_id:'cost-target'};
 const grant=await measure('issue connection ticket','/api/v1/connect/grant',{token:one,body:fields});
 await measure('authorize connection ticket','/api/v1/connect/authorize',{token:one,body:{...fields,grant:grant.data.grant}});
 assert.equal(costs[0].rowsWritten,0);assert.equal(costs[1].rowsWritten,0);assert.equal(costs[4].rowsWritten,0);
 console.log(JSON.stringify({localD1OperationMeasurements:costs}));
 });
 await t.test('logout and disabled accounts deny subsequent access',async()=>{
 assert.equal((await call('/api/logout',{token:one})).status,200);assert.equal((await call('/api/peers',{method:'GET',token:one})).status,401);
 await db.prepare('UPDATE br_users SET status=0 WHERE id=2').run();assert.equal((await call('/api/peers',{method:'GET',token:two})).status,401);
 });
 await t.test('one-use connection grants bind both devices and honor logout',async()=>{
 const source=await login(100),other=await login(101);
 await call('/api/currentUser',{token:source,body:{id:'second100'}});
 const fields={source_id:'device100',target_id:'second100'};
 const issue=()=>call('/api/v1/connect/grant',{token:source,body:fields});
 let issued=await issue();assert.equal(issued.status,200);assert.equal(issued.data.grant.length,64);
 assert.equal((await call('/api/v1/connect/authorize',{token:other,body:{...fields,grant:issued.data.grant}})).status,403);
 assert.equal((await call('/api/v1/connect/authorize',{token:source,body:{...fields,target_id:'device100',grant:issued.data.grant}})).status,400);
 const authorize=grant=>call('/api/v1/connect/authorize',{token:source,body:{...fields,grant}});
 assert.equal((await authorize(issued.data.grant)).status,200);
 assert.equal((await authorize(issued.data.grant)).status,403);
 issued=await issue();await db.prepare('UPDATE br_connect_grants SET expires_at=? WHERE token_hash=?').bind('2000-01-01',await digest(issued.data.grant)).run();
 assert.equal((await authorize(issued.data.grant)).status,403);
 const targetSession=await login(100);issued=await issue();await call('/api/logout',{token:source});
 assert.equal((await call('/api/v1/connect/authorize',{token:targetSession,body:{...fields,grant:issued.data.grant}})).status,403);
 assert.equal((await call('/api/v1/connect/grant',{token:other,body:fields})).status,403);
 const burst=await Promise.all(Array.from({length:25},()=>call('/api/v1/connect/grant',{token:targetSession,body:fields})));assert.equal(burst.filter(r=>r.status===200).length,19);assert.equal(burst.filter(r=>r.status===429).length,6);
 });
 await t.test('environment, errors, crawler and payload guards',async()=>{
 assert.equal((await worker.fetch(new Request('https://api.test/api/beyondremote/ping'),{...env,DATABASE_ROLE:'production'})).status,503);
 const robots=await worker.fetch(new Request('https://api.test/robots.txt'),env);assert.match(await robots.text(),/Disallow: \//);
 const r=await call('/api/currentUser',{body:{oversized:'x'.repeat(70000)}});assert.equal(r.status,413);assert.equal(r.headers.get('Cache-Control'),'no-store');
 });
 }finally{await mf.dispose();rmSync(dir,{recursive:true,force:true});}
});
