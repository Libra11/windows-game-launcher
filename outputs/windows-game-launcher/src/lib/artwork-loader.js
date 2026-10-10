const waiting=new WeakMap();
let observer;

function start(image) {
  const load=waiting.get(image);
  if(!load)return;
  waiting.delete(image);
  delete image.dataset.artworkDeferred;
  observer?.unobserve(image);
  load();
}

function getObserver() {
  if(typeof IntersectionObserver==='undefined')return null;
  observer ||= new IntersectionObserver(entries=>{
    for(const entry of entries){if(entry.isIntersecting&&entry.target.isConnected)start(entry.target);}
  },{rootMargin:'400px 0px'});
  return observer;
}

export function deferArtwork(image,load) {
  waiting.set(image,load);
  image.dataset.artworkDeferred='true';
  // 加载失败后的备用图片可能在已经挂载的列表里创建。
  if(image.isConnected){
    const watcher=getObserver();
    if(watcher)watcher.observe(image);else start(image);
  }
}

export function activateArtwork(root) {
  // 离开页面解除观察，避免观察器持续持有已丢弃分类的图片节点。
  observer?.disconnect();
  const images=root.querySelectorAll('img[data-artwork-deferred]');
  if(!images.length)return;
  const watcher=getObserver();
  images.forEach(image=>{if(watcher)watcher.observe(image);else start(image);});
}
