import { el, button, icon } from '../lib/dom.js';
import { modal, closeModal } from './modal.js';
import { chooseFile } from '../lib/bridge.js';

const messages = {
  ready:['可以自动检测','已找到可读取的解锁记录。游玩中写入新记录后，会自动更新成就并通知。'],
  unsupported:['当前版本无法自动检测','已检测到 Xbox/GDK 游戏文件，但未找到受支持的真实成就事件记录。Steam 成就资料不能替代这份游戏的成就机制，也不会根据游玩进度推断解锁。'],
  needs_appid:['先匹配 Steam AppID','AppID 用于匹配成就定义。填写正确的 AppID 后，再更新成就资料并检查记录。'],
  needs_definitions:['需要补全成就定义','成就名称、图标和完整列表尚未就绪。请更新资料；使用 Steam 接口时需要在设置中填写 API Key。'],
  missing_record:['尚未找到真实解锁记录','请确认游戏或运行环境会实际写出成就记录。可以指定已有的受支持记录文件；游戏存档和成就定义不等于解锁记录。'],
  error:['记录暂时无法读取','请查看下方检查结果。解锁记录和成就定义是不同文件，不能把定义文件选作解锁记录。'],
};

export function detectionGuide(game, actions) {
  const content = el('div', 'detection-guide');
  content.append(el('p','form-hint','正在检查成就资料和常见记录路径…'));
  const dialog = modal('成就检测引导', game.title, content);
  let version = 0;
  function paint(report) {
    const [title, description] = messages[report.state] || messages.error;
    const summary = el('section',`guide-result ${report.state === 'ready' ? 'ready' : ''}`);
    summary.append(icon(report.state === 'ready' ? 'check' : 'info'),el('h3','',title),el('p','',description));
    const facts = el('div','guide-facts');
    for (const [label,value] of [['Steam AppID',game.appid || '未设置'],['成就定义', `${report.definitions} 项 · ${report.schemaSource || '尚未获取'}`],['已记录解锁',String(report.unlocked)],['检查结果',report.message || '等待检测'],['读取路径',report.sourceFile || '尚未读取'],['最近检查',report.lastScan ? new Date(report.lastScan).toLocaleString('zh-CN') : '尚未检查']]) {
      const field = el('div'); field.append(el('small','',label),el('span','',value)); facts.append(field);
    }
    const paths = el('section','guide-paths'); paths.append(el('h3','','已检查的路径'));
    report.candidates.forEach(item => {
      const row = el('div','guide-path'); row.append(icon(item.exists ? 'check' : 'folder'),el('span','',item.path),el('small','',item.exists ? '已找到' : '未找到')); paths.append(row);
    });
    if (!report.candidates.length) paths.append(el('p','form-hint','设置 AppID 后可自动查找常见目录，也可直接选择记录文件。'));
    const controls = el('div','guide-actions');
    controls.append(button('重新检测','primary',() => load(),'refresh'),button('选择记录文件','secondary',async () => {
      const path = await chooseFile({multiple:false,filters:[{name:'成就解锁记录（JSON / INI）',extensions:['json','ini']}]});
      if (path) await load('set_record_path',{gameId:game.id,path});
    },'folder'));
    if (game.customUnlockPath) controls.append(button('恢复自动查找','secondary',()=>load('set_record_path',{gameId:game.id,path:''}),'refresh'));
    controls.append(button('更新成就资料','secondary',async()=> { await actions.run('sync_game',{gameId:game.id}); await load(); },'steam'));
    controls.append(button('编辑 AppID','secondary',async()=> {await closeModal(dialog); actions.edit(game,true);},'settings'));
    content.replaceChildren(summary,facts,paths,controls,el('p','form-hint','启动器只读取已有记录。成就名称和图标不会自行触发解锁；没有支持的记录时，可以在详情页手动记录。'));
  }
  async function load(command = 'check_detection', args = {gameId:game.id}) {
    const current = ++version;
    try {
      const report = await actions.run(command,args);
      if (!dialog.isConnected || current !== version) return;
      await actions.refresh(true); game = actions.game(game.id) || game; paint(report);
    } catch (error) {
      if (!dialog.isConnected || current !== version) return;
      content.querySelector('.form-error')?.remove(); content.append(el('p','form-error',String(error)));
    }
  }
  load(); return dialog;
}
