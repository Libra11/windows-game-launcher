export function localArtworkKey(item, role = 'poster') {
  return item?.recognized === false && ['poster', 'backdrop'].includes(role)
    ? item.localArtworkKey || ''
    : '';
}

export function mediaArtworkSource(item, role = 'poster') {
  return {
    backdrop: item.backdropUrl,
    poster: item.posterUrl || item.cover,
    logo: item.logoUrl,
    profile: item.profileUrl,
  }[role] || '';
}

// 片库以季为单位展示；季没有海报时，图片和缓存请求都归属整剧。
export function mediaPosterOwner(item, byId) {
  if (item?.kind !== 'season' || mediaArtworkSource(item) || !item.parentId) return item;
  const parent = byId.get(item.parentId);
  return parent?.kind === 'series' && mediaArtworkSource(parent) ? parent : item;
}
