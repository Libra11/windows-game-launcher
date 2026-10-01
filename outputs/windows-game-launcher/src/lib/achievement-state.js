export function hasNoSteamAchievements(game, achievements) {
  return game.source === 'steam' && game.schemaSource === 'Steam Web API（暂无成就）' && achievements.length === 0;
}

export function achievementEmptyMessage(game, achievements) {
  if (achievements.length) return '这个分类下暂时没有成就。';
  if (game.source === 'epic') return game.achievementPlatform?.definitionError || (game.schemaSource === 'Epic 官方成就（暂无成就）' ? '此游戏暂无 Epic 成就，你仍可通过 Epic 客户端游玩。' : '点击“更新资料”获取 Epic 成就列表、详情和账号解锁状态。');
  if (hasNoSteamAchievements(game, achievements)) return '此游戏暂无 Steam 成就，你仍可正常游玩并查看游戏资料。';
  if (game.achievementPlatform?.platform === 'xbox' && game.schemaSource !== 'Xbox 公开成就资料（Exophase）') return '点击“更新资料”获取 Xbox 公开成就名称、条件和图标，无需登录。其他游戏可在编辑游戏中填写公开页面地址。本地事件独立保存。';
  if (game.schemaSource === 'Xbox 本地事件（仅已确认）') return '尚未捕获成功的 Xbox 成就事件。启动游戏后将自动记录；Steam 成就列表不会用于匹配 Xbox ID。';
  if (game.scanStatus?.startsWith('同步失败：')) return '成就资料暂未获取成功，请查看右侧记录状态了解原因。';
  return '尚未获取成就定义，点击“更新资料”从 Steam 获取；API Key 可在设置中配置。';
}
