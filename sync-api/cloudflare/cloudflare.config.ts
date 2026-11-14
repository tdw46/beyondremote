import {bindings,defineConfig} from 'cf/config';
export default defineConfig(({mode})=>{
 const production=['production-preview','production'].includes(mode);
 return {accountId:'fade5b54820c702916dd12cfc7fa43cc',worker:{
 name:'beyondremote-native-staging',entrypoint:'./worker.mjs',compatibilityDate:'2026-11-10',compatibilityFlags:['nodejs_compat'],workersDev:true,previewUrls:false,
 env:{ENVIRONMENT:bindings.text('staging'),DATABASE_ROLE:bindings.text('staging'),BASE_URL:bindings.text('https://beyondremote-native-staging.support-commsopen-shop.workers.dev'),ALLOW_SIGNUP:bindings.text('0'),ACCOUNT_SERVER_SELECTION:bindings.text('1'),DB:bindings.d1({id:'8b84364f-4902-49a6-aa4b-25cdc411e505',name:'beyondremote-native-staging'}),LOGIN_LIMIT:bindings.rateLimit({namespace:'21301',simple:{limit:6,period:60}})},
 ...(production?{name:'beyondremote-native-production',workersDev:false,env:{
 ENVIRONMENT:bindings.text('production'),DATABASE_ROLE:bindings.text('production'),
 BASE_URL:bindings.text('https://api.beyondstudios.us'),
 PRODUCTION_ENABLED:bindings.text('1'),MIGRATION_READ_ONLY:bindings.text(mode==='production-preview'?'1':'0'),
 ALLOW_SIGNUP:bindings.text('1'),ACCOUNT_SERVER_SELECTION:bindings.text('0'),
 DB:bindings.d1({id:'6cbb7240-6f58-413f-9608-796f96b21e3a',name:'beyondremote-native-production'}),
 LOGIN_LIMIT:bindings.rateLimit({namespace:'22701',simple:{limit:6,period:60}})
 }}:{}),
 // Retain the approved aggregate-only monitoring; do not enable billable
 // request logs containing OAuth capability URLs during this migration.
 observability:{enabled:false}
 }};
});
