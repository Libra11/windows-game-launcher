import { isTauri } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { el, button } from '../lib/dom.js';
import { chooseFile, chooseSaveFile, preview } from '../lib/bridge.js';
import { backupFileName, backupPhases } from '../lib/backup.js';
import { getTheme, getThemeColor } from '../lib/theme.js';
import { getFontFamilies } from '../lib/font-family.js';
import { settingsSection } from './settings-components.js';
import { backupPaths } from './backup-paths.js';
import './settings-data.css';
const backupAppearance=()=>({theme:getTheme(),themeColor:getThemeColor(),fonts:getFontFamilies()});

export function settingsData(actions) {
  const element=settingsSection('数据与迁移','备份游戏库与记录，换电脑后恢复并重新定位游戏。');
  const status=el('p','settings-description');status.setAttribute('role','status');status.setAttribute('aria-live','polite');
  const progress=el('progress','backup-progress');progress.hidden=true;progress.setAttribute('aria-label','备份操作进度');
  const cancel=button('取消操作','settings-link',async()=>{
    const accepted=await actions.run('cancel_backup_operation',{operationId});
    if(!disposed)status.textContent=accepted?'正在取消操作…':'操作已进入完成阶段，请等待结果。';
  });cancel.hidden=true;
  let disposed=false,busy=false,stop,operationId='',inspected,editor,pathState={dirty:false,busy:false,remaining:0};
  const exportSection=el('section','settings-service');
  const create=button('创建备份','secondary',()=>perform(async()=>{
    const path=await chooseSaveFile({title:'保存游迹备份',defaultPath:backupFileName(),filters:[{name:'游迹备份',extensions:['youji-backup']}]});
    if(!path)return;
    const result=await actions.run('export_backup',{path,appearance:backupAppearance(),operationId});
    if(disposed)return;
    status.textContent=`备份完成：${result.manifest.counts.games} 款游戏、${result.manifest.counts.unlocks} 条解锁、${result.manifest.counts.tags} 个标签、${result.manifest.counts.collections} 个收藏夹、${result.manifest.counts.covers} 张封面。保存到 ${result.path}`
      +(result.manifest.missingCovers.length?`；${result.manifest.missingCovers.length} 项封面缺失，已跳过。`:'');
    actions.toast('备份已完成');
  }),'folder');
  exportSection.append(el('h3','','创建备份'),el('p','settings-description','包含游戏、星标、标签、收藏夹、时长、成就、外观和当前封面。不包含游戏程序、存档、字体文件或登录凭证。游戏运行时可保存当前检查点。'),create);
  const restoreSection=el('section','settings-service'),review=el('div','backup-review');
  const select=button('选择备份','secondary',()=>perform(async()=>{
    const path=await chooseFile({multiple:false,title:'选择游迹备份',filters:[{name:'游迹备份',extensions:['youji-backup']}]});
    if(!path)return;
    const inspection=await actions.run('inspect_backup',{path,operationId});
    if(disposed)return;
    inspected=inspection;confirmed.checked=false;editor?.dispose();review.replaceChildren();
    const heading=el('div','backup-summary');
    heading.append(el('p','settings-description',`备份版本 ${inspection.manifest.appVersion} · ${new Date(inspection.manifest.createdAt).toLocaleString('zh-CN')}`));
    const table=el('table','backup-counts');
    const head=el('tr');for(const name of ['内容','备份数据','当前数据'])head.append(el('th','',name));table.append(head);
    for(const [key,label] of [['games','游戏'],['favorites','星标收藏'],['tags','标签'],['collections','收藏夹'],['sessions','游玩会话'],['achievements','成就定义'],['unlocks','解锁记录'],['covers','封面']]){
      const row=el('tr');row.append(el('td','',label),el('td','',String(inspection.manifest.counts[key])),el('td','',String(inspection.current[key])));table.append(row);
    }
    heading.append(table,el('p','settings-description','完整恢复会替换当前游戏库及记录，并清除账号授权。恢复前会自动备份当前数据，网络代理保留在这台电脑。'));
    editor=backupPaths(inspection,actions,state=>{pathState=state;confirmed.checked=false;controls();});
    review.append(heading,editor.element,confirmation,restore);
    status.textContent='备份已通过校验。检查目录映射和待处理路径，然后确认恢复。';controls();
  }),'folder');
  const confirmation=el('label','backup-confirmation');
  const confirmed=el('input');confirmed.type='checkbox';confirmed.onchange=()=>controls();
  confirmation.append(confirmed,el('span','','我确认用此备份完整替换当前数据，并在安全备份完成后重启游迹。'));
  const restore=button('恢复并重启','primary',()=>perform(async()=>{
    const result=await actions.run('schedule_backup_restore',{preparationId:inspected.preparationId,
      relocation:editor.relocation(),currentAppearance:backupAppearance(),operationId});
    if(!disposed)status.textContent=`安全备份已保存到 ${result.safetyBackup}，正在重启恢复…`;
  }),'refresh');
  restoreSection.append(el('h3','','恢复与重新定位'),el('p','settings-description','选择备份后先检查数据数量和游戏路径。恢复时需要退出游戏并停止成就捕获；找不到的程序可稍后补选。'),select,review);
  function controls(){
    editor?.setDisabled(busy||pathState.busy);confirmed.disabled=busy||pathState.busy;
    create.disabled=preview||busy||pathState.busy;
    select.disabled=preview||busy||pathState.busy;
    restore.disabled=preview||busy||!inspected||!confirmed.checked||pathState.dirty||pathState.busy;
  }
  async function perform(action){
    if(busy)return;busy=true;operationId=crypto.randomUUID();controls();progress.hidden=false;cancel.hidden=false;progress.removeAttribute('value');
    try{await action();}catch(error){if(!disposed)status.textContent=String(error);}
    finally{busy=false;if(!disposed){progress.hidden=true;cancel.hidden=true;controls();}}
  }
  element.append(exportSection,restoreSection,progress,status,cancel);controls();
  if(isTauri())listen('data-backup-progress',event=>{
    if(disposed||event.payload.operationId!==operationId)return;
    const {phase,completed,total}=event.payload;
    if(['commit','restart','complete'].includes(phase))cancel.hidden=true;
    status.textContent=backupPhases[phase]||'正在处理数据';
    if(total>0){progress.max=total;progress.value=completed;}else progress.removeAttribute('value');
  }).then(cancel=>{if(disposed)cancel();else stop=cancel;}).catch(()=>{});
  actions.run('get_backup_restore_status').then(notice=>{
    if(!notice||disposed)return;
    const result=el('div','backup-last-result');result.append(el('h3','','上次恢复结果'),el('p','settings-description',notice.message));
    if(notice.safetyBackup)result.append(el('p','settings-description',`恢复前安全备份：${notice.safetyBackup}`));
    if(notice.missingPaths)result.append(el('p','settings-description',`${notice.missingPaths} 款游戏的路径仍待定位，可在游戏详情中编辑。`));
    element.append(result);
  }).catch(()=>{});
  return {element,dispose(){disposed=true;stop?.();editor?.dispose();}};
}
