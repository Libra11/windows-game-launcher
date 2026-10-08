import { el, button } from '../lib/dom.js';
import { preview } from '../lib/bridge.js';
import { settingsSection, settingsToggle } from './settings-components.js';
import { achievementNotificationSettings } from './achievement-notification-settings.js';
import { recentUnlocksDialog } from './recent-unlocks.js';

export async function settingsNotifications(settings, actions, save, options, saved) {
  const element=settingsSection('成就提示','设置新解锁成就的提示位置、停留时间和声音。');
  element.append(settingsToggle('显示成就弹层','只提示新解锁；历史记录不会补发通知。',settings.achievementNotifications,value=>save({achievementNotifications:value})));
  const notification=await achievementNotificationSettings(actions,options,saved);notification.element.classList.add('settings-notification-options');element.append(notification.element);
  const tools=el('div','settings-tools');
  for(const [label,delay] of [['立即测试',0],['5 秒后测试',5]]){
    const control=button(label,'secondary',async()=>{await notification.save();actions.toast(await actions.run('test_achievement_notification',{delaySeconds:delay}));},'trophy');
    control.disabled=preview;tools.append(control);
  }
  tools.append(button('最近解锁','settings-link',()=>recentUnlocksDialog(actions),'clock'));
  element.append(tools,el('p','settings-description','弹层不抢焦点，鼠标可以穿透。游戏使用无边框全屏时显示更稳定。'));
  return {element,dispose(){}};
}
