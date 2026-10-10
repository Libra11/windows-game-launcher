import { installState } from './launch-state.js';

export function createGameInstallController({game,command,refresh,changed,notify}) {
  const pending = new Set();
  const decorate = item => { item.installRequestPending = pending.has(item.id); return item; };
  async function install(item) {
    const current = decorate(game(item.id) || item);
    if (installState(current).disabled) return;
    pending.add(item.id); decorate(current); changed();
    try {
      await command('install_game',{gameId:item.id});
      notify(`已请求打开 ${current.title} 的安装界面，请在游戏客户端中确认`);
      await refresh();
    } catch (error) {
      // 后端重新检查发现已安装或共享失效时，刷新最新按钮状态。
      await refresh().catch(()=>{});
      throw error;
    } finally {
      pending.delete(item.id); decorate(current);
      const latest = game(item.id); if (latest) decorate(latest);
      changed();
    }
  }
  return {install,decorate};
}
