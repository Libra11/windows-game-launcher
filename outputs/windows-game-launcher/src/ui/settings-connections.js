import { el, button } from '../lib/dom.js';
import { preview } from '../lib/bridge.js';
import { field, submit } from './form-fields.js';
import { epicConnection } from './epic-connection.js';
import { xboxConnection } from './xbox-connection.js';
import { settingsSection } from './settings-components.js';

export function settingsConnections(settings, actions, save) {
  const element=settingsSection('游戏库与账号','连接游戏账号、导入收藏，或添加本地游戏。');
  const steam=el('form','settings-service form');steam.append(el('h3','','Steam'));
  const key=field('Steam Web API Key',settings.steamApiKey,'填写 Steam Web API Key','password');
  const id=field('SteamID64',settings.steamId,'17 位 Steam 账号 ID');id.input.inputMode='numeric';id.input.pattern='[0-9]*';
  const fields=el('div','settings-credentials');fields.append(key.wrapper,id.wrapper);
  steam.append(el('p','settings-description','用于导入游戏、读取官方成就与累计时长。Key 和账号信息保存在本机。'),fields);
  const saveCredentials=()=>save({steamApiKey:key.input.value.trim(),steamId:id.input.value.trim()});
  submit(steam,'保存 Steam 连接',saveCredentials);
  const tools=el('div','settings-tools');const importSteam=button('导入 Steam 游戏','secondary',async()=>{if(!steam.reportValidity())return;await saveCredentials();await actions.importSteam();},'steam');
  importSteam.disabled=preview;tools.append(importSteam);steam.append(tools);
  const epic=epicConnection(actions);epic.element.classList.add('settings-service');
  const local=el('section','settings-service');
  local.append(el('h3','','本地游戏'),el('p','settings-description','选择本机游戏程序，添加到统一游戏库中。'),button('添加本地游戏','secondary',actions.add,'plus'));
  const xbox=xboxConnection(actions);xbox.element.classList.add('settings-service');
  element.append(steam,epic.element,local,xbox.element);
  return {element,dispose(){epic.dispose();xbox.dispose();key.input.value='';}};
}
