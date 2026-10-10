let activePool;

function collectArtwork(roots) {
  const pool=new Map(),seen=new Set();
  for(const root of roots){
    if(!root)continue;
    root.querySelectorAll('[data-artwork-key]').forEach(host=>{
      if(seen.has(host))return;seen.add(host);
      if(![...host.children].some(node=>node.tagName==='IMG'))return;
      const key=host.dataset.artworkCacheKey||host.dataset.artworkKey;
      if(!pool.has(key))pool.set(key,[]);pool.get(key).push(host);
    });
  }
  return pool;
}

// 只保留普通／大屏的最后一份游戏库，不按导航次数累计页面。
export function createArtworkRetainer() {
  const libraries=new Map();
  return (previous,mode,isLibrary,build)=>{
    const parent=activePool;
    const snapshots=[...libraries.values()];
    // 详情与统计不能搬空留给主页的封面；只有回到游戏库时才取用快照。
    const retained=snapshots.includes(previous?.firstElementChild);
    const source=!retained||(isLibrary&&libraries.get(mode)===previous?.firstElementChild)?previous:null;
    // 两种模式现在保留完整列表，不能从另一模式借走其仍要使用的图片。
    activePool=collectArtwork(isLibrary?[source,libraries.get(mode)]:[source]);
    try {
      const next=build();
      if(isLibrary)libraries.set(mode,next);
      return next;
    }finally{activePool=parent;}
  };
}

// 在新图片创建和 src 赋值之前复用，包括尚未完成加载的节点。
export function restoreArtwork(host) {
  const candidates=activePool?.get(host.dataset.artworkCacheKey||host.dataset.artworkKey);
  while(candidates?.length){
    const previous=candidates.shift();
    if(previous===host)continue;
    const images=[...previous.children].filter(node=>node.tagName==='IMG');
    if(!images.length)continue;
    images.forEach(image=>host.append(image));
    host.classList.toggle('art-loaded',previous.classList.contains('art-loaded')||images.some(image=>image.dataset?.artworkReady!=='false'&&image.complete&&image.naturalWidth>0));
    return true;
  }
  return false;
}
