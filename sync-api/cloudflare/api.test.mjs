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
 const env={DB:measuredDB,ENVIRONMENT:'staging',DATABASE_ROLE:'staging',BASE_URL:'https://api.test',ALLOW_SIGNUP:'0',ACCOUNT_SERVER_SELECTION:'1',LOGIN_LIMIT:{limit:async()=>({success:true})}};
 const call=async(path,{method='POST',token,body={}}={})=>{const r=await worker.fetch(new Request('https://api.test'+path,{method,headers:{'Content-Type':'application/json',...(token?{Authorization:'Bearer '+token}:{})},...(['GET','HEAD'].includes(method)?{}:{body:JSON.stringify(body)})}),env);let text=await r.text();return {status:r.status,headers:r.headers,data:text?JSON.parse(text):null};};
 const login=async n=>{const r=await call('/api/login',{body:{username:'test'+n,password,id:'device'+n,deviceInfo:{name:'Computer '+n,os:'Darwin'}}});assert.equal(r.status,200);return r.data.access_token;};
 const one=await login(1),two=await login(2);
 await t.test('production activation and restore fences fail closed',async()=>{
 const ping=new Request('https://api.test/api/beyondremote/ping');
 for(const patch of [{ENVIRONMENT:'production'},{ENVIRONMENT:'production',DATABASE_ROLE:'production'},{DATABASE_ROLE:'production'},{BASE_URL:'https://wrong.test'}])assert.equal((await worker.fetch(ping,{...env,...patch})).status,503);
 const prod={...env,ENVIRONMENT:'production',DATABASE_ROLE:'production',PRODUCTION_ENABLED:'1',MIGRATION_READ_ONLY:'1'};
 assert.equal((await worker.fetch(ping,prod)).status,200);
 for(const path of ['/api/login','/api/logout','/api/currentUser'])assert.equal((await worker.fetch(new Request('https://api.test'+path,{method:'POST'}),prod)).status,503);
 for(const path of ['/auth/google/callback','/api/oidc/auth-query'])assert.equal((await worker.fetch(new Request('https://api.test'+path),prod)).status,503);
 assert.equal((await worker.fetch(new Request('https://api.test/api/peers',{headers:{Authorization:'Bearer '+one}}),prod)).status,200);
 assert.equal((await worker.fetch(new Request('https://api.test/api/peers'),prod)).status,401);
 const bridge={...prod,TRANSPORT_ORIGIN:'https://transport.test'};
 assert.equal((await worker.fetch(new Request('https://transport.test/api/beyondremote/ping'),bridge)).status,200);
 assert.equal((await worker.fetch(new Request('https://transport.test/api/peers'),bridge)).status,401);
 assert.equal((await worker.fetch(new Request('https://transport.test/api/peers',{headers:{Authorization:'Bearer '+one}}),bridge)).status,200);
 assert.equal((await worker.fetch(new Request('https://transport.test/api/login',{method:'POST'}),bridge)).status,503);
 assert.equal((await worker.fetch(new Request('https://transport.test/api/beyondremote/ping'),{...env,TRANSPORT_ORIGIN:'https://transport.test'})).status,503);
 for(const origin of ['http://transport.test','https://transport.test.evil.test','https://other.test'])assert.equal((await worker.fetch(new Request(origin+'/api/beyondremote/ping'),bridge)).status,503);
 });

 await t.test('wake tickets isolate accounts, purposes, revocation and concurrency',async()=>{
 const register=(id,deviceInfo,token=one)=>call('/api/currentUser',{token,body:{id,deviceInfo}});
 const helperInfo={os:'Darwin',wake_helper:'wake-v1',account_connection:'ticket-v1',account_direct_port:21128,account_public_key:'A'.repeat(43)+'=',managed_server_running:true,managed_server_public_host:'home.example.test',managed_server_key:'B'.repeat(43)+'=',local_ips:['192.168.4.1']};
 const targetInfo={os:'Windows',wake_enabled:true,wake_profiles:[{mac:'02:12:34:56:78:9a',ip:'192.168.4.2',prefix:24}]};
 await register('wake-target',targetInfo);
 await call('/api/v1/presence/open',{token:one,body:{id:'wake-helper',deviceInfo:helperInfo,state:'online-reported'}});
 await call('/api/v1/connection-servers/select',{token:one,body:{device_id:'wake-helper'}});
 const route=await call('/api/v1/connect/route',{token:one,body:{source_id:'device1',target_id:'wake-helper'}});
 assert.equal(route.data.public_address,'home.example.test');
 assert.equal((await call('/api/v1/connect/route',{token:two,body:{source_id:'device2',target_id:'wake-helper'}})).status,403);
 const body={source_id:'device1',target_id:'wake-target'};
 assert.equal((await call('/api/v1/wake/prepare',{body})).status,401);
 assert.equal((await call('/api/v1/wake/prepare',{token:two,body})).status,409);
 const prepare=()=>call('/api/v1/wake/prepare',{token:one,body});
 Object.assign(meter,{statements:0,rowsRead:0,rowsWritten:0});
 const grant=await prepare();assert.equal(grant.status,200);assert.equal(grant.data.helper_id,'wake-helper');
 console.log(JSON.stringify({wakePrepare:{...meter}}));
 const consume=(g,extra={},token=one)=>call('/api/v1/wake/consume',{token,body:{...body,helper_id:'wake-helper',grant:g,...extra}});
 assert.equal((await consume(grant.data.grant,{helper_id:'device1'})).status,403);
 assert.equal((await call('/api/v1/connect/authorize',{token:one,body:{...body,grant:grant.data.grant}})).status,403);
 Object.assign(meter,{statements:0,rowsRead:0,rowsWritten:0});
 assert.equal((await consume(grant.data.grant)).status,200);
 console.log(JSON.stringify({wakeConsume:{...meter}}));
 assert.equal((await consume(grant.data.grant)).status,403);
 const pending=await prepare();assert.equal(pending.status,200);
 await register('wake-target',{...targetInfo,wake_enabled:false});
 assert.equal((await consume(pending.data.grant)).status,409);
 await register('wake-target',targetInfo);
 const third=await prepare();assert.equal(third.status,200);
 assert.equal((await prepare()).status,429);
 await db.prepare('UPDATE br_wake_grants SET expires_at=? WHERE user_id=1').bind('2000-01-01').run();
 assert.equal((await consume(third.data.grant)).status,403);
 const race=await Promise.all(Array.from({length:20},prepare));
 assert.equal(race.filter(r=>r.status===200).length,3);
 assert.equal(race.filter(r=>r.status===429).length,17);
 const ticket=race.find(r=>r.status===200).data.grant;
 await db.prepare("DELETE FROM br_devices WHERE user_id=1 AND remote_id='device1'").run();
 assert.equal((await consume(ticket)).status,403);
 await register('device1',{os:'Darwin'});
 await db.prepare('DELETE FROM br_wake_grants WHERE user_id=1').run();
 // A stale helper login cannot consume a fresh request after its session is replaced.
 const another=await prepare();assert.equal(another.status,200);
 await db.prepare("UPDATE br_presence SET token_hash='revoked' WHERE user_id=1 AND remote_id='wake-helper'").run();
 assert.equal((await consume(another.data.grant)).status,403);
 await db.prepare("DELETE FROM br_devices WHERE user_id=1 AND remote_id IN('wake-target','wake-helper')").run();
 await db.prepare('DELETE FROM br_wake_grants WHERE user_id=1').run();
 });
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
 await t.test('server choice is owner scoped, idempotent and separate from reachability',async()=>{
 await call('/api/currentUser',{token:one,body:{id:'device1',deviceInfo:{os:'Darwin',account_connection:'ticket-v1'}}});
 const select=id=>call('/api/v1/connection-servers/select',{token:one,body:{device_id:id}});
 assert.equal((await call('/api/v1/connection-servers',{method:'GET'})).status,401);
 assert.equal((await select('device2')).status,403);
 await call('/api/currentUser',{token:one,body:{id:'phone',deviceInfo:{os:'iOS'}}});
 assert.equal((await select('phone')).status,400);
 assert.equal((await select('device1')).status,200);
 const before=await db.prepare('SELECT * FROM br_server_preferences WHERE user_id=1').first();
 Object.assign(meter,{statements:0,rowsRead:0,rowsWritten:0});
 assert.equal((await select('device1')).status,200);assert.equal(meter.rowsWritten,0);
 console.log(JSON.stringify({unchangedServerSelection:{...meter}}));
 assert.deepEqual(await db.prepare('SELECT * FROM br_server_preferences WHERE user_id=1').first(),before);
 const stale={os:'Windows',name:'Old desktop',managed_server_running:true,managed_server_public_host:'old.example.test',managed_server_key:'A'.repeat(43)+'='};
 await call('/api/currentUser',{token:one,body:{id:'old-server',deviceInfo:stale}});
 Object.assign(meter,{statements:0,rowsRead:0,rowsWritten:0});
 const list=(await call('/api/v1/connection-servers',{method:'GET',token:one})).data.devices;
 assert.equal(meter.rowsWritten,0);console.log(JSON.stringify({serverListOperations:{...meter}}));
 assert.equal(list.find(d=>d.id==='device1').selected,true);
 assert.equal(list.find(d=>d.id==='old-server').state,'unconfirmed');
 assert.equal(list.find(d=>d.id==='old-server').host,undefined);
 assert.ok(!(await call('/api/v1/connection-servers',{method:'GET',token:two})).data.devices.some(d=>d.id==='device1'));
 env.ACCOUNT_SERVER_SELECTION='1';
 try{
 let config=(await call('/api/currentUser',{token:one})).data.strategy.config_options;
 assert.equal(config['beyondremote-selected-server-id'],'device1');
 assert.equal(config['custom-rendezvous-server'],'');
 await select('old-server');
 config=(await call('/api/currentUser',{token:one})).data.strategy.config_options;
 assert.equal(config['custom-rendezvous-server'],'');
 const opened=await call('/api/v1/presence/open',{token:one,body:{id:'old-server'}});
 config=(await call('/api/currentUser',{token:one})).data.strategy.config_options;
 assert.equal(config['custom-rendezvous-server'],'old.example.test:21116');
 await call('/api/v1/presence/event',{token:one,body:{id:'old-server',generation:opened.data.generation,sequence:1,state:'offline-reported'}});
 config=(await call('/api/currentUser',{token:one})).data.strategy.config_options;
 assert.equal(config['custom-rendezvous-server'],'');
 await select('device1');
 }finally{env.ACCOUNT_SERVER_SELECTION='1';}
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
 assert.deepEqual(r.data,{port:21128,public_address:'',public_port:21128,public_key:key,addresses:['192.168.1.2','10.0.0.2','172.16.0.1','10.0.0.3']});assert.equal(meter.rowsWritten,0);
 console.log(JSON.stringify({routeOperations:{...meter}}));
 assert.equal((await call('/api/v1/connect/route',{body:fields})).status,401);
 assert.equal((await call('/api/v1/connect/route',{token:two,body:fields})).status,403);
 assert.equal((await call('/api/v1/connect/route',{token:one,body:{...fields,source_id:'device2'}})).status,403);
 await call('/api/currentUser',{token:one,body:{id:'lan-target',deviceInfo:{account_connection:'ticket-v1',account_direct_port:22,account_public_key:key,local_ips:['192.168.1.2']}}});
 assert.deepEqual((await call('/api/v1/connect/route',{token:one,body:fields})).data.addresses,[]);
 });
 await t.test('unselected desktop internet route requires current owner session and safe public endpoint',async()=>{
 const info={os:'Windows',account_connection:'ticket-v1',account_direct_port:21128,account_public_key:'A'.repeat(43)+'=',account_public_address:'47.203.39.150',account_public_port:45123};
 const fields={source_id:'device1',target_id:'internet-target'};
 const publish=extra=>call('/api/v1/presence/open',{token:one,body:{id:'internet-target',deviceInfo:{...info,...extra},state:'online-reported'}});
 await publish({});
 Object.assign(meter,{statements:0,rowsRead:0,rowsWritten:0});
 let result=await call('/api/v1/connect/route',{token:one,body:fields});
 assert.equal(result.data.public_address,info.account_public_address);assert.equal(result.data.public_port,45123);assert.equal(meter.rowsWritten,0);
 console.log(JSON.stringify({internetRouteOperations:{...meter}}));
 // UI account refresh has no access to the service process's in-memory mapping.
 const uiInfo={...info,account_public_address:'',account_public_port:0};
 Object.assign(meter,{statements:0,rowsRead:0,rowsWritten:0});
 await call('/api/currentUser',{token:one,body:{id:'internet-target',deviceInfo:uiInfo}});
 assert.equal(meter.rowsWritten,0,'An unchanged UI refresh must not rewrite device rows');
 result=await call('/api/v1/connect/route',{token:one,body:fields});
 assert.equal(result.data.public_address,info.account_public_address,'UI refresh must not erase the service route');
 assert.equal(result.data.public_port,45123);
 await Promise.all([
  publish({account_public_address:'',account_public_port:0}),
  ...Array.from({length:8},()=>call('/api/currentUser',{token:one,body:{id:'internet-target',deviceInfo:uiInfo}}))
 ]);
 assert.equal((await call('/api/v1/connect/route',{token:one,body:fields})).data.public_address,'','Concurrent UI refreshes must not resurrect a cleared service route');
 await publish({account_public_address:'',account_public_port:0});
 assert.equal((await call('/api/v1/connect/route',{token:one,body:fields})).data.public_address,'','Service must still revoke its route');
 await publish({});
 await call('/api/currentUser',{token:one,body:{id:'internet-target',deviceInfo:{...uiInfo,account_public_key:'B'.repeat(43)+'='}}});
 assert.equal((await call('/api/v1/connect/route',{token:one,body:fields})).data.public_address,'','A new device key must not inherit an old route');
 await publish({});
 assert.equal((await call('/api/v1/connect/route',{token:two,body:fields})).status,403);
 for(const account_public_address of ['127.0.0.1','169.254.169.254','100.64.0.1','192.168.1.1','256.1.2.3','example.com']){
  await publish({account_public_address});assert.equal((await call('/api/v1/connect/route',{token:one,body:fields})).data.public_address,'');
 }
 await publish({account_public_port:22});assert.equal((await call('/api/v1/connect/route',{token:one,body:fields})).data.public_address,'');
 await publish({});
 await db.prepare("UPDATE br_presence SET token_hash='revoked' WHERE user_id=1 AND remote_id='internet-target'").run();
 assert.equal((await call('/api/v1/connect/route',{token:one,body:fields})).data.public_address,'');
 await db.prepare("DELETE FROM br_devices WHERE user_id=1 AND remote_id='internet-target'").run();
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
