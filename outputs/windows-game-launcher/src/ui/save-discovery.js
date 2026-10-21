import { el, button } from '../lib/dom.js';
import { command } from '../lib/bridge.js';

// Discovery never binds a profile. Selection only fills the existing confirmation form.
export function directoryDiscovery(game, onChoose) {
  const section = el('section', 'form save-discovery');
  const status = el('p', ''); status.setAttribute('role', 'status');
  const rows = el('div', 'form');
  let disposed = false, busy = false;
  const search = button('重新识别存档目录', 'secondary', scan);
  section.append(el('h3', '', '自动识别存档目录'), status, rows, search);
  async function scan() {
    if (busy || disposed) return;
    busy = true; search.disabled = true; rows.replaceChildren();
    status.textContent = '正在检查已知存档位置…';
    try {
      const result = await command('saves_discover', { gameId: game.id });
      if (disposed) return;
      const candidates = result.candidates || [];
      status.textContent = candidates.length
        ? `找到 ${candidates.length} 个候选目录，请核对后选择。选择后仍需确认绑定。`
        : result.matched
          ? '已匹配游戏规则，但未找到可绑定的目录。可先运行游戏并保存一次，或手动选择；已绑定或不安全的目录不会列出。'
          : '内置规则暂未匹配此游戏，请手动选择存档目录。';
      if (result.limited) status.textContent += ' 检查达到上限，结果可能不完整。';
      for (const candidate of candidates) {
        const row = el('div', 'save-candidate');
        const modified = candidate.modified ? new Date(candidate.modified * 1000).toLocaleString() : '未知';
        row.append(el('strong', '', candidate.path), el('p', '', candidate.evidence),
          el('p', '', `${candidate.files} 个文件 · ${(candidate.bytes / 1024 / 1024).toFixed(2)} MiB · 最近修改：${modified}${candidate.incomplete ? '（统计不完整）' : ''}`));
        if (!candidate.files) row.append(el('p', '', '目录为空或无法完整读取，请确认游戏已生成存档。'));
        row.append(button('使用此目录', 'secondary', () => {
          if (disposed) return;
          onChoose(candidate.path);
          status.textContent = '已选择候选目录，请在下方核对路径和自动备份选项，再确认绑定。';
        }));
        rows.append(row);
      }
    } catch (e) {
      if (!disposed) status.textContent = `自动识别未完成：${String(e)}。仍可手动选择目录。`;
    } finally {
      busy = false; search.disabled = false;
    }
  }
  scan();
  return { element: section, dispose: () => { disposed = true; } };
}
