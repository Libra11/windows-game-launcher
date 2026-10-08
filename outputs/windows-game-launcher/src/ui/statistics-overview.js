import { el, icon } from '../lib/dom.js';
import { duration, percent } from '../lib/statistics.js';
import { platformChart } from './statistics-charts.js';

function summaryModule(label, glyph, className) {
  const node=el('section','stats-summary-module '+className);
  const heading=el('div','stats-summary-label');
  heading.append(icon(glyph),el('h2','',label));node.append(heading);
  return node;
}

function timeValue(seconds) {
  const node=el('div','stats-number');
  const value=seconds==null?'—':seconds<60?seconds===0?'0':'<1':seconds<3600?Math.floor(seconds/60):(Math.round(seconds/360)/10).toLocaleString('zh-CN');
  node.append(el('strong','',value),el('span','',seconds==null?'未获取':seconds<3600?'分钟':'小时'));
  return node;
}

export function statisticsHeader(data) {
  const header=el('header','stats-page-header');const copy=el('div');
  copy.append(el('div','stats-kicker','个人游戏档案'),el('h1','','游戏统计'),el('p','','那些投入的时光，都在这里留下了足迹。'));
  const date=el('div','stats-date-range');date.append(icon('clock'),el('span','',data?data.from.replaceAll('-','.')+' — '+data.to.replaceAll('-','.'):'正在读取你的游戏档案'));
  header.append(copy,date);return header;
}

export function statisticsOverview(summary, platforms) {
  const overview=el('div','stats-overview');
  const time=summaryModule('游戏时光','clock','stats-time-summary');
  const official=el('div','stats-time-main');
  official.append(el('span','stats-summary-caption','Steam 官方累计'),timeValue(summary.steamSeconds));
  const steamHint=summary.steamGames?'已获取 '+summary.steamKnown+' / '+summary.steamGames+' 款'+(summary.steamCached?' · 缓存':''):'当前分类没有 Steam 游戏';
  official.append(el('small','stats-summary-hint',steamHint));time.append(official);
  const local=el('div','stats-summary-footer');local.append(el('span','','本机累计'),el('strong','',duration(summary.localSeconds)));
  time.append(local,el('small','stats-summary-footnote','含历史记录 · 与 Steam 官方时长分别统计'));

  const achievements=summaryModule('成就足迹','trophy','stats-achievement-summary');
  const progress=el('div','stats-achievement-total');
  const count=el('div'),unlocked=el('div','stats-number');
  unlocked.append(el('strong','',summary.unlocked.toLocaleString('zh-CN')),el('span','','项'));
  count.append(el('span','stats-summary-caption','已解锁成就'),unlocked);
  const ring=el('div','stats-completion-ring');
  ring.style.setProperty('--completion',Math.min(100,Math.max(0,summary.completionRate||0))+'%');
  ring.setAttribute('role','img');ring.setAttribute('aria-label','整体成就完成度 '+percent(summary.completionRate));
  const center=el('div');center.append(el('strong','',percent(summary.completionRate)),el('span','','完成度'));ring.append(center);progress.append(count,ring);
  const achievementFooter=el('div','stats-achievement-footer');
  for(const [value,label] of [[summary.completedGames+' 款','全成就游戏'],[summary.manualUnlocked+' 项','手动记录']]){
    const item=el('div');item.append(el('strong','',value),el('span','',label));achievementFooter.append(item);
  }
  achievements.append(progress,achievementFooter,el('small','stats-summary-footnote','仅完整、非空的成就定义参与完成度'));

  const library=summaryModule('游戏收藏','library','stats-library-summary');
  const total=el('div','stats-number');total.append(el('strong','',summary.gameCount.toLocaleString('zh-CN')),el('span','','款游戏'));
  library.append(total,platformChart(platforms,summary.gameCount),el('small','stats-summary-footnote','按游戏导入来源统计'));
  overview.append(time,achievements,library);return overview;
}

export function periodOverview(period) {
  const strip=el('section','stats-period-strip');strip.setAttribute('aria-label','所选周期的本机活动');
  for(const [value,label,glyph] of [[duration(period.seconds),'本期本机游玩','play'],[period.activeDays+' 天','活跃天数'],[period.sessions+' 次','游玩会话'],[duration(period.averageSeconds),'平均单次'],[period.newUnlocks+' 项','新增解锁']]){
    const item=el('div',glyph?'stats-period-primary':'');
    const caption=el('span','stats-period-label',label);if(glyph)caption.prepend(icon(glyph));
    item.append(caption,el('strong','',value));strip.append(item);
  }
  return strip;
}
