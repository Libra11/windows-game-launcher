export function gamer520SearchUrl(query) {
  const url = new URL('https://www.gamer520.com/');
  url.searchParams.set('s', query.trim());
  return url.href;
}

export function createOnlineSearchController(command) {
  const state = { draft:'', scope:'pc', loading:false, error:'', data:null, requested:null };
  const subscribers = new Set();
  let version = 0;
  const notify = () => subscribers.forEach(listener => listener(state));
  async function search(query = state.draft, page = 1) {
    query = query.trim();
    if (!query || [...query].length > 100 || /[\u0000-\u001f\u007f]/.test(query)) {
      state.error = '请输入 1 至 100 个字符的游戏名称'; notify(); return;
    }
    if (state.loading && state.requested?.query === query && state.requested.page === page) return;
    const current = ++version;
    state.requested = { query, page }; state.loading = true; state.error = ''; notify();
    try {
      const data = await command('search_gamer520', { query, page });
      if (current !== version) return;
      state.data = data;
    } catch (error) {
      if (current !== version) return;
      state.error = String(error);
    } finally {
      if (current === version) { state.loading = false; notify(); }
    }
  }
  return {
    state, search,
    retry: () => state.requested && search(state.requested.query, state.requested.page),
    page: page => state.data && search(state.data.query, page),
    scope(value) { if (!['pc','all'].includes(value)) return; state.scope = value; notify(); },
    subscribe(listener) { subscribers.add(listener); listener(state); return () => subscribers.delete(listener); },
  };
}
