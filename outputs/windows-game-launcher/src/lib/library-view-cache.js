import { librarySnapshot } from './library-refresh.js';
import { queryGames } from './library-query.js';

// 每种模式只保留最后一份列表，返回页面不再重建全部卡片和控件。
export function createLibraryViewCache() {
  const views=new Map();
  return (mode,state,build)=>{
    const big=mode==='big-screen';
    const options=big
      ? [state.bigCategory,state.bigCollection,state.sort,state.installedOnly]
      : [state.filter,state.search,state.sort,state.installedOnly,state.view];
    // 秒数变化由局部补丁更新，只有时长排序顺序真的变化才重建列表。
    const order=state.sort==='time'
      ? queryGames(state.games,{category:big?state.bigCategory:state.filter,
        collection:big?state.bigCollection:'all',search:big?'':state.search,
        installedOnly:state.installedOnly,sort:'time'}).map(game=>game.id)
      : null;
    const key=librarySnapshot(state.games)+'|'+JSON.stringify([options,order]);
    const cached=views.get(mode);
    if(cached?.key===key)return cached.element;
    const element=build();
    views.set(mode,{key,element});
    return element;
  };
}
