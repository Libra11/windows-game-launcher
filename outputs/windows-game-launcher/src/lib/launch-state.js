const installationLabels = {
  checking:'检查安装中', installed:'开始游戏', not_installed:'未安装',
  client_missing:'Steam 未安装', unknown:'无法确认安装',
};

export function launchState(game) {
  const runtime = game.runtime || {};
  if (runtime.state === 'starting' || runtime.state === 'running') {
    return {label:runtime.state === 'starting' ? '正在启动…' : '游戏运行中', disabled:true, reason:runtime.message || ''};
  }
  if (game.source === 'epic') return { label:'在 Epic 中打开', disabled:false, reason:'通过 Epic 客户端处理安装和启动；指定实际游戏程序后可记录运行时长' };
  if (game.source === 'steam') {
    if(isFamilyUnavailable(game))return {label:'共享已失效',disabled:true,reason:'此游戏的家庭共享已不可用，请在设置中重新导入家庭库确认授权'};
    const installation = game.installation || {state:'checking'};
    return {
      label:installationLabels[installation.state] || installationLabels.unknown,
      disabled:installation.state !== 'installed',
      reason:installation.reason || (installation.state === 'checking' ? '正在检查本机 Steam 安装状态' : '尚未取得可用的安装状态'),
    };
  }
  if (game.installation && game.installation.state !== 'installed') {
    return {label:game.installation.state === 'not_installed' ? '启动文件缺失' : '无法访问游戏',disabled:true,reason:game.installation.reason};
  }
  return {label:'开始游戏',disabled:false,reason:runtime.message || '开始游戏'};
}

export function installationHint(game) {
  if (game.source === 'epic') return '安装状态由 Epic 客户端管理';
  if (['starting','running'].includes(game.runtime?.state)) return '';
  if(isFamilyUnavailable(game))return launchState(game).reason;
  if (game.source === 'local' && !game.installation) return '';
  if (game.installation?.state === 'installed') return '';
  const state = launchState(game);
  return game.installation?.state === 'checking' || !game.installation ? state.label : `${state.label} · ${state.reason}`;
}
import { isFamilyUnavailable } from './family-library.js';
