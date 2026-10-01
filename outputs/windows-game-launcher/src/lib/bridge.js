import { invoke, isTauri } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { open } from '@tauri-apps/plugin-dialog';
export const preview = import.meta.env.DEV && !isTauri();
const demoGames = [
  ['1123050', 'GRIME', 'local', '探索超现实的异域世界，用活体武器吞噬敌人，在不断进化中寻找自己的起源。'],
  ['1245620', '艾尔登法环', 'steam', '在交界地展开冒险，探索辽阔世界，揭开艾尔登法环的秘密。'],
  ['367520', '空洞骑士', 'steam', '深入古老王国的地下，探索错综复杂的洞穴与废墟。'],
  ['1086940', '博德之门 3', 'steam', '集结你的队伍，返回被遗忘的国度，书写属于你的故事。'],
  ['1145360', '哈迪斯', 'steam', '运用奥林匹斯众神的力量，逃离冥界。'],
  ['2379780', '小丑牌', 'steam', '创造不可思议的组合，迎接每一场盲注。'],
  ['413150', '星露谷物语', 'steam', '从一片旧农场开始，找到属于自己的生活节奏。'],
  ['588650', '死亡细胞', 'local', '挑战变化莫测的城堡，战斗、失败，再试一次。'],
].map(([appid, title, source, description], index) => ({ id: `${source}-${appid}`, appid, title, source, metadataJson: JSON.stringify({ description }), schemaSource: 'Steam Web API', scanStatus: source === 'local' ? '已读取本地记录' : 'Steam 成就已同步', sourceFile: source === 'local' ? 'C:\\Users\\Player\\AppData\\LocalLow\\Clover Bite\\GRIME\\Save Files\\save.gd' : 'Steam Web API', lastScan: new Date().toISOString(), customUnlockPath: '', exePath:'C:\\Games\\Game\\game.exe', favorite:index === 0, lastPlayed:index < 3 ? new Date(Date.now() - index * 86400000).toISOString() : '', playedSeconds:index < 3 ? 3600 * (index + 1) : 0, runtime:{state:'idle',message:'',elapsedSeconds:0} }));
const demoInstallations = {
  '1245620':['installed','示例：已确认本机安装'],
  '367520':['installed','示例：已确认本机安装'],
  '1086940':['not_installed','示例：此游戏尚未在本机安装'],
  '1145360':['not_installed','示例：安装尚未完成，请在 Steam 中完成安装'],
  '2379780':['unknown','示例：游戏库所在磁盘无法访问'],
  '413150':['not_installed','示例：此游戏尚未在本机安装'],
};
demoGames.forEach(game => {
  const entry = demoInstallations[game.appid];
  game.installation = game.source === 'steam' ? {state:entry[0],reason:entry[1],checkedAt:new Date().toISOString()} : {state:'installed',reason:'示例：本地启动文件可用',checkedAt:new Date().toISOString()};
  const steamHours = {'1245620':128,'367520':36,'1086940':82,'1145360':24,'2379780':17,'413150':0};
  game.playtime = { source:game.source, seconds:game.source === 'steam' ? steamHours[game.appid] * 3600 : game.playedSeconds,
    lastPlayed:game.source === 'steam' ? game.lastPlayed : '', checkedAt:game.source === 'steam' ? new Date().toISOString() : '', state:'ready',
    reason:game.source === 'steam' ? '示例：Steam 官方累计时长' : '示例：启动器自动记录的本地时长' };
});
try {
  const favorites = JSON.parse(localStorage.getItem('launcher-preview-favorites') || 'null');
  if (Array.isArray(favorites)) demoGames.forEach(game => { game.favorite = favorites.includes(game.id); });
} catch { /* 示例偏好不可读取时使用默认收藏。 */ }
const demoAchievements = [
  ['初次觉醒', '激活你的第一个替身。'], ['吸收', '获得你的第一个特性。'], ['巨石之下', '击败第一个强大的对手。'], ['探索者', '发现一处隐藏区域。'], ['全副武装', '收集新的武器。'], ['新的道路', '解锁新的移动能力。'], ['不屈', '继续你的旅程。'], ['蜕变', '发掘身体中的潜能。'],
].map(([name, description], i) => ({ apiName: `DEMO_${i}`, name, description, icon: '', unlockedAt: i < 3 ? '2026-09-28T13:30:00Z' : null, unlockSource: 'local' }));
export async function command(name, args = {}) {
  if (!preview) {
    if (!isTauri()) throw new Error('请在桌面启动器中使用此功能。');
    return invoke(name, args);
  }
  if (name === 'list_games') return demoGames;
  if (name === 'epic_connection_status') return { connected:false, displayName:'' };
  if (name === 'remove_local_game') {
    const index = demoGames.findIndex(game => game.id === args.gameId);
    if (index < 0) throw new Error('游戏不存在或已被移除');
    if (demoGames[index].source !== 'local') throw new Error('只能移除本地游戏');
    demoGames.splice(index, 1);
    return;
  }
  if (name === 'search_steam_games') {
    const aliases = { '1123050':'grime', '1245620':'elden ring', '367520':'hollow knight', '1086940':'baldurs gate 3', '1145360':'hades', '2379780':'balatro', '413150':'stardew valley', '588650':'dead cells' };
    const normalize = value => value.toLowerCase().replace(/[\s:'’]/g, '');
    const query = normalize(args.query.trim());
    if (!query) return [];
    return demoGames.filter(game => normalize(`${game.title} ${aliases[game.appid]}`).includes(query))
      .map(game => ({ appid:game.appid, name:game.title, image:`https://shared.fastly.steamstatic.com/store_item_assets/steam/apps/${game.appid}/header.jpg` }));
  }
  if (name === 'set_favorite') {
    const game = demoGames.find(item => item.id === args.gameId); if (!game) throw new Error('游戏不存在');
    game.favorite = args.favorite;
    try { localStorage.setItem('launcher-preview-favorites',JSON.stringify(demoGames.filter(item=>item.favorite).map(item=>item.id))); } catch { /* 示例偏好无法保存时仍可预览。 */ }
    return;
  }
  if (name === 'check_detection') {
    const game = demoGames.find(item=>item.id === args.gameId);
    return {state:'ready',message:'示例：已读取本地记录',schemaSource:game.schemaSource,sourceFile:game.sourceFile,lastScan:game.lastScan,definitions:8,unlocked:3,candidates:[{path:game.sourceFile,exists:true,kind:'示例记录'}]};
  }
  if (name === 'list_achievements') return demoAchievements;
  if (name === 'get_settings') return { steamApiKey: '', steamId: '', minimizeOnLaunch:false, closeToTray:true, achievementNotifications:true };
  if (name === 'list_recent_unlocks') return [];
  throw new Error('当前为设计预览，请在桌面应用中执行此操作。');
}
export const chooseFile = options => preview ? Promise.resolve(null) : open(options);
export const onUnlock = handler => isTauri() ? listen('achievement-unlocked', handler) : Promise.resolve(() => {});
export const onLibraryChange = handler => isTauri() ? listen('library-changed', handler) : Promise.resolve(() => {});
export const onLauncherError = handler => isTauri() ? listen('launcher-error', handler) : Promise.resolve(() => {});
