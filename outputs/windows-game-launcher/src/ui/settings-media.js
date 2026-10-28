import { isTauri } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { el, button } from '../lib/dom.js';
import { settingsSection } from './settings-components.js';
import { createMediaSettings } from './media/settings.js';
import { addSourceDialog } from './media/manage.js';
import { ignoredPathsDialog } from './media/ignored.js';

export function settingsMedia(actions) {
  const element = settingsSection('影视库', '管理媒体目录、资料来源、播放器与豆瓣观影记录。');
  const content = el('div');
  element.append(content);
  let page, pending, timer, refreshTimer, active = false, disposed = false, loading = false, again = false;
  const listeners = [];
  const controller = {
    state: { data: { settings: {}, sources: [], jobs: [] } },
    chooseFile: actions.chooseFile,
    toast: actions.toast,
    async run(name, input = {}, notify = true) {
      try { return await actions.command(name, { input }); }
      catch (error) { if (notify) actions.toast(String(error), true); throw error; }
    },
    refresh,
    addSource: () => addSourceDialog(controller),
    browse: (sourceId, path = '') => actions.media().browse(sourceId, path),
    ignoredPaths: (sourceId) => ignoredPathsDialog(controller, sourceId),
    async refreshLocalFiles() {},
    async scan(sourceId) {
      await controller.run('media_scan', { sourceId });
      await refresh();
      actions.toast('媒体扫描已启动');
    },
  };
  async function refresh() {
    if (disposed) return;
    again = true;
    if (pending) return pending;
    pending = (async () => {
      do {
        again = false;
        const data = await controller.run('media_settings_snapshot', {}, false);
        if (disposed) return;
        controller.state.data = {
          settings: data?.settings || {},
          sources: Array.isArray(data?.sources) ? data.sources : [],
          jobs: Array.isArray(data?.jobs) ? data.jobs : [],
        };
        page?.update(controller.state.data);
      } while (again);
    })().finally(() => { pending = null; });
    return pending;
  }
  function scheduleRefresh() {
    if (!active || disposed || document.hidden || document.querySelector('dialog[open]') || refreshTimer) return;
    refreshTimer = setTimeout(() => {
      refreshTimer = null;
      if (active && !disposed) refresh().catch(() => {});
    }, 2000);
  }
  if (isTauri()) {
    for (const event of ['media-changed', 'media-job-changed'])
      listen(event, scheduleRefresh).then((unlisten) => {
        if (disposed) unlisten();
        else listeners.push(unlisten);
      }).catch(() => {});
  }
  async function activate() {
    if (disposed) return;
    active = true;
    if (!timer) timer = setInterval(scheduleRefresh, 5000);
    if (loading) return;
    if (page) {
      page.update(controller.state.data);
      refresh().catch(() => {});
      return;
    }
    loading = true;
    content.replaceChildren(el('p', 'settings-loading', '正在读取影视设置…'));
    try {
      await refresh();
      if (disposed) return;
      page = createMediaSettings(controller);
      content.replaceChildren(page.element);
    } catch (error) {
      if (!disposed) content.replaceChildren(el('p', 'settings-description', '影视设置暂时无法读取：' + String(error)), button('重新读取', 'secondary', activate, 'refresh'));
    } finally {
      loading = false;
    }
  }
  function deactivate() {
    active = false;
    clearInterval(timer); timer = null;
    clearTimeout(refreshTimer); refreshTimer = null;
  }
  return { element, activate, deactivate, dispose() {
    disposed = true;
    deactivate();
    listeners.forEach((unlisten) => unlisten());
    page?.dispose();
  } };
}
