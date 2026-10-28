const fields = [
  'page',
  'category',
  'collectionId',
  'search',
  'searchQuery',
  'searchResults',
  'searchError',
  'searchPeoplePage',
  'searchItemsPage',
  'sort',
  'view',
  'onlyRecognized',
  'onlyPlayable',
  'watchedFilter',
  'tagIds',
  'batch',
  'selection',
  'selectedId',
  'detail',
  'detailSeasonId',
  'detailEpisodeId',
  'detailResourceId',
  'detailOnlyResources',
  'person',
  'personQuery',
  'sourceId',
  'browsePath',
  'browse',
  'historyDay',
  'historyMonth',
  'historyQuery',
];

// 只保存浏览状态，后台更新的数据、播放快照不会随返回倒退。
export function createMediaNavigation(state, { capture = () => null, restore = () => {} } = {}) {
  const stack = [];
  return {
    push() {
      const values = Object.fromEntries(
        fields.map((key) => [
          key,
          state[key] instanceof Set
            ? new Set(state[key])
            : Array.isArray(state[key])
              ? [...state[key]]
              : ['personQuery', 'historyQuery'].includes(key)
                ? { ...state[key] }
                : state[key],
        ])
      );
      stack.push({ values, ui: capture() });
      if (stack.length > 40) stack.shift();
    },
    back() {
      const previous = stack.pop();
      if (!previous) return false;
      Object.assign(state, previous.values);
      restore(previous.ui);
      return true;
    },
    clear() {
      stack.length = 0;
    },
    discardItems(ids) {
      for (let i = stack.length - 1; i >= 0; i--)
        if (ids.has(stack[i].values.selectedId)) stack.splice(i, 1);
    },
    get length() {
      return stack.length;
    },
    get previous() {
      return stack.at(-1)?.values;
    },
  };
}
