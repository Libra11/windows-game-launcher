import { playTime } from './library-query.js';
const syncDateFormat=new Intl.DateTimeFormat('zh-CN',{year:'numeric',month:'numeric',day:'numeric',hour:'numeric',minute:'numeric',second:'numeric'});

export function playtimeText(game) {
  const time = game.playtime;
  const steam = game.source === 'steam';
  const seconds = steam ? time?.seconds : time?.seconds ?? game.playedSeconds ?? 0;
  const label = steam ? time?.source==='steam-family'?'Steam 本人时长':'Steam 总时长' : '本地累计';
  if (seconds == null) return `${label} · ${time?.state === 'pending' ? '待同步' : '未获取'}`;
  return `${label} · ${seconds === 0 ? '0 分钟' : playTime(seconds)}${time?.state === 'cached' ? '（缓存）' : ''}`;
}

export function playtimeHint(game) {
  const time = game.playtime;
  let hint = time?.reason || (game.source === 'steam' ? '等待从 Steam 同步累计时长' : '从启动器启动后，按实际游戏进程自动记录');
  if (time?.checkedAt) {
    const date=new Date(time.checkedAt);
    if(!Number.isNaN(date.getTime()))hint+=`；上次同步：${syncDateFormat.format(date)}`;
  }
  return hint;
}
