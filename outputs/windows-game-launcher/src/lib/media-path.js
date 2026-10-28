// 仅格式化界面文字；实际读写和导航始终使用后端返回的原路径。
export function displayMediaPath(path) {
  if (typeof path !== 'string') return '';
  if (/^\\\\\?\\UNC\\/i.test(path)) return '\\\\' + path.slice(8);
  if (/^\\\\\?\\[a-z]:[\\/]/i.test(path)) return path.slice(4);
  return path;
}

export function mediaPathMatches(root, path, directory = true) {
  const key = (value) =>
    displayMediaPath(value)
      .replaceAll('\\', '/')
      .replace(/\/+/g, '/')
      .replace(/\/+$/, '')
      .toLowerCase();
  const base = key(root);
  const target = key(path);
  return !!base && (target === base || (directory && target.startsWith(base + '/')));
}
