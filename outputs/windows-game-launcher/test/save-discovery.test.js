import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
class Node {
 constructor(tag,text=''){this.tag=tag;this.textContent=text;this.children=[];}
 append(...n){this.children.push(...n);} setAttribute(k,v){this[k]=v;} replaceChildren(...n){this.children=n;}
 all(){return [this,...this.children.flatMap(n=>n.all())];}
}
const flush=()=>new Promise(r=>setImmediate(r));
async function fixture(command){
 const calls=[],chosen=[];
 globalThis.discoveryFixture={el:(tag,_class,text)=>new Node(tag,text),button:(text,_class,click)=>Object.assign(new Node('button',text),{click}),command:async(name,args)=>{calls.push({name,args});return command();}};
 const source=(await readFile(new URL('../src/ui/save-discovery.js',import.meta.url),'utf8')).replace(/^import .*;\r?\n/gm,'');
 const mod=await import('data:text/javascript;base64,'+Buffer.from('const {el,button,command}=globalThis.discoveryFixture;\n'+source+'\n//'+Math.random()).toString('base64'));
 const view=mod.directoryDiscovery({id:'game'},p=>chosen.push(p));await flush();return {view,calls,chosen,nodes:()=>view.element.all()};
}
test('自动查询只展示候选，点击才填入路径且不调用绑定',async()=>{
 const f=await fixture(()=>({matched:true,candidates:[{path:'C:/Save',evidence:'rule',files:1,bytes:10,modified:1}]}));
 assert.deepEqual(f.calls,[{name:'saves_discover',args:{gameId:'game'}}]);assert.deepEqual(f.chosen,[]);
 f.nodes().find(n=>n.textContent==='使用此目录').click();assert.deepEqual(f.chosen,['C:/Save']);assert.equal(f.calls.length,1);
});
test('未匹配、缺失目录、查询失败均提示手动选择',async()=>{
 for(const response of [{matched:false,candidates:[]},{matched:true,candidates:[]},new Error('read failed')]){
 const f=await fixture(()=>{if(response instanceof Error)throw response;return response;});assert(f.nodes().some(n=>n.textContent?.includes('手动选择')));
 }
});
test('关闭后忽略异步结果，重复识别不会并发执行',async()=>{
 let resolve;const promise=new Promise(r=>resolve=r);const f=await fixture(()=>promise);
 await f.nodes().find(n=>n.textContent==='重新识别存档目录').click();assert.equal(f.calls.length,1);
 f.view.dispose();resolve({candidates:[{path:'late',files:1,bytes:1}]});await flush();assert(!f.nodes().some(n=>n.textContent==='使用此目录'));
});
