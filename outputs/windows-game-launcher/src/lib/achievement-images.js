export function achievementImageSources(value) {
  if (!value) return [];
  let url;
  try { url = new URL(value); } catch { return []; }
  if (url.username || url.password) return [];
  const legacySteam = url.hostname === 'steamcdn-a.akamaihd.net'
    && /^\/steamcommunity\/public\/images\/apps\/\d+\/[a-f0-9]+\.(jpg|png)$/i.test(url.pathname)
    && ['http:', 'https:'].includes(url.protocol);
  if (legacySteam) {
    const current = new URL(url);
    current.protocol = 'https:';
    current.hostname = 'shared.akamai.steamstatic.com';
    current.port = '';
    current.pathname = current.pathname.replace('/steamcommunity/public/', '/community_assets/');
    url.protocol = 'https:';
    return [current.href, url.href];
  }
  return url.protocol === 'https:' ? [url.href] : [];
}
