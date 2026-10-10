import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';

const script=readFileSync(new URL('../src-tauri/src/steam_family/login.js',import.meta.url),'utf8')
  .replace('__LOGIN_NONCE_JSON__',JSON.stringify('fixture-state'));

async function login(response) {
  const location={origin:'https://store.steampowered.com'};
  const context={window:{location,addEventListener(){}},
    fetch:async()=>({ok:true,json:async()=>response}),
    AbortController,setTimeout:()=>1,clearTimeout(){},setInterval:()=>2,clearInterval(){},
  };
  runInNewContext(script,context);
  // 仅等待固定数量微任务，不启动真实网络或计时器。
  for(let index=0;index<5;index++)await Promise.resolve();
  return location.href;
}

test('登录成功读取嵌套 data.webapi_token，携带当前 nonce 回传',async()=>{
  const result=new URL(await login({success:1,data:{webapi_token:'fixture-token'}}));
  assert.equal(result.protocol,'youji-steam-family:');
  assert.equal(result.searchParams.get('state'),'fixture-state');
  assert.equal(result.searchParams.get('token'),'fixture-token');
});

test('未登录或接口未成功时不回传凭证',async()=>{
  assert.equal(await login({success:1,data:{}}),undefined);
  assert.equal(await login({success:0,data:{webapi_token:'fixture-token'}}),undefined);
});
