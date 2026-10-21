export function importSummary(value) {
  const counts=value.resourceCounts||{};
  const count=key=>Number(counts[key]||0);
  return [
    ['游戏', counts.game ?? value.localGames?.length ?? 0, '款'],
    ['成就记录', count('automatic_unlock')+count('manual_unlock'), '条'],
    ['游玩记录', count('play_session'), '次'],
    ['收藏夹', count('collection'), '个'],
    ['标签', count('tag'), '个'],
    ['分类归属', count('collection_member')+count('tag_member'), '条'],
    ['成就资料', count('definition_cache'), '款游戏'],
    ['历史时长', count('historical_baseline'), '款游戏'],
    ['官方时长', count('official_playtime'), '款游戏'],
    ['成就统计事件', count('unlock_event'), '条'],
    ['平台账号', count('platform_account'), '个'],
  ];
}

export function importGamePlan(value, game) {
  const selected=(value.mappings||[]).find(m=>m.localId===game.id);
  const existing=(value.cloudGames||[]).find(g=>g.id===selected?.cloudId);
  const candidates=(value.cloudGames||[]).filter(g=>g.source==='local'&&g.namespace===game.namespace);
  return { existing, candidates, needsChoice:game.source==='local'&&!existing&&candidates.length>0 };
}
