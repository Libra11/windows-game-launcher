// 已加载的图片节点短暂保留，返回页面或滚回原位置时可直接复用。
const RETAINED_ARTWORK_LIMIT = 96;
export function mediaViewSignature(value) {
  return JSON.stringify(value, (key, value) => {
    if (key === 'updatedAt') return undefined;
    if (key === 'positionSeconds') return Number(value) > 0;
    if (key === 'lastPlayed') return !!value;
    return value;
  });
}

export function createMediaViews() {
  const entries = new Map();
  return {
    use(key, value, build) {
      const signature = mediaViewSignature(value);
      const previous = entries.get(key);
      if (previous && previous.signature === signature) {
        if (key.startsWith('art:')) { entries.delete(key); entries.set(key, previous); }
        return previous.node;
      }
      const node = build();
      entries.set(key, { signature, node });
      return node;
    },
    prune() {
      for (const [key, entry] of entries) {
        if (entry.node?.isConnected) continue;
        const image = key.startsWith('art:') && entry.node?.querySelector('img[src]');
        if (!image) entries.delete(key);
      }
      const retained = [...entries].filter(([key, entry]) => key.startsWith('art:') && !entry.node?.isConnected);
      for (const [key] of retained.slice(0, Math.max(0, retained.length - RETAINED_ARTWORK_LIMIT))) entries.delete(key);
    },
    clear() { entries.clear(); },
  };
}

// 只插入、移动和移除实际变化的节点，避免把整个在屏页面先拆下来。
export function reconcileMediaChildren(parent, children) {
  children = children.filter(Boolean);
  let current = parent.firstChild;
  for (const child of children) {
    if (child === current) current = current.nextSibling;
    else parent.insertBefore(child, current);
  }
  while (current) {
    const next = current.nextSibling;
    current.remove();
    current = next;
  }
}

export function mediaDetailScope(items, id) {
  const included = new Set([id]);
  const children = new Map();
  for (const item of items) {
    if (!children.has(item.parentId)) children.set(item.parentId, []);
    children.get(item.parentId).push(item);
  }
  const queue = [id];
  for (let index = 0; index < queue.length; index++)
    for (const item of children.get(queue[index]) || [])
      if (!included.has(item.id)) { included.add(item.id); queue.push(item.id); }
  return items.filter((item) => included.has(item.id));
}
