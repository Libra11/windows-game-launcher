import { el, button } from '../../lib/dom.js';
import { customSelect } from '../custom-select.js';
import { submit } from '../form-fields.js';
import { settingsToggle } from '../settings-components.js';
import { field, input, jobList } from './components.js';
import { confirmAction } from './forms.js';
import { sourceSettings } from './source-settings.js';
import '../settings-media.css';

function service(title, description) {
  const element = el('section', 'settings-service');
  element.append(el('h3', '', title), el('p', 'settings-description', description));
  return element;
}

function filePathRow(controller, control, title) {
  const row = el('div', 'input-row');
  row.append(
    control,
    button(
      '选择文件',
      'secondary',
      async () => {
        const chosen = await controller.chooseFile({
          directory: false,
          multiple: false,
          title,
          filters: [{ name: 'Windows 程序', extensions: ['exe'] }],
        });
        if (typeof chosen === 'string') control.value = chosen;
      },
      'folder'
    )
  );
  return row;
}

export function createMediaSettings(controller) {
  const element = el('div', 'settings-media');
  const settings = controller.state.data.settings || {};
  const sources = sourceSettings(controller);
  element.append(sources.element);

  const metadata = service(
    'TMDB 资料连接',
    '用于识别影视资料与获取海报。API Key 或 Read Access Token 填写一项即可。'
  );
  const secretsForm = el('form', 'form');
  const apiKey = input('', 'password', '留空保留已保存的凭据');
  apiKey.autocomplete = 'new-password';
  apiKey.maxLength = 512;
  const accessToken = input('', 'password', '可只填写其中一项');
  accessToken.autocomplete = 'new-password';
  accessToken.maxLength = 8192;
  const secretStatus = el('p', 'settings-description');
  secretStatus.setAttribute('role', 'status');
  const credentials = el('div', 'settings-credentials');
  credentials.append(
    field('API Key', apiKey),
    field('API Read Access Token', accessToken, '填写原始令牌，不包含 Bearer 前缀。')
  );
  secretsForm.append(credentials);
  submit(secretsForm, '保存 TMDB 连接', async () => {
    const next = {};
    if (apiKey.value.trim()) next.tmdbApiKey = apiKey.value.trim();
    if (accessToken.value.trim()) next.tmdbReadAccessToken = accessToken.value.trim();
    if (!Object.keys(next).length) {
      controller.toast('填写 API Key 或 Read Access Token 后保存');
      return;
    }
    await controller.run('media_update_settings', { settings: next });
    apiKey.value = '';
    accessToken.value = '';
    await controller.refresh();
    controller.toast('TMDB 凭据已保存');
  });
  metadata.append(
    secretStatus,
    secretsForm,
    el('p', 'form-hint', '连接凭据保存在本机。个人评分和已看记录独立保存。')
  );
  element.append(metadata);

  const playback = service('播放器与扫描', '配置播放器、媒体探测程序与自动扫描间隔。');
  const playbackForm = el('form', 'form');
  const playerKind = customSelect(
    [
      ['potplayer', 'PotPlayer'],
      ['mpv', 'mpv'],
    ],
    settings.playerKind || 'potplayer',
    '播放器类型'
  );
  const playerPath = input(settings.playerPath || '', 'text', '留空尝试自动查找');
  const pollInterval = input(settings.potplayerPollIntervalMs ?? 500, 'number');
  pollInterval.min = '50';
  pollInterval.max = '2000';
  pollInterval.step = '1';
  pollInterval.required = true;
  const ffprobePath = input(settings.ffprobePath || '', 'text', 'ffprobe.exe 路径');
  const ffmpegPath = input(settings.ffmpegPath || '', 'text', '留空尝试自动查找 ffmpeg.exe');
  const scanInterval = input(settings.autoScanIntervalMinutes ?? 30, 'number');
  scanInterval.min = '0';
  scanInterval.max = '10080';
  scanInterval.step = '1';
  playbackForm.append(
    field('播放器', playerKind, '自动保存有效播放时长与续播位置。'),
    field('播放器程序', filePathRow(controller, playerPath, '选择播放器程序')),
    field(
      'PotPlayer 进度读取间隔（毫秒）',
      pollInterval,
      '默认 500，可设 50–2000。越小读取越频繁；保存后下次播放生效。'
    ),
    field(
      '媒体探测程序',
      filePathRow(controller, ffprobePath, '选择 ffprobe.exe'),
      '可选。留空依次查找截图程序同目录、PATH 和常见安装目录；未找到时技术参数显示未知。'
    ),
    field(
      '视频截图程序',
      filePathRow(controller, ffmpegPath, '选择 ffmpeg.exe'),
      '为未识别且没有图片的视频生成备用封面与背景。留空依次查找探测程序同目录、PATH 和常见安装目录；未找到时保留默认封面。'
    ),
    field('自动扫描间隔（分钟）', scanInterval, 'Windows 会监听目录变动并快速扫描，此间隔用于定时补扫；0 同时关闭监听与定时扫描。游迹运行或驻留托盘时执行。')
  );
  submit(playbackForm, '保存播放与扫描设置', async () => {
    if (!playbackForm.reportValidity()) return;
    await controller.run('media_update_settings', {
      settings: {
        playerKind: playerKind.value,
        playerPath: playerPath.value.trim(),
        potplayerPollIntervalMs: Number(pollInterval.value),
        ffprobePath: ffprobePath.value.trim(),
        ffmpegPath: ffmpegPath.value.trim(),
        autoScanIntervalMinutes: Number(scanInterval.value),
      },
    });
    await controller.refresh();
    controller.toast('设置已保存');
  });
  playback.append(playbackForm);
  element.append(playback);

  const douban = service(
    '豆瓣观影记录',
    '从豆瓣看过列表导入新增条目与观影记录，再自动获取 TMDB 资料；已有记录不更新。'
  );
  const connection = el('div', 'settings-description settings-media-connection');
  connection.setAttribute('role', 'status');
  const tools = el('div', 'settings-tools');
  tools.append(
    button(
      '登录／重新登录豆瓣',
      'secondary',
      async () => {
        await controller.run('media_douban_begin_login');
        await updateConnection();
      },
      'network'
    ),
    button(
      '导入新增记录',
      'secondary',
      async () => {
        await controller.run('media_douban_sync');
        await controller.refresh();
        await updateConnection();
      },
      'refresh'
    ),
    button(
      '断开连接',
      'settings-link',
      () =>
        confirmAction(
          '断开豆瓣连接',
          '停止后台更新并清除豆瓣登录会话，已导入的影视及个人记录会保留。',
          async () => {
            await controller.run('media_douban_disconnect');
            await updateConnection();
          },
          '断开连接'
        ),
      'close'
    )
  );
  let autoUpdate = !!settings.doubanAutoUpdate;
  const auto = settingsToggle(
    '自动导入豆瓣新增记录',
    '按下方间隔读取到上次成功位置，已有记录跳过；点击「保存更新设置」后生效。',
    autoUpdate,
    (value) => {
      autoUpdate = value;
    }
  );
  const interval = input(settings.doubanUpdateIntervalMinutes ?? 360, 'number');
  interval.min = '15';
  interval.max = '10080';
  interval.step = '1';
  const doubanForm = el('form', 'form');
  doubanForm.append(
    auto,
    field('自动更新间隔（分钟）', interval, '仅在游迹运行期间更新；需要验证时暂停并提示。')
  );
  submit(doubanForm, '保存更新设置', async () => {
    if (!doubanForm.reportValidity()) return;
    await controller.run('media_update_settings', {
      settings: {
        doubanAutoUpdate: autoUpdate,
        doubanUpdateIntervalMinutes: Number(interval.value),
      },
    });
    await controller.refresh();
    controller.toast('豆瓣更新设置已保存');
  });
  douban.append(connection, tools, doubanForm);
  element.append(douban);

  const jobs = service('后台任务', '查看扫描、识别和导入的执行状态。');
  const jobsContent = el('div');
  const jobTools = el('div', 'settings-tools');
  jobTools.append(button('刷新任务状态', 'secondary', () => controller.refresh(), 'refresh'));
  jobs.append(jobsContent, jobTools);
  element.append(jobs);

  let disposed = false,
    connectionRunning = false,
    jobsSignature = '',
    connectionSignature = '';
  async function updateConnection() {
    if (connectionRunning || disposed) return;
    connectionRunning = true;
    try {
      const value = await controller.run('media_douban_status', {}, false);
      if (disposed) return;
      const signature = JSON.stringify(value);
      if (signature === connectionSignature) return;
      connectionSignature = signature;
      const phase = value.phase || value.state || '';
      const connected = value.connected || value.loggedIn;
      const message =
        value.message ||
        (phase === 'running'
          ? '正在后台更新'
          : phase === 'needs_login' || phase === 'login_required'
            ? '需要重新登录'
            : connected
              ? '已连接豆瓣'
              : '尚未连接豆瓣');
      connection.replaceChildren(el('p', '', message));
      const last = value.lastSuccessAt || value.lastSyncedAt || value.lastSync;
      if (last)
        connection.append(
          el('p', 'form-hint', `上次更新：${new Date(last).toLocaleString('zh-CN')}`)
        );
      if (value.error) connection.append(el('p', 'form-error', value.error));
    } catch {
      if (!disposed && connectionSignature !== 'error') {
        connectionSignature = 'error';
        connection.replaceChildren(el('p', '', '暂时无法读取连接状态，可稍后重试。'));
      }
    } finally {
      connectionRunning = false;
    }
  }
  function update(data) {
    sources.update(data.sources);
    const current = data.settings || {};
    secretStatus.textContent = `API Key：${current.hasApiKey ? '已保存' : '未配置'} · Read Access Token：${current.hasReadAccessToken ? '已保存' : '未配置'}`;
    const signature = JSON.stringify(data.jobs);
    if (signature !== jobsSignature) {
      jobsSignature = signature;
      jobsContent.replaceChildren(jobList(data.jobs));
    }
    updateConnection();
  }
  update(controller.state.data);
  return {
    element,
    update,
    dispose() {
      disposed = true;
      apiKey.value = '';
      accessToken.value = '';
    },
  };
}
