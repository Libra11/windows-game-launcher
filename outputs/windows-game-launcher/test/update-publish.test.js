import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync,mkdirSync,writeFileSync,readFileSync,rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
const publishTest=process.platform==='win32'?test.skip:test;
const script=fileURLToPath(new URL('../../../.github/scripts/publish-update-release.sh',import.meta.url));
function publish({draft='missing',uploadFail=false,stable='v1.0.0',complete=true,tag='v1.1.0'}={}){
  const root=mkdtempSync(join(tmpdir(),'youji-publish-'));
  try{
    const bin=join(root,'bin'),release=join(root,'release'),log=join(root,'calls');mkdirSync(bin);mkdirSync(release);
    for(const name of [`youji_${tag.slice(1)}_x64-setup.exe`,'latest.json','release-notes.md'])writeFileSync(join(release,name),'fixture');if(complete)writeFileSync(join(release,`youji_${tag.slice(1)}_x64-setup.exe.sig`),'fixture');
    writeFileSync(join(bin,'git'),'#!/usr/bin/env bash\nexit 1\n',{mode:0o755});
    writeFileSync(join(bin,'gh'),`#!/usr/bin/env bash
printf '%s\\n' "$*" >> "$FAKE_LOG"
if [[ "$1 $2" == "release view" ]]; then
  if [[ "$FAKE_DRAFT" == "missing" ]]; then exit 1; fi
  echo "$FAKE_DRAFT"
fi
if [[ "$1 $2" == "release upload" && "$FAKE_FAIL_UPLOAD" == "yes" ]]; then exit 1; fi
if [[ "$1" == "api" ]]; then echo "$FAKE_STABLE"; fi
exit 0
`,{mode:0o755});
    const result=spawnSync('bash',[script,release],{cwd:root,encoding:'utf8',env:{...process.env,PATH:`${bin}:${process.env.PATH}`,RELEASE_TAG:tag,RELEASE_SHA:'a'.repeat(40),GITHUB_REPOSITORY:'Libra11/windows-game-launcher',FAKE_LOG:log,FAKE_DRAFT:draft,FAKE_FAIL_UPLOAD:uploadFail?'yes':'no',FAKE_STABLE:stable}});
    let calls='';try{calls=readFileSync(log,'utf8');}catch{}
    return {status:result.status,calls};
  }finally{rmSync(root,{recursive:true,force:true});}
}
publishTest('上传失败留下草稿，不执行公开发布',()=>{
  const result=publish({uploadFail:true});assert.notEqual(result.status,0);assert.match(result.calls,/release create.*--draft/);assert.match(result.calls,/release upload/);assert.doesNotMatch(result.calls,/release edit/);
});
publishTest('公开版本不覆盖，缺失签名在调用发布 API 前拒绝',()=>{
  const published=publish({draft:'false'});assert.equal(published.status,0);assert.doesNotMatch(published.calls,/release upload|release edit/);
  const missing=publish({complete:false});assert.notEqual(missing.status,0);assert.equal(missing.calls,'');
});
publishTest('上传完整后才公开，较旧正式版本不提升最新入口',()=>{
  const good=publish();assert.equal(good.status,0);assert.ok(good.calls.indexOf('release upload')<good.calls.indexOf('release edit'));assert.match(good.calls,/release edit.*--draft=false --latest /);
  const old=publish({stable:'v1.2.0'});assert.equal(old.status,0);assert.match(old.calls,/release edit.*--latest=false/);
});

publishTest('预发布保持测试标记，不成为正式更新入口',()=>{
  const result=publish({tag:'v1.1.0-beta.1'});assert.equal(result.status,0);assert.match(result.calls,/release edit.*--latest=false --prerelease/);
});
