import { playTime } from './library-query.js';

export function playtimeText(game) {
  const time = game.playtime;
  const steam = game.source === 'steam';
  const seconds = steam ? time?.seconds : time?.seconds ?? game.playedSeconds ?? 0;
  const label = steam ? 'Steam 总时长' : '本地累计';
  if (seconds == null) return `${label} · ${time?.state === 'pending' ? '待同步' : '未获取'}`;
  return `${label} · ${seconds === 0 ? '0 分钟' : playTime(seconds)}${time?.state === 'cached' ? '（缓存）' : ''}`;
}

export function playtimeHint(game) {
  const time = game.playtime;
  let hint = time?.reason || (game.source === 'steam' ? '等待从 Steam 同步累计时长' : '从启动器启动后，按实际游戏进程自动记录');
  if (time?.checkedAt) hint += `；上次同步：${new Date(time.checkedAt).toLocaleString('zh-CN')}`;
  return hint;
}
