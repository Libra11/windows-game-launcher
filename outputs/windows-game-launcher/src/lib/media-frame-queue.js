// 缓存查询先行；仅缺失的截图进入有界解码队列，滚走的请求在启动前丢弃。
export function createFrameQueue(limit = 2) {
  let active = 0;
  const queue = [];
  function drain() {
    while (active < limit && queue.length) {
      const next = queue.shift();
      if (!next.wanted()) { next.resolve(null); continue; }
      active++;
      Promise.resolve().then(next.run).catch(() => null).then(next.resolve).finally(() => { active--; drain(); });
    }
  }
  return (run, wanted) => new Promise(resolve => {
    queue.push({ run, wanted, resolve });
    drain();
  });
}
