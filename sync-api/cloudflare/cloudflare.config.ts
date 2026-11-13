import {bindings,defineConfig} from 'cf/config';
export default defineConfig({accountId:'fade5b54820c702916dd12cfc7fa43cc',worker:{
 name:'beyondremote-native-staging',entrypoint:'./worker.mjs',compatibilityDate:'2026-11-10',compatibilityFlags:['nodejs_compat'],workersDev:true,previewUrls:false,
 env:{ENVIRONMENT:bindings.text('staging'),DATABASE_ROLE:bindings.text('staging'),BASE_URL:bindings.text('https://beyondremote-native-staging.support-commsopen-shop.workers.dev'),ALLOW_SIGNUP:bindings.text('0'),ACCOUNT_SERVER_SELECTION:bindings.text('1'),DB:bindings.d1({id:'8b84364f-4902-49a6-aa4b-25cdc411e505',name:'beyondremote-native-staging'}),LOGIN_LIMIT:bindings.rateLimit({namespace:'21301',simple:{limit:6,period:60}})},
 observability:{enabled:false}
}});
