export function onlineGamePresentation(title = '') {
  const parts = title.split('|').map(part => part.trim()).filter(Boolean);
  const edition = parts.slice(1).join(' · ');
  const badges = [];
  if (/中文|中字|国语/.test(edition) && !/(?:无|非|不支持|没有)\s*中文/.test(edition)) badges.push('中文');
  if (/DLC/i.test(edition) && !/(?:无|不含|没有)\s*DLC/i.test(edition)) badges.push('含 DLC');
  if (/支持手柄/.test(edition) && !/(?:不|未|无)\s*支持手柄/.test(edition)) badges.push('支持手柄');
  return { name:parts[0] || title, edition, badges };
}

export function onlineGameType(item) {
  if (item.isPc || item.categories?.some(value => value === 'PC PLAY')) return 'PC 游戏';
  if (item.categories?.some(value => value.includes('Switch'))) return 'Switch';
  return item.categories?.[0] || '游戏资源';
}
