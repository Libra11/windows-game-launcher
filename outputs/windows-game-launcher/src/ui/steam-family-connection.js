import { isTauri } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { el, button, icon } from '../lib/dom.js';
import { preview } from '../lib/bridge.js';

export function steamFamilyConnection(actions,{beforeLogin,onConnected}) {
  const element=el('section','form steam-family-connection');
  const heading=el('div','connection-card');heading.append(icon('steam'),el('div','','Steam 家庭游戏库'));
  const status=el('p','form-hint','正在读取家庭库连接状态…');status.setAttribute('aria-live','polite');
  let disposed=false,busy=false,connection,stop;
  function controls() {
    login.disabled=preview||busy;
    importGames.disabled=preview||busy||!connection?.connected;
    syncTime.disabled=preview||busy||!connection?.connected||!connection?.hasFamily;
    disconnect.disabled=preview||busy;
    disconnect.hidden=!connection?.steamId;
  }
  async function refresh(message) {
    const next=await actions.run('steam_family_connection_status');
    if(disposed)return;
    connection=next;
    if(next.connected)await onConnected?.(next);
    if(disposed)return;
    status.textContent=message||(next.connected
      ? next.hasFamily?`已连接 ${next.familyName||'Steam 家庭'}。可导入尚未安装的共享游戏。`:'已登录 Steam；该账号尚未加入家庭组，可加入后再导入。'
      : next.expired?'Steam 家庭库授权已过期，请重新登录。'
      : next.accountMismatch?'家庭库登录账号与填写的 SteamID64 不一致，请重新登录对应账号。'
      :'登录 Steam 后，读取你的家庭共享游戏。API Key 仍用于个人成就和官方时长。');
    login.querySelector('span').textContent=next.connected?'重新登录 Steam':'登录 Steam 家庭库';
    controls();
  }
  async function perform(action,message) {
    if(busy)return;
    busy=true;controls();status.textContent=message;
    try{await action();}catch(error){if(!disposed)status.textContent=String(error);}
    finally{busy=false;if(!disposed)controls();}
  }
  const login=button('登录 Steam 家庭库','secondary',()=>perform(async()=>{
    if(!await beforeLogin())return refresh();
    await actions.run('steam_family_begin_login');
    // 等待登录时可再次打开同一窗口，不重复创建授权窗口。
    if(!disposed)status.textContent='Steam 官方登录窗口已打开，请完成扫码或 Steam Guard 验证。';
  },'正在打开 Steam 官方登录窗口…'),'steam');
  const importGames=button('导入家庭游戏','secondary',()=>perform(async()=>{
    const result=await actions.run('import_steam_family');
    await actions.refresh();
    const message=result.hasFamily
      ? `家庭库已同步：新增 ${result.added} 款，家庭共享 ${result.shared} 款，自有 ${result.owned} 款。`
        +(result.timeGames?`已读取 ${result.timeGames} 款游戏的本人时长。`:'')
        +(result.excluded?`已跳过 ${result.excluded} 项不可共享或非游戏内容。`:'')
        +(result.unavailable?`${result.unavailable} 款原共享游戏已标记不可用，历史记录保留。`:'')
      :'该账号目前没有家庭组，原共享条目已标记不可用，游戏和记录仍保留。';
    await refresh(message);actions.toast(message);
  },'正在读取 Steam 家庭共享库…'),'refresh');
  const syncTime=button('同步本人时长','secondary',()=>perform(async()=>{
    const result=await actions.run('refresh_steam_family_playtime');
    await actions.refresh();
    await refresh(`已读取 ${result.games} 款游戏的本人时长，其中 ${result.sharedGames} 款为家庭共享。`);
    actions.toast(`已更新 ${result.games} 款游戏的 Steam 本人时长`);
  },'正在读取当前账号的个人游戏时长…'),'clock');
  const disconnect=button('断开家庭库连接','secondary',()=>perform(async()=>{
    await actions.run('steam_family_disconnect');
    await refresh('家庭库连接已断开，已导入的游戏与记录保留。');
  },'正在断开家庭库连接…'));
  const tools=el('div','settings-tools');tools.append(login,importGames,syncTime,disconnect);
  element.append(heading,status,tools,el('p','form-hint','密码与扫码验证仅在 Steam 官方页面处理，游迹不读取密码。读取家庭库的登录凭证保存在本机，过期后重新登录。游戏共享资格与可用副本最终由 Steam 客户端确认。'));
  controls();refresh().catch(error=>{if(!disposed)status.textContent=String(error);});
  if(isTauri())listen('steam-family-auth',async event=>{
    if(disposed)return;
    try{
      await refresh(event.payload.state==='connected'?undefined:event.payload.message);
      if(event.payload.state==='connected')actions.toast(connection?.hasFamily?'Steam 家庭库已连接，可以导入共享游戏':'Steam 已登录，该账号目前没有家庭组');
    }catch(error){if(!disposed)status.textContent=String(error);}
  }).then(cancel=>{if(disposed)cancel();else stop=cancel;}).catch(()=>{});
  return {element,dispose(){disposed=true;stop?.();}};
}
