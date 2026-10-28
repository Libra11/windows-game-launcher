export function mediaSourceLinks(item) {
  const values = [...(item.externalIds || [])];
  if (item.imdbId) values.push({ provider: 'imdb', providerType: 'title', externalId: item.imdbId });
  const seen = new Set();
  return values.flatMap(source => {
    const id = String(source.externalId || '');
    const parts = id.split(':');
    const numeric = parts.every(value => /^\d{1,20}$/.test(value));
    let href = '', label = '';
    if (source.provider === 'douban' && source.providerType === 'subject' && numeric && parts.length === 1) {
      href = `https://movie.douban.com/subject/${id}/`; label = '在豆瓣查看';
    } else if (source.provider === 'tmdb' && numeric) {
      const kind = source.providerType;
      if (kind === 'movie' && parts.length === 1) href = `https://www.themoviedb.org/movie/${id}`;
      if (kind === 'tv' && parts.length === 1) href = `https://www.themoviedb.org/tv/${id}`;
      if (kind === 'season' && parts.length === 2) href = `https://www.themoviedb.org/tv/${parts[0]}/season/${parts[1]}`;
      if (kind === 'episode' && parts.length === 3) href = `https://www.themoviedb.org/tv/${parts[0]}/season/${parts[1]}/episode/${parts[2]}`;
      label = '在 TMDB 查看';
    } else if (source.provider === 'imdb' && /^tt\d{7,12}$/.test(id)) {
      href = `https://www.imdb.com/title/${id}/`; label = '在 IMDb 查看';
    }
    if (!href || seen.has(href)) return [];
    seen.add(href);
    return [{ ...source, href, label }];
  });
}
