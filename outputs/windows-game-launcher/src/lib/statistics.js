export const statisticsPeriods = [['7','近 7 天'],['30','近 30 天'],['90','近 90 天'],['365','近一年'],['all','全部记录']];
export const statisticsSources = [['all','全部平台'],['steam','Steam'],['epic','Epic'],['local','本地']];
export function duration(seconds) {
  if (seconds == null) return '未获取';
  if (seconds === 0) return '0 分钟';
  if (seconds < 60) return '不足 1 分钟';
  const minutes = Math.floor(seconds / 60);
  return minutes < 60 ? minutes+' 分钟' : Math.floor(minutes/60)+' 小时'+(minutes%60 ? ' '+minutes%60+' 分' : '');
}
export const percent = value => value == null ? '—' : (Math.round(value * 10) / 10)+'%';
export const localDate = value => value ? new Date(value).toLocaleString('zh-CN', {year:'numeric',month:'numeric',day:'numeric',hour:'2-digit',minute:'2-digit'}) : '进行中';
export function monthDays(month) {
  const [year, number] = month.split('-').map(Number);
  const first = new Date(year, number - 1, 1, 12);
  return { offset:(first.getDay() + 6) % 7, dates:Array.from({length:new Date(year,number,0).getDate()},(_,i)=>month+'-'+String(i+1).padStart(2,'0')) };
}
export function shiftMonth(month, delta) {
  const [year, number] = month.split('-').map(Number);
  const date = new Date(year, number - 1 + delta, 1, 12);
  return date.getFullYear()+'-'+String(date.getMonth()+1).padStart(2,'0');
}
export function rankedGames(games, basis) {
  const key = {period:'periodSeconds',local:'localSeconds',steam:'steamSeconds'}[basis];
  return games.filter(game=>game[key] != null && game[key] > 0)
    .sort((a,b)=>b[key]-a[key] || a.title.localeCompare(b.title,'zh-CN')).map(game=>({...game,rankSeconds:game[key]}));
}

// 统计有独立的请求与页面状态，不参与游戏封面快照。
export function createStatisticsController({command, render, visible, now = Date.now}) {
  const state = { query:{source:'all',period:'30'}, data:null, sessions:null, error:'', loading:false,
    offset:0, month:'', metric:'seconds', inspectDate:'', rankBasis:'period', rankExpanded:false, achievementExpanded:false, scroll:0, returnFocus:'' };
  let generation = 0, pending = null, again = false, dirty = false, lastFetched = -Infinity;
  function refresh(force = false) {
    if (!visible() && !force) return Promise.resolve();
    if (dirty && visible()) { dirty=false; render(); }
    if (pending) { if (force) again = true; return pending; }
    if (!force && now() - lastFetched < 5000) return Promise.resolve();
    const version = generation, query = {...state.query}, offset = state.offset;
    state.loading = true;
    if (!state.data) render();
    pending = Promise.all([
      command('get_statistics',{query}),
      command('list_statistics_sessions',{query,offset,limit:20}),
    ]).then(([data,sessions])=>{
      if (version !== generation) return;
      const changed = JSON.stringify({...data,generatedAt:''}) !== JSON.stringify({...state.data,generatedAt:''})
        || JSON.stringify(sessions) !== JSON.stringify(state.sessions) || !!state.error;
      state.data = data; state.sessions = sessions; state.error = '';
      if (!state.month || state.month < data.from.slice(0,7) || state.month > data.to.slice(0,7)) state.month = data.to.slice(0,7);
      if (changed) { if (visible()) render(); else dirty=true; }
    }).catch(error=>{
      if (version !== generation) return;
      state.error = String(error); if (visible()) render();
    }).finally(()=>{
      pending = null; state.loading = false; lastFetched = now();
      if (again) { again = false; refresh(true); }
    });
    return pending;
  }
  function filter(key,value) {
    if(state.query[key]===value)return;
    generation++; state.query[key] = value; state.offset = 0; state.month = '';
    dirty=false;
    state.inspectDate='';
    state.data = null; state.sessions = null; state.error = ''; render(); refresh(true);
  }
  return {state, refresh, filter,
    cycle:delta=>{const index=statisticsPeriods.findIndex(([value])=>value===state.query.period); filter('period',statisticsPeriods[(index+delta+statisticsPeriods.length)%statisticsPeriods.length][0]);},
    page:offset=>{generation++; const last=Math.max(0,Math.floor(((state.sessions?.total||0)-1)/20)*20); state.offset=Math.min(last,Math.max(0,offset)); return refresh(true);},
    change:(key,value)=>{state[key]=value; render();},
    day:async day=>{
      const query={...state.query,day};
      const [data,sessions] = await Promise.all([command('get_statistics',{query}),command('list_statistics_sessions',{query,offset:0,limit:20})]);
      return {data,sessions,query};
    },
    dayPage:(query,offset)=>command('list_statistics_sessions',{query,offset,limit:20}),
  };
}
