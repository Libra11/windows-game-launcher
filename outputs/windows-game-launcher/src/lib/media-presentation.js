export function presentationKey(item) {
  const id = item?.externalIds?.find(
    (source) =>
      source.provider === 'tmdb' && ['movie', 'tv', 'season'].includes(source.providerType)
  );
  return id ? `${id.providerType}:${id.externalId}` : '';
}

// 附加资料独立加载，不延迟本地详情和播放入口；同作品在当前会话只请求一次。
export function createPresentationLoader({ run, onChange = () => {} }) {
  const entries = new Map();
  return {
    get(item) {
      return entries.get(presentationKey(item)) || { logoUrl: '', people: [], status: 'idle' };
    },
    async load(item, refresh = false) {
      const key = presentationKey(item);
      if (!key) return;
      const previous = entries.get(key);
      if (previous?.status === 'loading' || (!refresh && previous)) return;
      const entry = { logoUrl: '', people: [], ...previous, status: 'loading' };
      entries.set(key, entry);
      try {
        Object.assign(entry, await run('media_presentation', { itemId: item.id, refresh }, false), {
          status: 'ready',
        });
      } catch {
        entry.status = 'error';
      }
      onChange(item.id, key);
    },
  };
}
