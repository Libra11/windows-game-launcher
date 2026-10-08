import { achievementNotificationSettings } from './achievement-notification-settings.js';
import { xboxConnection } from './xbox-connection.js';
import { epicConnection } from './epic-connection.js';
import { el, button, icon } from '../lib/dom.js';
import { preview } from '../lib/bridge.js';
import { themePicker } from './theme-controls.js';
import { modal, closeModal } from './modal.js';
import { field, submit } from './form-fields.js';
import { recentUnlocksDialog } from './recent-unlocks.js';
import { exitDialog } from './exit-dialog.js';

function check(label, checked) {
  const element = el('label','setting-check'); const input = el('input'); input.type = 'checkbox'; input.checked = checked;
  element.append(input, el('span','',label)); return {element,input};
}

export async function settingsDialog(actions) {
  const settings = await actions.run('get_settings');
  const form = el('form', 'form');
  const appearance = themePicker(); form.append(appearance.element);
  const minimize = check('确认游戏运行后最小化，结束后恢复启动器', settings.minimizeOnLaunch);
  const tray = check('关闭窗口时保留在托盘，继续计时和检测成就', settings.closeToTray);
  const notifications = check('显示游迹成就弹层', settings.achievementNotifications);
  form.append(minimize.element, tray.element, notifications.element);
  const achievementOptions = await achievementNotificationSettings(actions);
  form.append(achievementOptions.element);
  const tools = el('div','settings-tools');
  const test = button('测试通知','secondary',async () => {
    await achievementOptions.save();
    const message = await actions.run('test_achievement_notification'); actions.toast(message);
  },'trophy'); test.disabled = preview; if (preview) test.title = '请在桌面应用中测试成就弹层';
  const delayed = button('5 秒后测试', 'secondary', async () => {
    await achievementOptions.save();
    actions.toast(await actions.run('test_achievement_notification', {delaySeconds:5}));
  }, 'trophy'); delayed.disabled = preview;
  tools.append(delayed);
  tools.append(test, button('最近解锁','secondary',()=>recentUnlocksDialog(actions),'clock'));
  form.append(tools, el('p','form-hint','游迹弹层置顶显示，不抢焦点，鼠标可穿透。建议游戏使用无边框全屏；独占全屏可能遮挡弹层。错过的成就可在“最近解锁”查看。'));
  const connection = el('div','connection-card'); connection.append(icon('steam'), el('div','','连接 Steam 游戏库')); form.append(connection);
  const key = field('Steam Web API Key', settings.steamApiKey, '填写你的 Steam API Key', 'password');
  const id = field('SteamID64', settings.steamId, '17 位 Steam 账号 ID'); id.input.pattern = '[0-9]*'; id.input.inputMode = 'numeric';
  form.append(key.wrapper,id.wrapper,el('p','form-hint','用于导入游戏、同步官方成就与累计时长。游戏详情及游玩时长需公开，Key 和记录保存在本机。'));
  const xbox = xboxConnection(actions); form.append(xbox.element);
  const epic = epicConnection(actions); form.append(epic.element);
  const dialog = modal('设置','管理外观、后台运行与游戏账号连接。',form);
  dialog.addEventListener('close',()=>{appearance.dispose();xbox.dispose();epic.dispose();},{once:true});
  submit(form,'保存设置',async()=> {
    await achievementOptions.save();
    await actions.run('save_settings',{steamApiKey:key.input.value.trim(),steamId:id.input.value.trim(),minimizeOnLaunch:minimize.input.checked,closeToTray:tray.input.checked,achievementNotifications:notifications.input.checked});
    await closeModal(dialog); await actions.refresh(); actions.toast('设置已保存');
  });
  const exit = button('退出启动器','settings-exit',async()=> {await closeModal(dialog); await exitDialog(actions);},'close'); exit.disabled = preview;
  form.append(exit);
}
