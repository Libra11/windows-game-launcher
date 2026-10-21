import { resourceLabel, describeCloudValue, parseCloudValue } from './cloud-presentation.js';
import { el, button } from '../lib/dom.js';
import { field } from './form-fields.js';
import { settingsSection } from './settings-components.js';

export const cloudPhaseLabel = phase => ({
  unbound:'待预览并绑定资料库', login_required:'需要登录', pending:'有待上传的修改',
  conflict:'有待处理的冲突', unsupported:'请在 Windows 桌面端使用云同步', synced:'已同步', error:'同步暂时失败', syncing:'正在同步',
})[phase] || '尚未绑定';

export function settingsCloud(actions) {
  const element=settingsSection('游迹云账号','同步游戏资料、成就与游玩记录。本机路径和平台凭据留在当前设备。');
  const status=el('p','settings-description');status.setAttribute('role','status');
  const form=el('div','settings-service form');
  const url=field('云服务地址','','https://你的云服务域名');
  const email=field('受邀邮箱','','name@example.com');email.input.type='email';
  const name=field('设备名称','我的电脑','例如：掌机');
  const code=field('邮箱验证码','','8 位数字');code.input.autocomplete='one-time-code';
  const tools=el('div','settings-tools'),details=el('div'),preview=el('div'),conflicts=el('div'),devices=el('div');
  let disposed=false,busy=false,challengeId='',bound=false;
  async function run(job) {
    if(busy||disposed)return;busy=true;
    element.querySelectorAll('button').forEach(b=>b.disabled=true);
    try{await job();}catch(error){if(!disposed)status.textContent=String(error);}
    finally{busy=false;if(!disposed)element.querySelectorAll('button').forEach(b=>b.disabled=false);}
  }
  const send=button('发送验证码','secondary',()=>run(async()=>{
    const response=await actions.run('cloud_challenge',{baseUrl:url.input.value,email:email.input.value});
    challengeId=response.challengeId;if(!disposed)status.textContent='若邮箱已获邀请，验证码将发送到邮箱。';
  }));
  const login=button('登录','primary',()=>run(async()=>{
    if(!challengeId)throw new Error('请先发送验证码');
    await actions.run('cloud_verify',{challengeId,code:code.input.value,deviceName:name.input.value});
    code.input.value='';await refresh();
  }));
  let mappings=[],resolutions=[];
  async function prepareImport(){
    const value=await actions.run('cloud_prepare_import',{mappings,resolutions});
    if(disposed)return;
    mappings=value.effectiveMappings||mappings;
    preview.replaceChildren(el('h3','','导入预览'),el('p','settings-description',`共 ${value.resourceCount} 项资源。确认后绑定此账号；路径和凭据不会上传。`));
    for(const warning of value.warnings||[])preview.append(el('p','settings-description',String(warning)));
    for(const game of value.localGames||[]){
      const row=el('label','field');row.append(el('span','',game.title));
      const selected=(value.mappings||[]).find(m=>m.localId===game.id);
      if(game.source!=='local'){
        row.append(el('span','settings-description','按平台身份关联：'+(selected?.cloudId||'新增条目')));
      }else{
        const select=el('select');select.setAttribute('aria-label',game.title+' 的云端关联');
        const auto=el('option','','按身份自动关联或新增');auto.value='';select.append(auto);
        for(const candidate of (value.cloudGames||[]).filter(g=>g.source==='local')){
          const option=el('option','',candidate.title+' · '+candidate.namespace);
          option.value=candidate.id;option.disabled=candidate.namespace!==game.namespace;select.append(option);
        }
        select.value=mappings.find(m=>m.localId===game.id)?.cloudId||'';
        select.onchange=()=>{mappings=mappings.filter(m=>m.localId!==game.id);if(select.value)mappings.push({localId:game.id,cloudId:select.value});confirm.hidden=true;};
        row.append(select,el('small','settings-description','当前成就命名空间：'+game.namespace));
      }preview.append(row);
    }
    for(const conflict of value.conflicts||[]){
      const row=el('section','settings-service');row.append(el('h3','',resourceLabel(conflict.kind)+'：需要选择'),
        el('p','',conflict.reason),el('p','', '本机：'+describeCloudValue(conflict.kind,conflict.local)),
        el('p','', '云端：'+describeCloudValue(conflict.kind,conflict.cloud,conflict.deleted)));
      const choose=(choice,extra={})=>{resolutions=resolutions.filter(r=>r.key!==conflict.key);resolutions.push({key:conflict.key,choice,...extra});confirm.hidden=true;};
      const select=el('select');select.setAttribute('aria-label',resourceLabel(conflict.kind)+' 的导入处理');
      const empty=el('option','','请选择处理方式');empty.value='';select.append(empty);
      for(const choice of conflict.choices||[]){const option=el('option','',({local:'保留本机',cloud:'采用云端',map:'关联已有分类',rename:'重命名本机分类'})[choice]);option.value=choice;select.append(option);}
      const target=el('select');target.setAttribute('aria-label','已有分类');
      for(const c of conflict.candidates||[]){const option=el('option','',c.name);option.value=c.id;target.append(option);}
      const rename=el('input');rename.setAttribute('aria-label','新的分类名称');rename.placeholder='新的分类名称';
      const changed=()=>choose(select.value,select.value==='map'?{targetId:target.value}:select.value==='rename'?{name:rename.value}:{});
      select.onchange=changed;target.onchange=changed;rename.oninput=changed;
      row.append(select);if((conflict.choices||[]).includes('map'))row.append(target);
      if((conflict.choices||[]).includes('rename'))row.append(rename);preview.append(row);
    }
    const confirm=button('确认绑定并导入' ,'primary',()=>run(async()=>{await actions.run('cloud_commit_import');preview.replaceChildren();await refresh();}));
    confirm.hidden=(value.unresolvedCount||0)>0;
    if(confirm.hidden)preview.append(el('p','settings-description','请解决全部冲突并重新预览，尚未提交任何资料。'));
    preview.append(button('按所选关联重新预览' ,'secondary',()=>run(prepareImport)),confirm);
  }
  const prepare=button('预览资料导入','secondary',()=>run(prepareImport));
  const sync=button('立即同步','secondary',()=>run(async()=>{status.textContent='正在同步…';await actions.run('cloud_sync');await refresh();}));
  const logout=button('退出登录','secondary',()=>run(async()=>{await actions.run('cloud_logout');await refresh();}));
  const list=button('查看设备','secondary',()=>run(async()=>{
    const response=await actions.run('cloud_devices');if(disposed)return;devices.replaceChildren();
    for(const d of Array.isArray(response)?response:response.devices||[]){
      const row=el('div','settings-row');row.append(el('span','',d.name||d.deviceName||d.id));
      row.append(button('撤销此设备','secondary',()=>run(async()=>{
        if(!window.confirm('撤销此设备的云访问？本机文件会保留。'))return;
        await actions.run('cloud_revoke_device',{deviceId:d.id||d.deviceId});row.remove();
      })));devices.append(row);
    }
  }));
  tools.append(send,login,prepare,sync,list,logout);
  form.append(url.wrapper,email.wrapper,name.wrapper,code.wrapper,tools);
  element.append(status,form,details,preview,conflicts,devices);
  async function refresh() {
    const data=await actions.run('cloud_status');if(disposed)return;
    bound=!!data.boundUserId;
    if(!url.input.value)url.input.value=data.baseUrl||'';
    url.input.disabled=bound;
    status.textContent=cloudPhaseLabel(data.phase)+(data.error?'：'+data.error:'');
    details.replaceChildren(el('p','settings-description',`待上传：${data.pending||0}；上次成功：${data.lastSuccess||'尚未同步'}`));
    prepare.hidden=bound||!data.loggedIn;sync.hidden=!bound;logout.hidden=!data.loggedIn;
    const library=el('section','settings-service');library.append(el('h3','','游戏归档'));
    const active=(data.games||[]).filter(g=>!g.archived),archived=(data.games||[]).filter(g=>g.archived);
    const select=el('select');select.setAttribute('aria-label','选择要归档的游戏');
    for(const g of active){const option=el('option','',g.title);option.value=g.id;select.append(option);}
    if(active.length)library.append(select,button('归档所选游戏','secondary',()=>run(async()=>{
      if(!select.value)return;await actions.run('cloud_set_archived',{gameId:select.value,archived:true});await refresh();
    })));
    library.append(el('p','settings-description','归档会在全部设备隐藏游戏，保留成就、时长和存档。正在运行的游戏不能归档。'));
    for(const g of archived){const row=el('div','settings-row');row.append(el('span','',g.title),button('恢复到游戏库','secondary',()=>run(async()=>{
      await actions.run('cloud_set_archived',{gameId:g.id,archived:false});await refresh();
    })));library.append(row);}
    details.append(library);
    conflicts.replaceChildren();
    for(const item of data.conflicts||[]){
      const row=el('section','settings-service');row.append(el('h3','',`同步冲突：${resourceLabel(item.kind)}`));
      const result=parseCloudValue(item.result)||{},current=result.current;
      row.append(el('p','',`本机：${describeCloudValue(item.kind,item.local,item.action==='delete')}`),
        el('p','',`云端：${describeCloudValue(item.kind,current?.payload,current?.deleted)}`),
        el('p','settings-description',`来源设备：${current?.deviceId?.slice(0,8)||'未知'}；接收时间：${current?.updatedAt||'未知'}`),
        el('p','settings-description',result.reason||'同一记录已在其他设备修改。'));
      const debug=el('details');debug.append(el('summary','','诊断数据'),el('pre','',item.result));row.append(debug);
      if(['official_snapshot','official_playtime'].includes(item.kind))row.append(el('p','settings-description','官方记录发生冲突，请在游戏详情重新更新资料。旧快照不能强制覆盖。'));
      for(const [label,keepLocal] of [['采用云端',false],['保留本机',true]]){
        if(keepLocal&&['official_snapshot','official_playtime'].includes(item.kind))continue;
        row.append(button(label,'secondary',()=>run(async()=>{await actions.run('cloud_resolve',{operationId:item.operationId,keepLocal});await refresh();})));
      }conflicts.append(row);
    }
  }
  run(refresh);
  const timer=setInterval(()=>{if(!busy&&!disposed)run(refresh);},5000);
  return {element,dispose(){disposed=true;clearInterval(timer);}};
}
