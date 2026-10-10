import { el, button } from '../lib/dom.js';
import { settingsSection,settingsToggle } from './settings-components.js';
import { updatePhases,updateBusy,downloadSize,downloadPercent } from '../lib/app-update.js';
import { renderUpdateNotes } from './update-notes.js';
import { modal,closeModal } from './modal.js';
import './settings-updates.css';
export function settingsUpdates(settings,actions,save) {
  const updates=actions.updates;
  const element=settingsSection('关于与更新','查看游迹版本，选择合适的时间更新。');
  const version=el('div','app-update-version'),current=el('strong');version.append(el('span','','当前版本'),current,el('span','app-update-channel','正式版渠道'));element.append(version);
  element.append(settingsToggle('启动时检查更新','只检查正式版本，不会自动下载或安装。',settings.checkUpdatesOnStartup,async value=>{await save({checkUpdatesOnStartup:value});await updates.refresh();}));
  const area=el('section','app-update-area'),heading=el('div','app-update-heading'),target=el('h3'),date=el('span');heading.append(target,date);
  const message=el('p','settings-description');message.setAttribute('role','status');message.setAttribute('aria-live','polite');
  const error=el('p','form-error'),result=el('p','settings-description'),progress=el('progress','app-update-progress');progress.setAttribute('aria-label','更新下载进度');
  const amount=el('span','app-update-amount'),controls=el('div','app-update-actions'),notes=el('div');
  const check=button('检查更新','secondary',()=>updates.check(),'refresh');
  const download=button('下载更新','primary',()=>updates.download(updates.status.preparationId),'arrow');
  const cancel=button('取消下载','secondary',()=>updates.cancel(updates.status.operationId),'close');
  const install=button('安装并重启','primary',()=>{
    const selected=updates.status,body=el('div','organization-dialog');
    body.append(el('p','',`游迹将关闭并安装 ${selected.version}，完成后重新打开。游戏库、账号连接和设置会保留。请先退出游戏并等待成就捕获停止。`));
    const dialog=modal('安装更新','安装前会保存计时记录，不会自动创建数据备份。',body);
    body.append(button('确认安装并重启','primary',async()=>{await closeModal(dialog);await updates.install(selected.preparationId);},'check'));
  },'refresh');
  controls.append(check,download,install,cancel);area.append(heading,message,error,result,progress,amount,controls,notes);element.append(area);
  let notesKey='',disposed=false;
  function paint(status){
    if(disposed||!status)return;
    current.textContent=status.currentVersion||'—';const busy=updateBusy(status);
    target.textContent=status.version?`新版本 ${status.version}`:'版本更新';
    const timestamp=Date.parse(status.publishedAt);date.textContent=Number.isFinite(timestamp)?new Date(timestamp).toLocaleDateString('zh-CN'):'';
    message.textContent=status.supported?(updatePhases[status.phase]||'') : status.reason;
    error.textContent=status.error||'';error.hidden=!status.error;result.textContent=status.installMessage||'';result.hidden=!status.installMessage;
    check.disabled=busy||!status.supported;
    download.hidden=!status.supported||!status.preparationId||status.downloadReady||['checking','preparing','installing'].includes(status.phase);download.disabled=busy;
    install.hidden=!status.supported||!status.downloadReady||!status.preparationId;install.disabled=busy;
    cancel.hidden=status.phase!=='downloading';
    progress.hidden=!['downloading','verifying'].includes(status.phase);const percent=downloadPercent(status);
    if(percent===null)progress.removeAttribute('value');else {progress.max=100;progress.value=percent;}
    amount.hidden=progress.hidden;amount.textContent=`${downloadSize(status.downloaded)}${status.total?` / ${downloadSize(status.total)}`:''}`;
    const displayedNotes=status.version?status.notes:status.currentNotes;
    const next=JSON.stringify([status.version,displayedNotes]);if(next!==notesKey){notesKey=next;notes.replaceChildren();if(displayedNotes||status.version){notes.append(el('h3','app-update-notes-title',status.version?'新版本更新说明':'本版更新说明'),renderUpdateNotes(displayedNotes));}}
  }
  const stop=updates.subscribe(paint);return {element,dispose(){disposed=true;stop();}};
}
