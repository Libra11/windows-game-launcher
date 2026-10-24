import test from 'node:test';
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
class Node{constructor(tag,text=''){this.tag=tag;this.textContent=text;this.children=[];this.events={};this.style={};this.value='';this.isConnected=true;}append(...n){this.children.push(...n);}replaceChildren(...n){this.children=n;}setAttribute(k,v){this[k]=v;}addEventListener(k,f){this.events[k]=f;}querySelector(){return null;}all(){return[this,...this.children.flatMap(n=>n.all())];}}
Node.prototype.click=function(){return this.events.click?.();};
const flush=()=>new Promise(r=>setImmediate(r));
async function fixture(handler,mode='friends',localGames=[]){
 const calls=[],errors=[],images=[];let tick;
 const domSource=(await readFile(new URL('../src/lib/dom.js',import.meta.url),'utf8')).replace(/^import .*;\r?\n/gm,'');
 const actualDom=await import('data:text/javascript;base64,'+Buffer.from('const failedArtwork=new Set();const document={createElement:tag=>new globalThis.socialTestNode(tag)};\n'+domSource).toString('base64'));
 globalThis.socialTestNode=Node;
 globalThis.socialFixture={icon:()=>new Node('svg'),el:(tag,cls,text)=>{const n=new Node(tag,text);if(tag==='img')images.push(n);return n;},button:(text,cls,click)=>Object.assign(actualDom.button(text,cls,click),{textContent:text}),modal:()=>new Node('dialog'),closeModal:async()=>{},setInterval:f=>(tick=f,1),clearInterval:()=>{},document:{hidden:false,querySelector:()=>null}};
 const source=(await readFile(new URL('../src/ui/social.js',import.meta.url),'utf8')).replace(/^import .*;\r?\n/gm,'');
 const mod=await import('data:text/javascript;base64,'+Buffer.from('const {el,button,icon,modal,closeModal,setInterval,clearInterval,document}=globalThis.socialFixture;\n'+source+'\n//'+Math.random()).toString('base64'));
 const view=mod.createSocialPage({command:async(name,args)=>{calls.push({name,args});if(name==='social_local')return{status:{connected:false,error:'offline'},games:localGames,notifications:true,dnd:false};return handler(args);},settings:()=>{},toast:(e)=>errors.push(e)},mode);await flush();return{mod,view,calls,errors,images,tick:()=>tick(),nodes:()=>view.element.all()};
}
const me={id:'self',nickname:'Me',friendCode:'code',avatar:'builtin:leaf',privacy:{hiddenGames:[]}};
test('无变化轮询保留页面节点，离开页面停止请求本机游戏库与远端',async()=>{
 const f=await fixture(({action})=>action==='me'?me:{friends:[]});const header=f.view.element.children[0];
 f.tick();await flush();assert.equal(f.view.element.children[0],header);
 assert(f.calls.filter(c=>c.name==='social_local').every(c=>c.args.includeGames===false));
 const count=f.calls.length;f.view.element.isConnected=false;f.tick();await flush();assert.equal(f.calls.length,count);f.view.dispose();
});
test('迟到平台图片不覆盖云端图片，状态变化复用已解码图片节点',async()=>{
 const space={profile:me,presence:{online:false},recent:[],games:[{key:'steam:1',title:'Game',cover:'https://example.test/cover.jpg',coverRevision:'abc'}],initialized:true,libraryVisible:true,total:1};
 const f=await fixture(({action})=>action==='me'?me:action==='cover'?{data:'data:image/jpeg;base64,abc'}:space,'space');await flush();
 const platform=f.images.find(n=>n.src.startsWith('https:')),cloud=f.images.find(n=>n.src.startsWith('data:'));
 cloud.onload();platform.onload();assert(f.nodes().includes(cloud));assert(!f.nodes().includes(platform));
 space.localSeconds=10;await f.nodes().find(n=>n.textContent==='刷新').click();assert(f.nodes().includes(cloud));assert.equal(f.images.length,2);f.view.dispose();
});
test('云封面请求最多四个并发',async()=>{
 const pending=[];const space={profile:me,presence:{online:false},recent:[],games:Array.from({length:8},(_,i)=>({key:`steam:${i}`,title:'Game',coverRevision:'abc'})),initialized:true,libraryVisible:true,total:8};
 const f=await fixture(({action})=>action==='me'?me:action==='cover'?new Promise(resolve=>pending.push(resolve)):space,'space');
 assert.equal(pending.length,4);pending[0]({data:'data:image/jpeg;base64,abc'});await flush();assert.equal(pending.length,5);f.view.dispose();
});
test('离线状态不把缓存中的游戏冒充实时状态',async()=>{const f=await fixture(({action})=>action==='me'?me:{friends:[{id:'a',nickname:'Friend',relation:'accepted',presence:{online:true,games:[{title:'Game'}]}}]});assert(f.nodes().some(n=>String(n.textContent).includes('状态暂不可用')));assert(!f.nodes().some(n=>n.textContent==='正在玩 Game'));f.view.dispose();});
test('未同步空间只提示，不自动执行导入',async()=>{const f=await fixture(({action})=>action==='me'?me:{profile:me,presence:{online:false},recent:[],games:[],initialized:false,libraryVisible:true,total:0},'mine');assert(f.nodes().some(n=>n.textContent==='前往云同步设置'));assert(f.calls.every(c=>['social_call','social_local'].includes(c.name)));f.view.dispose();});
test('释放页面后忽略迟到响应，不继续轮询',async()=>{let resolve;const f=await fixture(()=>new Promise(r=>resolve=r));f.view.dispose();resolve(me);await flush();f.tick();await flush();assert.equal(f.calls.filter(c=>c.args?.action==='friends').length,0);});
test('好友申请必须主动发送，拒绝暴露旧的访问内容',async()=>{let failed=false;const f=await fixture(({action})=>{if(failed)throw Error('空间不可访问');return action==='me'?me:{friends:[]};});assert(!f.calls.some(c=>c.args?.action==='request'));failed=true;await f.nodes().find(n=>n.textContent==='刷新').click();assert(f.nodes().some(n=>n.textContent==='Error: 空间不可访问'));f.view.dispose();});

test('仅自己的空间请求本机匹配封面，好友空间不读取本机图片',async()=>{const space={profile:me,presence:{online:false},recent:[],games:[{key:'epic:1',title:'Game'}],initialized:true,libraryVisible:true,total:1};for(const mode of ['mine','space']){const f=await fixture(({action})=>action==='me'?me:space,mode,[{key:'epic:1',id:'local-id'}]);await flush();assert.equal(f.calls.some(c=>c.name==='get_cached_cover'&&c.args.gameId==='local-id'),mode==='mine');f.view.dispose();}});

test('好友空间按所有者和版本读取云封面，刷新复用请求',async()=>{const space={profile:me,presence:{online:false},recent:[],games:[{key:'epic:1',title:'Game',coverRevision:'abc'}],initialized:true,libraryVisible:true,total:1};const f=await fixture(({action})=>action==='me'?me:action==='cover'?{data:''}:space,'space');await flush();await f.nodes().find(n=>n.textContent==='刷新').click();assert.equal(f.calls.filter(c=>c.args?.action==='cover').length,1);assert.equal(f.calls.find(c=>c.args?.action==='cover').args.id,'self');f.view.dispose();});
