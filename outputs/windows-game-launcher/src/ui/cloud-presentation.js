const labels={
  game:'游戏条目',title:'游戏名称',metadata_reference:'资料关联',achievement_reference:'成就关联',
  archive:'游戏归档',favorite:'星标收藏',tag:'标签',collection:'收藏夹',tag_member:'标签归属',
  collection_member:'收藏夹归属',collection_order:'收藏夹顺序',platform_account:'平台账号',
  automatic_unlock:'自动成就',manual_unlock:'手动成就',official_snapshot:'官方成就',
  play_session:'游玩记录',definition_cache:'成就资料',official_playtime:'官方时长',
};
export const resourceLabel=kind=>labels[kind]||'同步记录';
export function parseCloudValue(value){try{return typeof value==='string'?JSON.parse(value):value;}catch{return null;}}
export function describeCloudValue(kind,payload,deleted=false){
  if(deleted)return '已删除';
  const p=parseCloudValue(payload);
  if(!p)return '暂无记录';
  if(kind==='favorite')return p.value?'已收藏':'未收藏';
  if(kind==='archive')return p.value?'已归档':'在游戏库中';
  if(kind==='manual_unlock')return p.present?'已手动解锁':'已取消手动解锁';
  if(kind.endsWith('_member'))return p.present?'已加入':'已移出';
  if(kind==='collection_order')return `${p.ids?.length||0} 个收藏夹的排列顺序`;
  if(kind==='official_snapshot')return `${Object.keys(p.unlocks||{}).length} 项官方解锁`;
  if(kind==='play_session')return `${Math.floor((p.seconds||0)/60)} 分钟游玩记录`;
  return p.name||p.value||p.displayTitle||'记录已更新';
}
