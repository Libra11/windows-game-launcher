const ease = 'cubic-bezier(0.22, 1, 0.36, 1)';
export const reducedMotion = () => matchMedia('(prefers-reduced-motion: reduce)').matches;

export function reveal(node, { delay = 0, distance = 10, duration = 280 } = {}) {
  if (!node || reducedMotion()) return;
  return node.animate([
    { opacity: 0, transform: `translateY(${distance}px)` },
    { opacity: 1, transform: 'translateY(0)' },
  ], { duration, delay, easing: ease, fill: 'backwards' });
}

// 只在用户切换视图时入场，后台同步和相同状态重绘不重播。
export function createViewMotion() {
  let previous;
  return (root, state) => {
    const current = {
      mode: state.bigScreen,
      page: state.selectedId || state.page || 'library',
      collection: JSON.stringify(state.bigScreen ? [state.bigCategory,state.bigCollection] : [state.filter, state.sort, state.view]),
      achievements: state.achievementFilter,
    };
    const pageChanged = !previous || current.mode !== previous.mode || current.page !== previous.page;
    const collectionChanged = !previous || current.collection !== previous.collection;
    const returningToLibrary=current.page==='library'&&previous&&current.mode===previous.mode&&previous.page!=='library';
    if (pageChanged&&!returningToLibrary) {
      // 长列表不作为整张动画图层，只让推荐区与首屏卡片入场。
      const target=current.page==='library'?root.querySelector('.library-feature, .big-screen-hero'):root;
      reveal(target, { distance: current.mode ? 0 : 10 });
    }
    const list = current.page !== 'library' ? root.querySelector('.achievement-list')
      : root.querySelector('.game-grid, .game-list, .big-screen-grid');
    if (!returningToLibrary&&(pageChanged || collectionChanged || current.achievements !== previous?.achievements)) {
      if (!pageChanged&&current.page!=='library') reveal(list, { distance: 6, duration: 220 });
      if (list) for(let index=0;index<Math.min(6,list.children.length);index++){
        reveal(list.children[index], { delay: index * 20, distance: 8 });
      }
    }
    previous = current;
  };
}

export async function dismiss(dialog) {
  if (reducedMotion()) return;
  const { opacity, transform } = getComputedStyle(dialog);
  const animation = dialog.animate([
    { opacity, transform },
    { opacity: 0, transform: 'translateY(8px) scale(0.98)' },
  ], { duration: 140, easing: 'cubic-bezier(0.4, 0, 1, 1)', fill: 'forwards' });
  await animation.finished.catch(() => {});
}
