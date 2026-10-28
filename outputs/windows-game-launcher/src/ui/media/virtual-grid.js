import { el } from '../../lib/dom.js';
import { reconcileMediaChildren } from '../../lib/media-view.js';

// 以整行分块，只保留视口附近的卡片；离屏块保留高度和滚动位置。
export function createMediaVirtualGrid(views) {
  const root = el('div');
  let values = [], build, mode = 'grid', columns = 0, chunks = [];
  let identity = '', observer, resizeObserver, frame = 0, active = false;

  function renderChunk(chunk) {
    const nodes = values.slice(chunk.start, chunk.end).map((value, index) => {
      const node = build(value, chunk.start + index);
      node.dataset.mediaGridKey = String(value.id ?? value.path);
      node.setAttribute('role', 'listitem');
      node.setAttribute('aria-posinset', String(chunk.start + index + 1));
      node.setAttribute('aria-setsize', String(values.length));
      return node;
    });
    chunk.element.style.minHeight = '';
    reconcileMediaChildren(chunk.element, nodes);
    chunk.mounted = true;
  }
  function unmountChunk(chunk) {
    if (!chunk.mounted || chunk.element.contains(document.activeElement)) return;
    const height = chunk.element.getBoundingClientRect().height;
    if (height) chunk.element.style.minHeight = `${height}px`;
    chunk.element.replaceChildren();
    chunk.mounted = false;
  }
  function schedule() {
    if (!active || frame) return;
    frame = requestAnimationFrame(() => { frame = 0; layout(); });
  }
  function layout() {
    if (!active || !root.isConnected) return;
    const style = getComputedStyle(root);
    const count = mode === 'list' ? 1 : Math.max(1, style.gridTemplateColumns.split(' ').filter(Boolean).length);
    const changed = count !== columns;
    if (changed) {
      columns = count;
      rebuild();
    }
    if (!observer) {
      for (const chunk of chunks) renderChunk(chunk);
    }
  }
  function rebuild() {
    observer?.disconnect();
    const count = Math.max(1, columns);
    const chunkSize = mode === 'list' ? 16 : count * 4;
    const style = root.isConnected ? getComputedStyle(root) : null;
    const gap = parseFloat(style?.rowGap) || (mode === 'list' ? 10 : 26);
    const columnGap = parseFloat(style?.columnGap) || 20;
    const width = root.clientWidth || 900;
    const cardWidth = Math.max(1, (width - (count - 1) * columnGap) / count);
    const rowHeight = mode === 'list' ? 107 : cardWidth * 1.5 + 73;
    chunks = [];
    for (let start = 0; start < values.length; start += chunkSize) {
      const end = Math.min(values.length, start + chunkSize);
      const element = el('div', `media-virtual-chunk ${mode === 'list' ? 'media-list' : 'media-grid'}`);
      const rows = Math.ceil((end - start) / count);
      element.style.minHeight = `${Math.max(1, rows * rowHeight + (rows - 1) * gap)}px`;
      const chunk = { element, start, end, mounted: false, visible: false };
      element.addEventListener('focusout', () => queueMicrotask(() => {
        if (!chunk.visible) { unmountChunk(chunk); views.prune(); }
      }));
      chunks.push(chunk);
    }
    if (chunks.length && !root.isConnected) renderChunk(chunks[0]);
    reconcileMediaChildren(root, chunks.map((chunk) => chunk.element));
    if (root.isConnected) {
      for (const chunk of chunks) {
        const rect = chunk.element.getBoundingClientRect();
        if (rect.bottom >= -600 && rect.top <= window.innerHeight + 600) renderChunk(chunk);
      }
    }
    if (observer) for (const chunk of chunks) observer.observe(chunk.element);
  }
  return {
    restoreAnchor(key) {
      layout();
      if (!active || key == null) return;
      const index = values.findIndex(value => String(value.id ?? value.path) === key);
      const chunk = chunks.find(chunk => chunk.start <= index && chunk.end > index);
      if (chunk && !chunk.mounted) renderChunk(chunk);
    },
    show(nextValues, nextBuild, nextMode = 'grid') {
      values = nextValues;
      build = nextBuild;
      const nextIdentity = JSON.stringify([nextMode, values.map((value) => value.id ?? value.path)]);
      const changed = identity !== nextIdentity || mode !== nextMode;
      mode = nextMode;
      root.className = `media-virtual-grid ${mode === 'list' ? 'media-list' : 'media-grid'}`;
      root.setAttribute('role', 'list');
      if (!active) {
        active = true;
        if (typeof IntersectionObserver !== 'undefined') observer = new IntersectionObserver((entries) => {
          if (!active || !root.isConnected) return;
          for (const entry of entries) {
            const chunk = chunks.find((value) => value.element === entry.target);
            if (!chunk) continue;
            chunk.visible = entry.isIntersecting;
            if (chunk.visible && !chunk.mounted) renderChunk(chunk);
            else if (!chunk.visible) unmountChunk(chunk);
          }
          views.prune();
        }, { rootMargin: '900px 0px' });
        if (typeof ResizeObserver !== 'undefined') {
          resizeObserver = new ResizeObserver(schedule);
          resizeObserver.observe(root);
        } else window.addEventListener('resize', schedule);
      }
      if (changed) {
        identity = nextIdentity;
        columns = mode === 'list' ? 1 : 4;
        rebuild();
      } else {
        for (const chunk of chunks) {
          if (chunk.mounted) renderChunk(chunk);
          observer?.observe(chunk.element);
        }
      }
      schedule();
      return root;
    },
    hide() {
      active = false;
      observer?.disconnect(); observer = null;
      resizeObserver?.disconnect(); resizeObserver = null;
      window.removeEventListener('resize', schedule);
      if (frame) cancelAnimationFrame(frame);
      frame = 0;
    },
  };
}
