export function detectionState(game) {
  if (game.source === 'epic') {
    const error = game.achievementPlatform?.definitionError;
    return { label:error ? 'Epic 成就同步失败' : game.schemaSource === 'Epic 官方成就（暂无成就）' ? '暂无 Epic 成就' : game.scanStatus === 'Epic 成就已同步' ? 'Epic 官方成就已同步' : '等待 Epic 官方同步', detail:error || game.scanStatus || '点击“更新资料”，通过已连接的 Epic 账号读取成就详情和解锁状态。', state:error ? 'error' : game.scanStatus === 'Epic 成就已同步' ? 'ready' : 'pending' };
  }
  const status = game.scanStatus || '';
  if (game.source !== 'local') return { label:'Steam 官方同步', detail:status || '等待同步官方成就', state:'steam' };
  if (game.achievementPlatform?.platform === 'xbox' || game.schemaSource?.startsWith('Xbox') || status.includes('Xbox 本地')) {
    return {label:status.includes('失败') ? 'Xbox 捕获失败' : status.startsWith('已读取') ? 'Xbox 本地接口监听中' : '等待 Xbox 游戏启动', detail:`${status}。${game.schemaSource === 'Xbox 公开成就资料（Exophase）' ? '成就名称与条件来自 Exophase 公开资料；解锁由本地真实事件记录。' : '点击更新资料获取公开成就列表，无需登录。'}不上传本地解锁。`, state:status.includes('失败') ? 'error' : status.startsWith('已读取') ? 'ready' : 'pending'};
  }
  if (!game.appid) return { label:'尚未匹配游戏', detail:'先搜索并关联正确的 Steam 游戏，再获取成就资料。', state:'pending' };
  if (/缺少成就定义/.test(status)) return { label:'需要补全成就定义', detail:status, state:'pending' };
  if (/格式无效/.test(status)) return { label:'记录格式不支持', detail:'已找到文件，但无法解析为解锁记录。请确认没有选到成就定义文件。', state:'error' };
  if (/无法读取|失败/.test(status)) return { label:'记录读取失败', detail:status, state:'error' };
  if (/^当前无法自动检测/.test(status)) return { label:'当前版本无法自动检测', detail:status, state:'unsupported' };
  if (/^已读取/.test(status)) return { label:'已找到解锁记录', detail:'正在自动监听，新解锁会更新成就并发送通知。', state:'ready' };
  if (/未找到/.test(status)) return { label:'尚未找到真实解锁记录', detail:'自动检测需要游戏或运行环境实际写出受支持的成就记录。仅保存游戏或关联 Steam 不保证会生成记录。', state:'pending' };
  return { label:'等待首次检测', detail:'启动器将自动检查常用记录目录，也可以指定已有的 JSON 或 INI 解锁记录。', state:'pending' };
}
