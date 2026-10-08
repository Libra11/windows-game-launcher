import { el, button } from '../lib/dom.js';
import { preview } from '../lib/bridge.js';
import { settingsSection, settingsToggle } from './settings-components.js';
import { exitDialog } from './exit-dialog.js';

export function settingsGeneral(settings, actions, save) {
  const element=settingsSection('后台与运行','管理游戏启动后的窗口行为，以及后台记录方式。');
  element.append(settingsToggle('游戏运行后最小化','确认游戏进程后最小化，游戏结束后恢复启动器。',settings.minimizeOnLaunch,value=>save({minimizeOnLaunch:value})));
  element.append(settingsToggle('关闭到托盘','关闭窗口后继续计时与检测成就，通过托盘重新打开。',settings.closeToTray,value=>save({closeToTray:value})));
  const exit=el('div','settings-exit-row');const copy=el('div');copy.append(el('strong','','退出游迹'),el('p','settings-description','正常保存记录并退出，正在运行的游戏继续运行。'));
  const quit=button('退出启动器','secondary',()=>exitDialog(actions),'close');quit.disabled=preview;
  exit.append(copy,quit);element.append(exit);return {element,dispose(){}};
}
