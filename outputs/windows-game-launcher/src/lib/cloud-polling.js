// 登录前没有同步进度可轮询；不可见和正在编辑时也不打断用户。
export function canPollCloud(element, loggedIn, doc = document) {
  return loggedIn && element.isConnected && !element.hidden && !doc.hidden
    && !doc.activeElement?.matches('input, textarea, select, [contenteditable="true"]');
}
