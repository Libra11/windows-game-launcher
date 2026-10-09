export function artworkSources(info, wide = false) {
  const library = wide ? info.libraryHeroes : info.libraryCovers;
  return [...new Set([
    ...(Array.isArray(library) ? library : []), info.cover, info.icon,
  ].filter(url => typeof url === 'string' && url.trim()))];
}
