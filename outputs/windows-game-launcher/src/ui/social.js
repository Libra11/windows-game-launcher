import { el, button, icon } from '../lib/dom.js';
import { modal, closeModal } from './modal.js';
import './social.css';
import { localCoverUrl } from '../lib/cover-cache.js';
const avatars={'builtin:leaf':'🌿','builtin:moon':'🌙','builtin:star':'⭐'};
export function statusText(p,connected=true){if(!connected)return '连接异常 · 状态暂不可用';if(!p?.online)return '离线';return p.games?.length?`正在玩 ${p.games[0].title}${p.games.length>1?` 等 ${p.games.length} 款游戏`:''}`:'在线';}
export function hours(seconds){return seconds==null?'未公开':`${(Number(seconds)/3600).toFixed(1)} 小时`;}
export function createSocialPage({command,settings,toast,onNavigate=()=>{}},initial='friends'){
 const element=el('div','social-page');let disposed=false,version=0,me,mode=initial,owner,offset=0,busy=false,editing=false,connected=false,lastView='';let localGames=new Map();const cloudCovers=new Map(),coverNodes=new Map();
 let coverRequests=0;const coverQueue=[];
 function pumpCovers(){while(!disposed&&coverRequests<4&&coverQueue.length){const job=coverQueue.shift();coverRequests++;job().finally(()=>{coverRequests--;pumpCovers();});}}
 function fetchCover(key,args){if(!cloudCovers.has(key)){const task=new Promise(resolve=>{coverQueue.push(async()=>{try{resolve(await call('cover',args));}catch{cloudCovers.delete(key);resolve(null);}});});cloudCovers.set(key,task);pumpCovers();}return cloudCovers.get(key);}
 const call=(action,args={})=>command('social_call',{action,...args});
 const error=e=>toast(String(e),true);
 function action(label,fn,style='secondary'){return button(label,style,async()=>{try{await fn();}catch(e){error(e);}});}
 function checkbox(label,checked,change){const row=el('label','social-toggle');const input=el('input');input.type='checkbox';input.checked=checked;input.onchange=()=>change(input.checked);row.append(input,el('span','',label));return row;}
 async function portrait(p){const node=el('div','social-avatar',avatars[p.avatar]||'🎮');if(p.avatar==='custom'){call('avatar',{id:p.id}).then(v=>{if(disposed||!node.isConnected)return;const img=el('img');img.alt=`${p.nickname}的头像`;if(/^data:image\/png;base64,/.test(v.data)){img.src=v.data;node.replaceChildren(img);}}).catch(()=>{});}return node;}
 function confirmation(title,fn){const body=el('div','form');const dialog=modal(title,'此操作会立即更新双方的好友访问权限。',body);body.append(action('取消',()=>closeModal(dialog)),action('确认',async()=>{await fn();await closeModal(dialog);await load();},'primary'));}
 async function relation(id,act){await call('relation',{id,body:{action:act}});await load();}
 function header(title){const h=el('header','social-heading');const copy=el('div');copy.append(el('p','social-eyebrow','一起留下游戏足迹'),el('h1','',title));const nav=el('div','social-actions');nav.append(action('好友',()=>open('friends')),action('我的空间',()=>open('mine')),action('刷新',()=>load()));if(mode!=='friends')nav.append(action('全屏 / 退出全屏',async()=>{if(document.fullscreenElement===element)await document.exitFullscreen();else await element.requestFullscreen();}));h.append(copy,nav);element.append(h);}
 async function edit(){editing=true;const body=el('form','social-edit');const dialog=modal('编辑个人空间','好友默认可见；隐身只隐藏在线状态。',body);dialog.addEventListener('close',()=>{editing=false;},{once:true});
 const field=(label,value,max)=>{const l=el('label','field',label);const input=el('input');input.value=value||'';input.maxLength=max;l.append(input);body.append(l);return input;};
 const name=field('昵称',me.nickname,40),bio=field('简介',me.bio,500);
 let chosen=me.avatar;const picker=el('div','social-actions');for(const [key,value]of Object.entries(avatars)){picker.append(action(value,()=>{chosen=key;hint.textContent=`已选择 ${value}`;}));}body.append(picker);
 const hint=el('p','social-muted','选择内置头像，或上传 PNG / JPEG / WebP（不超过 500 KB）。');body.append(hint);
 const file=el('input');file.type='file';file.accept='image/png,image/jpeg,image/webp';file.setAttribute('aria-label','自定义头像');body.append(file);
 file.onchange=async()=>{const f=file.files?.[0];if(!f)return;if(f.size>512000){hint.textContent='图片不能超过 500 KB';return;}
 try{const data=await new Promise((resolve,reject)=>{const reader=new FileReader();reader.onload=()=>resolve(String(reader.result).split(',')[1]);reader.onerror=reject;reader.readAsDataURL(f);});await call('uploadAvatar',{body:{data}});chosen='custom';hint.textContent='头像已上传';}catch(e){hint.textContent=String(e);}};
 const privacy=structuredClone(me.privacy);
 for(const [key,label]of [['invisible','隐身（好友看到离线，不发送开始游戏通知）'],['library','好友可查看游戏库'],['playtime','好友可查看游玩时长'],['achievements','好友可查看成就']])body.append(checkbox(label,privacy[key],v=>privacy[key]=v));
 body.append(el('p','social-muted','游戏库开关控制完整列表；最近游玩仍展示。隐藏指定游戏会同时隐藏它的动态、记录与统计。'));
 const local=await command('social_local',{});const hidden=new Set(privacy.hiddenGames);const list=el('div','social-hidden-list');const filter=el('input');filter.placeholder='搜索要隐藏的游戏';filter.setAttribute('aria-label','搜索隐藏游戏');body.append(filter,list);
 const draw=()=>{list.replaceChildren();for(const g of local.games.filter(g=>g.title.toLowerCase().includes(filter.value.toLowerCase()))){list.append(checkbox(g.title,hidden.has(g.key),v=>v?hidden.add(g.key):hidden.delete(g.key)));}};filter.oninput=draw;draw();
 const save=action('保存资料与隐私',async()=>{await call('edit',{body:{nickname:name.value,bio:bio.value,avatar:chosen,privacy:{...privacy,hiddenGames:[...hidden]}}});await closeModal(dialog);await load();},'primary');body.append(save);body.onsubmit=e=>{e.preventDefault();save.click();};
 }
 async function drawFriends(v,local){
 header('好友');
 const accepted=v.friends.filter(p=>p.relation==='accepted');
 const online=accepted.filter(p=>connected&&p.presence?.online).length;
 const layout=el('div','social-friends-layout');const main=el('div','social-friends-main');const aside=el('aside','social-friends-aside');
 const summary=el('section','social-friends-summary');summary.append(el('p','social-eyebrow','一起玩，也一起分享'));
 summary.append(el('h2','','下一场冒险，有好友同行'),el('p','social-muted','看看朋友正在玩什么，走进他们的游戏空间。'));
 const counts=el('div','social-stats');for(const [value,label]of [[accepted.length,'位好友'],[connected?online:'—','位在线'],[v.friends.filter(p=>p.relation==='incoming').length,'条新申请']]){const stat=el('div','social-stat');stat.append(el('strong','',String(value)),el('span','',label));counts.append(stat);}summary.append(counts);main.append(summary);
 const controls=el('section','social-panel social-invite');controls.append(el('h2','','添加好友'),el('p','social-muted','输入好友码，向朋友发出邀请。'));
 const add=el('div','social-actions');const code=el('input');code.placeholder='输入对方的好友码';code.setAttribute('aria-label','好友码');code.maxLength=64;add.append(code,action('发送申请',async()=>{await call('request',{body:{code:code.value}});code.value='';toast('好友申请已发送');await load();},'primary'));controls.append(add);
 aside.append(controls);
 const share=el('section','social-panel social-share');share.append(el('h2','','我的好友码'),el('p','social-muted','把好友码发给朋友，同意申请后即可互看空间。'));
 const ownCode=el('code','social-own-code',me.friendCode);share.append(ownCode,action('复制好友码',async()=>{try{await navigator.clipboard.writeText(me.friendCode);toast('好友码已复制');}catch{toast('无法复制，请选中好友码手动复制',true);}}));aside.append(share);
 const notices=el('section','social-panel social-notices');notices.append(el('h2','','动态通知'),checkbox('显示好友游戏通知',local.notifications,async notifications=>{await command('social_local',{settings:{notifications}}).catch(error);}),checkbox('免打扰',local.dnd,async dnd=>{await command('social_local',{settings:{dnd}}).catch(error);}),el('p','social-muted','免打扰期间保持安静，结束后不补发旧通知。'));notices.append(action('预览通知效果',async()=>{await command('social_test_notice');toast('测试提示已排队，不会通知其他人');}));aside.append(notices);
 const status=el('p',`social-status social-connection${connected?' is-online':''}`,connected?'已连接 · 游戏中也可接收通知':`连接异常：${local.status.error||'正在连接'}。列表中的在线状态暂不可用。`);status.setAttribute('role','status');main.append(status);
 if(!accepted.length){const empty=el('section','social-friends-empty');empty.append(icon('game'),el('h2','','邀请第一位游戏伙伴'),el('p','social-muted','分享你的好友码，或输入朋友的好友码。\n成为好友后，就能查看游戏动态与个人空间。'),action('添加好友',()=>code.focus(),'primary'));main.append(empty);}
 for(const [kind,title]of [['incoming','收到的申请'],['accepted','我的好友'],['outgoing','已发送的申请'],['blocked','已屏蔽']]){
 const group=v.friends.filter(p=>p.relation===kind).sort((a,b)=>Number(connected&&b.presence?.online||false)-Number(connected&&a.presence?.online||false));if(!group.length)continue;
 const section=el('section','social-friend-group');section.append(sectionHeading(title,`${group.length} 位`));const list=el('div','social-friends');
 for(const p of group){const row=el('article','social-friend');row.append(await portrait(p));const info=el('div','social-friend-info');info.append(el('h3','',p.nickname),el('p',`social-status${connected&&p.presence?.online&&kind==='accepted'?' is-online':''}`,p.relation==='accepted'?statusText(p.presence,connected):({incoming:'想和你成为游戏伙伴',outgoing:'等待对方同意',blocked:'已屏蔽'})[p.relation]));row.append(info);const actions=el('div','social-actions');
 if(p.relation==='accepted'){actions.append(action('进入空间',()=>open('space',p.id)),action(p.muted?'取消静音':'静音',()=>relation(p.id,p.muted?'unmute':'mute')),action('删除好友',()=>confirmation(`删除 ${p.nickname}？`,()=>call('relation',{id:p.id,body:{action:'remove'}}))));}
 if(p.relation==='incoming')actions.append(action('接受',()=>relation(p.id,'accept'),'primary'),action('拒绝',()=>relation(p.id,'reject')));
 if(p.relation==='outgoing')actions.append(action('取消申请',()=>relation(p.id,'remove')));
 if(p.relation==='blocked')actions.append(action('取消屏蔽',()=>relation(p.id,'unblock')));else actions.append(action('屏蔽',()=>confirmation(`屏蔽 ${p.nickname}？`,()=>call('relation',{id:p.id,body:{action:'block'}}))));row.append(actions);list.append(row);}
 section.append(list);main.append(section);}
 layout.append(main,aside);element.append(layout);
 }
 async function detail(key){const v=await call('space',{id:owner||me.id,game:key});if(!v.game){await load();throw Error('游戏资料已隐藏或不可访问');}const g=v.game;const body=el('div','social-achievements');body.append(el('p','social-muted',`本地记录 ${hours(g.localSeconds)} · 平台官方 ${hours(g.officialSeconds)}（分别展示，不相加）`));
 if(g.achievements==null)body.append(el('p','','成就未公开'));else if(!g.achievements.length)body.append(el('p','','暂无同步的成就资料'));else for(const a of g.achievements){const row=el('article','social-achievement');const art=el('div','social-achievement-icon');art.append(icon('trophy'));if(a.icon&&/^https:\/\//.test(a.icon)){const img=el('img');img.alt='';img.loading='lazy';img.src=a.icon;img.onerror=()=>art.replaceChildren(icon('trophy'));art.replaceChildren(img);}const copy=el('div');row.append(art,copy);copy.append(el('h3','',a.name),el('p','social-muted',a.description),el('p','',a.unlocked?`已解锁 · ${{manual:'手动标记',automatic:'本地检测',official:'平台官方'}[a.source]||'未知来源'}`:'尚未解锁'));body.append(row);}const dialog=modal(g.title,'来自已同步的游戏资料',body);dialog.classList.add('social-detail-modal');}
 function cover(g,compact=false){
  const nodeKey=JSON.stringify([mode,owner||me.id,g.key,g.coverRevision,g.cover,g.title,compact]);
  const frame=el('div',compact?'social-recent-cover':'social-cover');
  if(coverNodes.has(nodeKey)){frame.append(coverNodes.get(nodeKey));return frame;}
  const media=el('div','social-cover-media');frame.append(media);let priority=0;
  const empty=el('div','social-cover-placeholder');empty.append(icon('game'));if(!compact)empty.append(el('span','',g.title));media.append(empty);
  // Only successfully decoded images can replace a lower-priority source.
  function show(src,rank){if(disposed||rank<=priority)return;const img=el('img');img.alt='';img.decoding='async';img.onload=()=>{if(disposed||rank<=priority)return;priority=rank;media.replaceChildren(img);};img.src=src;}
  if(g.cover)show(g.cover,1);
  if(mode==='mine'&&localGames.has(g.key)){command('get_cached_cover',{gameId:localGames.get(g.key).id,wide:false}).then(path=>{if(path)show(localCoverUrl(path),3);}).catch(()=>{});}
  if(g.coverRevision){const cacheKey=`${owner||me.id}:${g.key}:${g.coverRevision}`;fetchCover(cacheKey,{id:owner||me.id,game:g.key}).then(v=>{if(v&&/^data:image\/jpeg;base64,/.test(v.data))show(v.data,2);else if(!disposed){coverNodes.delete(nodeKey);lastView='';}});}
  coverNodes.set(nodeKey,media);if(coverNodes.size>80)coverNodes.delete(coverNodes.keys().next().value);
  if(cloudCovers.size>80)cloudCovers.delete(cloudCovers.keys().next().value);return frame;
 }
 function gameCard(g){
  const card=action('',()=>detail(g.key),'social-game');card.setAttribute('aria-label',`查看 ${g.title} 的游戏资料`);
  const art=cover(g);const badge=el('span','social-platform',({steam:'Steam',epic:'Epic',local:'本地'})[g.source]||'游戏');art.append(badge);
  if(g.hidden)art.append(el('span','social-hidden-badge','已隐藏'));
  const info=el('div','social-game-info');info.append(el('h3','',g.title));
  const time=g.officialSeconds>0?`平台 ${hours(g.officialSeconds)}`:g.localSeconds>0?`本地 ${hours(g.localSeconds)}`:null;
  const meta=el('div','social-game-meta');meta.append(el('span','',time||'查看游戏资料'),icon('arrow'));info.append(meta);
  if(g.achievementCount>0){const progress=el('div','social-progress');progress.setAttribute('aria-label',`成就 ${g.unlockedCount} / ${g.achievementCount}`);const fill=el('span');fill.style.width=`${Math.min(100,Math.max(0,100*(g.unlockedCount||0)/g.achievementCount))}%`;progress.append(fill);info.append(progress,el('p','social-achievement-caption',`成就 ${g.unlockedCount} / ${g.achievementCount}`));}
  card.append(art,info);return card;
 }
 function sectionHeading(title,detailText){const h=el('div','social-section-heading');h.append(el('h2','',title));if(detailText)h.append(el('span','social-muted',detailText));return h;}
 async function drawSpace(v){
  header(mode==='mine'?'我的空间':'好友空间');
  const hero=el('section','social-hero');const intro=el('div','social-profile');intro.append(await portrait(v.profile));
  const info=el('div','social-profile-copy');const online=connected&&v.presence?.online;
  const status=el('span',`social-status${online?' is-online':''}`,statusText(v.presence,connected));status.setAttribute('role','status');
  info.append(status,el('h2','',v.profile.nickname),el('p','social-bio',v.profile.bio||'每一场冒险，都值得留下足迹。'));intro.append(info);
  if(mode==='mine')intro.append(action('编辑资料',edit,'secondary social-profile-edit'));hero.append(intro);
  if(v.presence.games?.length>1)hero.append(el('p','social-now-playing',v.presence.games.map(g=>g.title).join(' / ')));
  const footer=el('div','social-hero-footer');const stats=el('div','social-stats');
  for(const [value,label]of [[v.libraryVisible?v.total:null,'款游戏'],[v.localSeconds==null?null:hours(v.localSeconds),'本地累计']]){if(value==null)continue;const stat=el('div','social-stat');stat.append(el('strong','',String(value)),el('span','',label));stats.append(stat);}
  footer.append(stats,el('p','social-updated',v.updatedAt?`资料更新于 ${new Date(v.updatedAt).toLocaleString()}`:'游戏资料尚未同步'));hero.append(footer);element.append(hero);
  if(!v.initialized){const empty=el('section','social-empty social-panel');empty.append(el('h2','','旅程，从第一款游戏开始'),el('p','','还没有同步游戏资料。不会自动确认资料导入。'));if(mode==='mine')empty.append(action('前往云同步设置',()=>settings('cloud')));element.append(empty);}
  const layout=el('div',v.recent.length?'social-space-layout':'social-space-layout without-recent');const library=el('section','social-library');
  library.append(sectionHeading('游戏收藏',v.libraryVisible?`${v.total} 款 · 点击封面查看资料`:'游戏库未公开'));
  const grid=el('div','social-games');v.games.forEach(g=>grid.append(gameCard(g)));library.append(grid);
  if(v.libraryVisible&&v.initialized&&!v.games.length)library.append(el('p','social-empty','这里暂时没有可展示的游戏。'));
  const pager=el('div','social-actions social-pagination');if(offset>0)pager.append(action('上一页',async()=>{offset=Math.max(0,offset-24);await load();}));if(v.games.length)pager.append(el('span','social-muted',`${offset+1}–${offset+v.games.length} / ${v.total}`));if(v.nextOffset!=null)pager.append(action('下一页',async()=>{offset=v.nextOffset;await load();}));library.append(pager);layout.append(library);
  if(v.recent.length){const aside=el('aside','social-recent');aside.append(sectionHeading('最近游玩','近期足迹'));const list=el('div','social-recent-list');for(const g of v.recent){const row=el('div','social-recent-item');row.append(cover(g,true));const copy=el('div');copy.append(el('h3','',g.title),el('time','',new Date(g.lastPlayed).toLocaleDateString(undefined,{month:'short',day:'numeric'})));row.append(copy);list.append(row);}aside.append(list);layout.append(aside);}element.append(layout);
 }
 async function load(){
  if(disposed||busy)return;busy=true;const n=++version;
  try{
   const [profile,local]=await Promise.all([call('me'),command('social_local',{includeGames:mode==='mine'})]);
   if(disposed||n!==version)return;
   if(me&&me.id!==profile.id){cloudCovers.clear();coverNodes.clear();lastView='';}
   me=profile;connected=local.status.connected;localGames=new Map(local.games.map(g=>[g.key,g]));
   const v=mode==='friends'?await call('friends'):await call('space',{id:owner||me.id,offset});
   if(disposed||n!==version)return;
   const signature=JSON.stringify([mode,owner,offset,profile,v,connected,local.status.error,local.notifications,local.dnd,mode==='mine'?local.games:null]);
   if(signature===lastView)return;
   element.replaceChildren();
   if(mode==='friends')await drawFriends(v,local);else await drawSpace(v);
   lastView=signature;
  }catch(e){if(!disposed&&n===version){lastView='';cloudCovers.clear();coverNodes.clear();element.replaceChildren();header('好友与空间');element.append(el('p','social-empty',String(e)),action('云账号设置',()=>settings('cloud')),action('重试',load));}}
  finally{busy=false;}
 }
 async function open(next,id){if(busy)return;if(mode!==next||owner!==id){coverNodes.clear();cloudCovers.clear();}lastView='';mode=next;owner=id;offset=0;onNavigate(next);await load();}
 const timer=setInterval(()=>{if(!disposed&&element.isConnected&&!editing&&!document.hidden&&!element.querySelector('input:focus')&&!element.querySelector('input:not([type=checkbox])')?.value&&!document.querySelector('dialog[open]'))load();},10000);
 load();return{element,open,dispose(){disposed=true;version++;clearInterval(timer);coverQueue.length=0;cloudCovers.clear();coverNodes.clear();}};
}
