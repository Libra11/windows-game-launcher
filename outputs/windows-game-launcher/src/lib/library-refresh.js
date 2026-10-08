export function librarySnapshot(games) {
  return JSON.stringify(games.map(({ lastScan, runtime, playedSeconds, playtime, installation, ...game }) => {
    const { seconds, checkedAt, ...time } = playtime || {};
    return { ...game, playtime:playtime && time, installation:installation && { state:installation.state, reason:installation.reason } };
  }));
}

// 重绘时复用相同游戏和地址的封面节点，保留已加载图片及正在进行的请求。
export function preserveArtwork(previous, next) {
  const available = new Map();
  previous.querySelectorAll('[data-artwork-key]').forEach(host => {
    const key = host.dataset.artworkKey;
    if (!available.has(key)) available.set(key, []);
    available.get(key).push(host);
  });
  next.querySelectorAll('[data-artwork-key]').forEach(host => {
    const old = available.get(host.dataset.artworkKey)?.shift();
    if (!old) return;
    // 只移动封面图片和占位符，保留新控件绑定的收藏、启动等事件。
    const art = [...old.children].filter(node => node.tagName === 'IMG' || node.classList.contains('art-letter'));
    [...host.children].filter(node => node.tagName === 'IMG' || node.classList.contains('art-letter')).forEach(node => node.remove());
    art.forEach(node => host.prepend(node));
    host.classList.toggle('art-loaded', old.classList.contains('art-loaded'));
  });
}
