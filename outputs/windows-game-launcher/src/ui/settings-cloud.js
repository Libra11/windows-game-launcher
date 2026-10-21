import { resourceLabel, describeCloudValue, parseCloudValue } from './cloud-presentation.js';
import { el, button } from '../lib/dom.js';
import { field } from './form-fields.js';
import { settingsSection } from './settings-components.js';
import { canPollCloud } from '../lib/cloud-polling.js';
import { importSummary, importGamePlan } from './cloud-import-summary.js';

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
  const importNotice=el('p','settings-description cloud-import-notice');
  importNotice.setAttribute('role','status');importNotice.setAttribute('aria-live','polite');
  let disposed=false,busy=false,challengeId='',bound=false,loggedIn=false,detailsSnapshot='';
  async function run(job,background=false) {
    if(busy||disposed)return;busy=true;
    if(!background)element.querySelectorAll('button').forEach(b=>b.disabled=true);
    try{await job();}catch(error){if(!disposed)status.textContent=String(error);}
    finally{busy=false;if(!disposed&&!background)element.querySelectorAll('button').forEach(b=>b.disabled=false);}
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
  function runImport(message,job) {
    return run(async()=>{
      importNotice.textContent=message;preview.setAttribute('aria-busy','true');
      try{await job();}
      catch(error){
        if(!disposed)importNotice.textContent=`操作未完成：${String(error)}。请根据提示处理；不要重复确认导入。`;
        throw error;
      }finally{if(!disposed)preview.setAttribute('aria-busy','false');}
    });
  }
  async function prepareImport(){
    const value=await actions.run('cloud_prepare_import',{mappings,resolutions});
    if(disposed)return;
    mappings=value.effectiveMappings||mappings;
    preview.replaceChildren(el('h3','','导入预览'),el('p','settings-description','以下是本机参与同步的数据。确认后绑定此账号；游戏文件、存档文件、本机路径和登录凭据不会随此次资料导入上传。'));
    const summary=el('div','cloud-import-summary');
    for(const [label,count,unit] of importSummary(value)){
      const item=el('div','cloud-import-stat');item.append(el('strong','',String(count)),el('span','',`${unit} · ${label}`));summary.append(item);
    }
    preview.append(summary,el('p','settings-description','数量按数据类别统计；同一款游戏会包含名称、关联、收藏与归档状态等多条同步记录，不代表新增多款游戏。'));
    const automatic=el('details','cloud-import-automatic'),automaticRows=el('div','cloud-import-games');
    let automaticCount=0,choiceCount=0;
    const attention=el('section','cloud-import-attention');
    for(const warning of new Set(value.warnings||[]))preview.append(el('p','settings-description',String(warning)));
    for(const game of value.localGames||[]){
      const row=el('label','field');row.append(el('span','',game.title));
      const plan=importGamePlan(value,game);
      if(!plan.needsChoice){
        const platform=({steam:'Steam',epic:'Epic',local:'本地游戏'})[game.source]||'游戏';
        row.append(el('span','settings-description',`${platform} · ${plan.existing?'关联云端已有游戏':'新增到云端'}`));
        automaticCount++;automaticRows.append(row);
      }else{
        choiceCount++;
        row.append(el('small','settings-description','云端有成就资料兼容的本地游戏。若为同一款，请选择关联；否则保留为独立游戏。'));
        const select=el('select');select.setAttribute('aria-label',game.title+' 的云端关联');
        const auto=el('option','','保留为独立游戏（不手动关联）');auto.value='';select.append(auto);
        for(const candidate of plan.candidates){
          const option=el('option','',candidate.title);
          option.value=candidate.id;select.append(option);
        }
        select.value=mappings.find(m=>m.localId===game.id)?.cloudId||'';
        select.onchange=()=>{mappings=mappings.filter(m=>m.localId!==game.id);if(select.value)mappings.push({localId:game.id,cloudId:select.value});confirm.hidden=true;};
        row.append(select);attention.append(row);
      }
    }
    const conflictCount=value.unresolvedCount||0;
    preview.append(el('h3','',conflictCount?`需要处理 · ${conflictCount} 项冲突`:'没有待处理的冲突'));
    if(choiceCount)preview.append(el('p','settings-description',`${choiceCount} 款本地游戏可核对关联；不会自动合并。`),attention);
    if(!choiceCount&&!conflictCount)preview.append(el('p','settings-description','游戏关联已自动安排，可以确认导入。'));
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
    if(automaticCount){automatic.append(el('summary','',`自动处理的游戏 · ${automaticCount} 款（查看明细）`),automaticRows);preview.append(automatic);}
    const confirm=button('确认绑定并导入' ,'primary',()=>runImport('正在确认并导入云端，随后会下载合并结果，请稍候…',async()=>{
      confirm.textContent='正在导入…';
      try{
        await actions.run('cloud_commit_import');
        if(disposed)return;
        preview.replaceChildren();importNotice.textContent='导入已完成，资料库已绑定当前云账号。';
        try{await refresh();}catch(error){importNotice.textContent=`导入已完成，但状态刷新失败：${String(error)}`;}
      }catch(error){
        if(String(error).includes('本地数据已变化'))confirm.hidden=true;
        throw error;
      }finally{confirm.textContent='确认绑定并导入';}
    }));
    confirm.hidden=(value.unresolvedCount||0)>0;
    if(confirm.hidden)preview.append(el('p','settings-description','请解决全部冲突并重新预览，尚未提交任何资料。'));
    preview.append(button('按所选关联重新预览' ,'secondary',()=>runImport('正在重新生成导入预览…',prepareImport)),confirm);
    importNotice.textContent=confirm.hidden?'预览已更新，请处理冲突后重新预览。':'预览已就绪，请核对后确认导入。';
  }
  const prepare=button('预览资料导入','secondary',()=>runImport('正在读取本机资料并生成云端预览…',prepareImport));
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
  element.append(status,form,details,preview,importNotice,conflicts,devices);
  async function refresh() {
    const data=await actions.run('cloud_status');if(disposed)return;
    bound=!!data.boundUserId;
    loggedIn=!!data.loggedIn;
    if(!url.input.value)url.input.value=data.baseUrl||'';
    url.input.disabled=bound;
    const message=cloudPhaseLabel(data.phase)+(data.error?'：'+data.error:'');
    if(status.textContent!==message)status.textContent=message;
    prepare.hidden=bound||!data.loggedIn;sync.hidden=!bound;logout.hidden=!data.loggedIn;
    const snapshot=JSON.stringify([data.pending,data.lastSuccess,data.games,data.conflicts]);
    if(snapshot===detailsSnapshot)return;
    detailsSnapshot=snapshot;
    details.replaceChildren(el('p','settings-description',`待上传：${data.pending||0}；上次成功：${data.lastSuccess||'尚未同步'}`));
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
  const timer=setInterval(()=>{if(!busy&&!disposed&&canPollCloud(element,loggedIn))run(refresh,true);},5000);
  return {element,dispose(){disposed=true;clearInterval(timer);}};
}
