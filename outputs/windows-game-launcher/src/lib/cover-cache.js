import { convertFileSrc, invoke, isTauri } from '@tauri-apps/api/core';

const pending = new Map();
const knownPaths = new Map();

function coverKey(game, wide) {
  return JSON.stringify([game.id,game.appid,game.source,wide]);
}

export function usesLocalCovers() {
  return typeof window !== 'undefined' && isTauri();
}

export function localCoverPath(info, wide, game) {
  const path = info.localArtwork?.[wide ? 'hero' : 'portrait'];
  if(typeof path === 'string' && path){
    knownPaths.set(coverKey(game,wide),path);
    return path;
  }
  return knownPaths.get(coverKey(game,wide)) || null;
}

export function localCoverUrl(path) {
  return convertFileSrc(path);
}

export function requestLocalCover(game, wide) {
  const key = coverKey(game,wide);
  if (!pending.has(key)) {
    const request = invoke('get_cached_cover', {gameId:game.id, wide})
      .then(path=>{
        if(path)knownPaths.set(key,path);
        return path;
      })
      .catch(() => null)
      .finally(() => pending.delete(key));
    pending.set(key, request);
  }
  return pending.get(key);
}
