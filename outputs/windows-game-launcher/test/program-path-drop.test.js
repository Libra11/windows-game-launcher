import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { runInNewContext } from 'node:vm';
import { createProgramDropHandler } from '../src/lib/program-drop.js';
import { programDropPoint } from '../src/lib/program-path.js';

test('实际文件拖入监听按路径框分流，弹窗外仍沿用原导入流程', async () => {
  const source=(await readFile(new URL('../src/ui/program-drop.js',import.meta.url),'utf8')).replace(/^import .+;\r?\n/gm,'').replace('export async function','async function');
  let receive, blocked=true, target;
  const imported=[],filled=[],messages=[];
  const node=()=>({hidden:true,setAttribute(){},append(){},classList:{add(){},remove(){}}});
  const mount=runInNewContext(source+'\nmountProgramDrop',{
    el:node,document:{body:{append(){}},querySelector:()=>blocked?{}:null},
    onFileDrop:async handler=>{receive=handler;return()=>{};},
    createProgramDropHandler,programPathTarget:()=>target,
    command:async(name,{path})=>{assert.equal(name,'prepare_local_import');return{exePath:path};},
    addDialog:async(actions,candidate)=>imported.push(candidate.exePath),
  });
  await mount({toast:message=>messages.push(message)});
  for(const field of ['game','launcher']) {
    target={wrapper:node(),select:path=>filled.push([field,path])};
    receive({payload:{type:'drop',position:{x:100,y:100},paths:[field+'.lnk']}});
  }
  assert.deepEqual(filled,[['game','game.lnk'],['launcher','launcher.lnk']]);
  assert.deepEqual(imported,[]);
  receive({payload:{type:'drop',paths:['a.exe','b.exe']}});
  assert.match(messages.at(-1),/一次拖入一个/);assert.equal(filled.length,2);
  target=null;receive({payload:{type:'drop',paths:['outside.exe']}});
  assert.match(messages.at(-1),/关闭当前窗口/);assert.deepEqual(imported,[]);
  blocked=false;receive({payload:{type:'drop',paths:['new.exe']}});
  for(let i=0;i<5;i++)await Promise.resolve();
  assert.deepEqual(imported,['new.exe']);
});

test('实际路径框命中使用缩放后的坐标，并排除被遮挡或禁用的字段', async () => {
  const source=await readFile(new URL('../src/ui/program-path-field.js',import.meta.url),'utf8');
  const targetSource=source.slice(source.indexOf('export function programPathTarget')).replace('export function','function');
  let disabled=false, inside=true; const points=[];
  const wrapper={},target={input:{matches:()=>disabled},select(){}};
  const targetAt=runInNewContext(targetSource+'\nprogramPathTarget',{
    programDropPoint,window:{devicePixelRatio:1.5},targets:new WeakMap([[wrapper,target]]),
    document:{querySelectorAll:()=>[{contains:()=>false},{contains:()=>inside}],elementsFromPoint:(x,y)=>{points.push([x,y]);return[{closest:()=>wrapper}];}},
  });
  assert.equal(targetAt({x:600,y:300}).input,target.input);
  assert.deepEqual(points,[[400,200]]);
  disabled=true;assert.equal(targetAt({x:600,y:300}),null);
  disabled=false;inside=false;assert.equal(targetAt({x:600,y:300}),null);
});
