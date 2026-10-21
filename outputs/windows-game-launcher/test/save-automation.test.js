import test from 'node:test';
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
class Node {
 constructor(tag,text=''){this.tag=tag;this.textContent=text;this.children=[];this.events={};}
 append(...nodes){this.children.push(...nodes);}
 setAttribute(k,v){this[k]=v;}
 addEventListener(k,fn){this.events[k]=fn;}
 all(){return [this,...this.children.flatMap(n=>n.all())];}
}
const flush=()=>new Promise(r=>setImmediate(r));
async function fixture(run){
 const dialogs=[],calls=[];
 const el=(tag,_class,text)=>new Node(tag,text);
 const button=(text,_class,click)=>Object.assign(new Node('button',text),{click});
 globalThis.saveUiFixture={el,button,preview:false,command:async(name,args)=>{calls.push({name,args});return run(name,args);},
 modal:(title,subtitle,content)=>{const d=new Node('dialog',title);d.append(content);dialogs.push(d);return d;},
 closeModal:async d=>d.events.close?.(),submit:(form,label,action)=>form.append(button(label,'',action))};
 const source=(await readFile(new URL('../src/ui/save-automation.js',import.meta.url),'utf8')).replace(/^import .*;\r?\n/gm,'');
 const mod=await import('data:text/javascript;base64,'+Buffer.from('const {el,button,preview,command,modal,closeModal,submit}=globalThis.saveUiFixture;\n'+source+'\n//'+Math.random()).toString('base64'));
 return {mod,calls,dialogs,find:label=>dialogs.at(-1).all().find(n=>n.textContent===label)};
}
test('恢复预览不会修改目录，取消后不提交恢复',async()=>{
 const f=await fixture(()=>({reviewId:'review',localFiles:2}));
 const result=f.mod.restoreWithReview({id:'p',name:'test'},'s');await flush();
 assert.deepEqual(f.calls.map(c=>c.name),['saves_review']);await f.find('取消').click();assert.equal(await result,false);
 assert.equal(f.calls.length,1);
});
test('确认恢复使用预览令牌，完成后才允许继续',async()=>{
 const f=await fixture(()=>({reviewId:'review',localFiles:2}));
 const result=f.mod.restoreWithReview({id:'p'},'s');await flush();
 await f.find('备份当前进度并恢复').click();assert.equal(await result,true);
 assert.deepEqual(f.calls.at(-1),{name:'saves_restore_review',args:{reviewId:'review',confirmed:true}});
});
test('发现云端差异先提示，选择本地不会下载或覆盖',async()=>{
 const f=await fixture(()=>({notices:[{profileId:'p',name:'test',snapshotId:'s',cloud:{available:true}}]}));
 const result=f.mod.checkSaveBeforeLaunch({id:'g'});await flush();
 assert.deepEqual(f.calls.map(c=>c.name),['saves_preflight']);await f.find('使用本地继续').click();assert.equal(await result,true);
 assert.equal(f.calls.length,1);
});
test('断网允许使用本地，已有设置不会自动勾选',async()=>{
 const f=await fixture(()=>{throw new Error('offline');});assert.equal(await f.mod.checkSaveBeforeLaunch({id:'g'}),true);
 const c=f.mod.checkbox('自动备份');assert.equal(c.input.checked,false);
});
