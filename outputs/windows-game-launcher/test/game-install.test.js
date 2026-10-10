import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { runInNewContext } from 'node:vm';
import { createGameInstallController } from '../src/lib/game-install-controller.js';
import { launchState, installState, installationHint } from '../src/lib/launch-state.js';
import { librarySnapshot } from '../src/lib/library-refresh.js';

const steam = () => ({id:'steam-480',title:'测试游戏',source:'steam',installation:{state:'not_installed'},runtime:{state:'idle',elapsedSeconds:0}});

test('安装请求去重、跨刷新保留等待状态，完成后不产生游玩状态',async()=>{
  let current=steam(),complete,calls=0;
  const messages=[];
  const controller=createGameInstallController({
    game:()=>current, changed() {}, notify:message=>messages.push(message),
    command:async(name,args)=>{
      assert.equal(name,'install_game'); assert.deepEqual(args,{gameId:'steam-480'}); calls++;
      await new Promise(resolve=>{complete=resolve;});
    },
    refresh:async()=>{current=controller.decorate({...current});},
  });
  const runtime=current.runtime;
  const before=librarySnapshot([current]);
  const request=controller.install(current);
  assert.equal(launchState(current).label,'正在打开安装…');
  assert.equal(launchState(current).disabled,true);
  assert.equal(librarySnapshot([current]),before);
  current=controller.decorate({...current});
  await controller.install(current);
  assert.equal(calls,1);
  assert.equal(current.installRequestPending,true);
  complete(); await request;
  assert.equal(current.installRequestPending,false);
  assert.equal(current.runtime,runtime);
  assert.equal(launchState(current).label,'安装游戏');
  assert.match(messages[0],/请在游戏客户端中确认/);
});

test('安装请求失败后刷新安装状态并恢复入口，不留下启动失败或计时',async()=>{
  let current=steam(),refreshed=0;
  const controller=createGameInstallController({
    game:()=>current, changed() {}, notify:()=>assert.fail('失败不能提示已打开'),
    command:async()=>{throw new Error('客户端链接关联缺失');},
    refresh:async()=>{refreshed++; current=controller.decorate({...current});},
  });
  await assert.rejects(controller.install(current),/客户端链接关联缺失/);
  assert.equal(refreshed,1);
  assert.equal(current.installRequestPending,false);
  assert.equal(current.runtime.state,'idle');
  assert.equal(current.runtime.elapsedSeconds,0);
  assert.equal(launchState(current).disabled,false);
});

test('实际主界面的 Steam 主按钮操作分发到安装命令，跳过启动流程',async()=>{
  const source=await readFile(new URL('../src/main.js',import.meta.url),'utf8');
  const launchSource=source.slice(source.indexOf('async function launch(game)'),source.indexOf('const gameInstalls ='));
  const current=steam(); let installed=0;
  const launch=runInNewContext(`${launchSource}\nlaunch`,{
    actions:{game:()=>current}, launchState,
    gameInstalls:{install:async item=>{assert.equal(item,current); installed++;}},
    run:()=>assert.fail('安装不能调用 launch_game'),
    patchRuntime:()=>assert.fail('安装不能进入启动流程'),
  });
  await launch(current);
  assert.equal(installed,1);
  assert.equal(current.runtime.state,'idle');
});

function element(tag) {
  const node={tag,dataset:{},children:[],attributes:{},disabled:false};
  node.append=(...children)=>{for(const child of children){child.parent=node;node.children.push(child);}};
  node.querySelector=selector=>node.children.find(child=>child.tag===selector);
  node.replaceWith=replacement=>{
    const parent=node.parent;replacement.parent=parent;parent.children[parent.children.indexOf(node)]=replacement;
  };
  node.setAttribute=(key,value)=>{node.attributes[key]=value;};
  node.getAttribute=key=>node.attributes[key];
  return node;
}
const controlsSource=(await readFile(new URL('../src/ui/game-controls.js',import.meta.url),'utf8'))
  .replace(/^import .*;\r?\n/gm,'').replaceAll('export function ','function ');
const controls=runInNewContext(`${controlsSource}\n({launchButton,installButton,patchRuntime})`,{
  el:element,icon:()=>element('svg'),launchState,installState,installationHint,
});

test('实际公共按钮在 Steam 安装完成后自动切换为开始游戏',async()=>{
  const current=steam();let clicked=0;
  const node=controls.launchButton(current,{game:()=>current,launch:async()=>{clicked++;}});
  assert.equal(node.querySelector('span').textContent,'安装游戏');
  assert.equal(node.dataset.glyph,'download');
  await node.onclick();assert.equal(clicked,1);
  current.installation.state='installed';
  controls.patchRuntime({querySelectorAll:()=>[node]},[current]);
  assert.equal(node.querySelector('span').textContent,'开始游戏');
  assert.equal(node.dataset.glyph,'play');
});

test('实际 Epic 安装按钮使用统一文案，并保留未知安装状态及原打开入口',async()=>{
  const current={...steam(),id:'epic-test',source:'epic',installation:{state:'unknown'}};
  let installed=0;
  const actions={game:()=>current,install:async()=>{installed++;}};
  const node=controls.installButton(current,actions);
  assert.equal(node.querySelector('span').textContent,'安装游戏');
  assert.equal(launchState(current).label,'在 Epic 中打开');
  await node.onclick();assert.equal(installed,1);
  assert.equal(current.installation.state,'unknown');
  current.runtime.state='running';
  controls.patchRuntime({querySelectorAll:()=>[node]},[current]);
  assert.equal(node.disabled,true);
  await node.onclick();assert.equal(installed,1);
});
