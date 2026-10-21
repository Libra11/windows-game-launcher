import { el, button } from '../lib/dom.js';
import { command, preview } from '../lib/bridge.js';
import { modal, closeModal } from './modal.js';
import { submit } from './form-fields.js';
import './save-forms.css';
export function checkbox(text, checked=false) {
  const label=el('label','save-option'),input=el('input');input.type='checkbox';input.checked=checked;
  label.append(input,el('span','',text));return {label,input};
}
const phases={pending:'等待处理',waitingIdle:'等待游戏退出',waitingStable:'等待文件稳定',validating:'云端校验中',retrying:'等待重试',attention:'需要处理'};
export function taskSummary(settings,tasks) {
  return tasks.length?tasks.map(t=>`${t.kind==='backup'?'备份':'上传'}：${phases[t.phase]||t.phase}${t.error?' · '+t.error:''}`).join('；'):
    settings.notice||'尚无自动任务';
}
export function automationSettings(profile,settings,refresh) {
  const form=el('form','form');
  const backup=checkbox('退出游戏后自动备份',settings.autoBackup),upload=checkbox('自动上传云端并清理过期版本',settings.autoUpload);
  upload.input.disabled=!profile.cloudId;
  form.append(backup.label,upload.label,el('p','','保留最近 10 份及近 7 天每日最后一份；当前进度、分叉末端、未完成上传和手动保留的版本受保护。游迹退出后暂停，下次启动继续。'));
  if(!profile.cloudId)form.append(el('p','','请先关联云存档，再开启自动上传。'));
  const dialog=modal('自动存档备份',profile.name,form);
  submit(form,'保存设置',async()=>{
    await command('saves_automation_settings',{profileId:profile.id,autoBackup:backup.input.checked,autoUpload:upload.input.checked});
    await refresh();await closeModal(dialog);
  });
}
export async function restoreWithReview(profile,snapshotId,refresh=async()=>{}) {
  const review=await command('saves_review',{profileId:profile.id,snapshotId});
  return new Promise(resolve=>{
    const form=el('form','form');let completed=false;
    form.append(el('p','',`当前目录：${profile.root||profile.name}；本地 ${review.localFiles} 个文件。恢复会替换整个目录，先保存安全备份。请退出游戏及其他存档工具。`));
    form.append(el('p','',`要恢复的版本：${new Date(review.snapshotCreatedAt).toLocaleString()} · ${(Number(review.snapshotBytes||0)/1024/1024).toFixed(2)} MiB`));
    const dialog=modal('确认恢复存档','确认后等待游戏和捕获任务停止；本地内容若变化须重新确认。关闭窗口不会取消已经确认的恢复任务。',form);
    dialog.addEventListener('close',()=>{if(!completed)resolve(false);},{once:true});
    form.append(button('取消','secondary',async()=>{resolve(false);await closeModal(dialog);}));
    submit(form,'备份当前进度并恢复',async()=>{
      await command('saves_restore_review',{reviewId:review.reviewId,confirmed:true});
      completed=true;await refresh();resolve(true);await closeModal(dialog);
    });
  });
}
export async function checkSaveBeforeLaunch(game) {
  if(preview)return true;
  const result=await Promise.race([
    command('saves_preflight',{gameId:game.id}).catch(()=>({offline:true,notices:[]})),
    new Promise(resolve=>setTimeout(()=>resolve({offline:true,notices:[]}),3000)),
  ]);
  if(!result.notices?.length)return true;
  return new Promise(resolve=>{
    const content=el('div','form');let decided=false;
    const dialog=modal('云端有不同的存档版本','不会自动覆盖本机。时间较晚不代表游戏进度较新。',content);
    dialog.addEventListener('close',()=>{if(!decided)resolve(false);},{once:true});
    content.append(button('使用本地继续','primary',async()=>{decided=true;resolve(true);await closeModal(dialog);}));
    for(const n of result.notices){
      const row=el('section','form');row.append(el('h3','',n.name),el('p','',n.forked?'存在多个进度分支，请查看历史后选择。':`云端版本：${n.cloud?.serverCreatedAt?new Date(n.cloud.serverCreatedAt).toLocaleString():'未知时间'}；本地${n.localSnapshotId?'有备份来源':'尚未承接云端版本'}`));
      const status=el('p','');status.setAttribute('role','status');
      let restoring=false;
      if(!n.forked&&n.cloud?.available!==false)row.append(button('恢复云端版本','secondary',async()=>{
        if(restoring)return;restoring=true;
        try{status.textContent='正在下载并校验…';await command('saves_download',{profileId:n.profileId,snapshotId:n.snapshotId});
          if(await restoreWithReview({id:n.profileId,name:n.name},n.snapshotId)){decided=true;resolve(true);await closeModal(dialog);}else status.textContent='已取消恢复';
        }catch(e){status.textContent=String(e);}finally{restoring=false;}
      }));
      row.append(button('查看历史','secondary',async()=>{const {openSaves}=await import('./detail-saves.js');decided=true;resolve(false);await closeModal(dialog);openSaves(game);}),status);content.append(row);
    }
  });
}
