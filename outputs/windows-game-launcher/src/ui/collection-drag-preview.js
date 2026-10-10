import { el, icon } from '../lib/dom.js';

// 只使用源卡片已加载的封面，不复制按钮、占位动画或发起额外图片请求。
export function createCollectionDragPreview(card, payload, kind) {
  const source = card.querySelector('.game-art');
  const sourceRect = source?.getBoundingClientRect();
  const image = source?.querySelector('img');
  const node = el('div', 'collection-drag-ghost');
  node.setAttribute('role', 'status');
  node.setAttribute('aria-live', 'polite');
  node.setAttribute('aria-atomic', 'true');
  const stack = el('div', 'collection-drag-stack');
  stack.setAttribute('aria-hidden', 'true');
  const cover = el('div', 'collection-drag-cover');
  const fallback = el('span', 'collection-drag-fallback');
  fallback.append(icon('game'));
  cover.append(fallback);
  if (image?.complete && image.naturalWidth && (image.currentSrc || image.src)) {
    const copy = el('img');
    copy.alt = ''; copy.draggable = false;
    copy.src = image.currentSrc || image.src;
    copy.onerror = () => copy.remove();
    cover.append(copy);
  }
  stack.append(cover);
  if (payload.gameIds.length > 1) {
    stack.classList.add('collection-drag-stack-multiple');
    stack.append(el('span', 'collection-drag-count', String(payload.gameIds.length)));
  }
  const caption = el('div', 'collection-drag-caption');
  const hint = el('span', 'collection-drag-hint', '拖入左侧收藏夹 · Esc 取消');
  caption.append(el('strong', '', payload.title), hint);
  node.append(stack, caption);
  document.body.append(node);
  const width = node.offsetWidth, height = node.offsetHeight;
  const reducedMotion = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
  let entered = false, animation;

  return {
    node, hint,
    position(x, y) {
      const left = Math.max(8, Math.min(x + (kind === 'touch' ? 24 : 18), innerWidth - width - 8));
      const preferredTop = kind === 'touch' ? y - height - 24 : y + 18;
      const top = Math.max(8, Math.min(preferredTop, innerHeight - height - 8));
      // 指针跟随与入场动画分层，移动时不插值，避免封面追赶光标。
      node.style.transform = `translate(${left}px,${top}px)`;
      if (entered) return;
      entered = true;
      if (reducedMotion) return;
      const rect = cover.getBoundingClientRect();
      const from = sourceRect?.width && sourceRect.height
        ? `translate(${sourceRect.left - rect.left}px,${sourceRect.top - rect.top}px) scale(${sourceRect.width / rect.width},${sourceRect.height / rect.height})`
        : 'translate(0,6px) scale(.9)';
      animation = cover.animate([
        { transform: from, opacity: .85 },
        { transform: 'translate(0,0) scale(1)', opacity: 1 },
      ], { duration: 240, easing: 'cubic-bezier(.22,1,.36,1)' });
    },
    dispose() {
      animation?.cancel();
      node.remove();
    },
  };
}
