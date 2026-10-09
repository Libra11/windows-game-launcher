export function createProgramDropHandler({ prepare, open, notify, isBlocked }) {
  let busy = false;
  return async paths => {
    if (busy || isBlocked()) { notify('请先完成或关闭当前窗口，再拖入游戏。', true); return; }
    if (paths.length !== 1) { notify('请一次拖入一个游戏程序或快捷方式。', true); return; }
    if (!/\.(exe|lnk)$/i.test(paths[0])) { notify('目前支持 .exe 游戏程序和 .lnk 快捷方式。', true); return; }
    busy = true;
    try {
      const candidate = await prepare(paths[0]);
      if (candidate.existingGameId) { notify('此游戏已在游戏库中，无需重复导入。', true); return; }
      if (!isBlocked()) await open(candidate);
      else notify('请先关闭当前窗口，再重新拖入游戏。', true);
    } catch (error) { notify(String(error), true); }
    finally { busy = false; }
  };
}
