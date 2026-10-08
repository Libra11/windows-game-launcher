// 仅用于开发浏览器的明确标注示例，不会在正式桌面构建中读取。
export function createStatisticsPreview(games) {
  const now=new Date(), to=dateKey(now);
  const start=new Date(now.getFullYear(),now.getMonth(),now.getDate()-44,0);
  const rows=[],events=[];
  for(let i=0;i<45;i++){
    const day=new Date(start.getFullYear(),start.getMonth(),start.getDate()+i,20);
    if(i%7===1)continue;
    const game=games[i%games.length];if(!game)continue;
    const seconds=(45+i%6*25)*60;
    rows.push({id:'sample-'+i,gameId:game.id,gameTitle:game.title,source:game.source,startedAt:day.toISOString(),endedAt:new Date(day.getTime()+seconds*1000).toISOString(),seconds,dailyRecorded:true,date:dateKey(day)});
    if(i%3===0)events.push({gameId:game.id,gameTitle:game.title,apiName:'sample-'+i,name:['初次觉醒','探索者','不屈','新的道路'][i%4],icon:'',source:game.source==='steam'?'steam':'local',date:dateKey(day),recordedAt:day.toISOString()});
  }
  function filtered(query) {
    const days=query.period==='all'?45:Number(query.period);
    const fromDate=new Date(now.getFullYear(),now.getMonth(),now.getDate()-days+1);
    const from=dateKey(fromDate), source=game=>query.source==='all'||game.source===query.source;
    return {from,fromDate,selected:games.filter(source),sessions:rows.filter(source).filter(row=>row.date>=from&&row.date<=to),unlocks:events.filter(event=>games.some(game=>game.id===event.gameId&&source(game))).filter(e=>e.date>=from&&e.date<=to)};
  }
  function snapshot(query) {
    const {from,fromDate,selected,sessions,unlocks}=filtered(query);
    const daily=[];
    for(let day=new Date(fromDate);dateKey(day)<=to;day.setDate(day.getDate()+1)){
      const date=dateKey(day);daily.push({date,recorded:date>=dateKey(start),seconds:sessions.filter(s=>s.date===date).reduce((sum,s)=>sum+s.seconds,0),unlocks:unlocks.filter(e=>e.date===date).length});
    }
    const stats=selected.map(game=>{
      const index=games.findIndex(item=>item.id===game.id);
      const totalAchievements=8, unlocked=index<2?8:3+index%4;
      return {gameId:game.id,title:game.title,source:game.source,metadataJson:game.metadataJson,
        localSeconds:game.playedSeconds+rows.filter(s=>s.gameId===game.id).reduce((sum,s)=>sum+s.seconds,0),
        periodSeconds:sessions.filter(s=>s.gameId===game.id).reduce((sum,s)=>sum+s.seconds,0),
        steamSeconds:game.source==='steam'?game.playtime.seconds:null,steamState:'ready',steamCheckedAt:now.toISOString(),
        totalAchievements,unlocked,manualUnlocked:index===0?1:0,definitionState:'complete',completionRate:unlocked/totalAchievements*100};
    });
    const steam=stats.filter(g=>g.source==='steam');
    const seconds=sessions.reduce((sum,s)=>sum+s.seconds,0);
    return {startedAt:start.toISOString(),startDate:dateKey(start),from,to,generatedAt:now.toISOString(),
      summary:{gameCount:stats.length,steamSeconds:steam.length?steam.reduce((sum,g)=>sum+g.steamSeconds,0):null,steamKnown:steam.length,steamGames:steam.length,steamCached:false,steamCheckedAt:now.toISOString(),
        localSeconds:stats.reduce((sum,g)=>sum+g.localSeconds,0),unlocked:stats.reduce((sum,g)=>sum+g.unlocked,0),manualUnlocked:stats.reduce((sum,g)=>sum+g.manualUnlocked,0),
        completedGames:stats.filter(g=>g.completionRate===100).length,completionRate:stats.length?stats.reduce((sum,g)=>sum+g.unlocked,0)/(stats.length*8)*100:null},
      period:{seconds,activeDays:daily.filter(d=>d.seconds>0).length,sessions:sessions.length,averageSeconds:sessions.length?Math.floor(seconds/sessions.length):0,newUnlocks:unlocks.length},
      daily,platforms:['steam','epic','local'].map(source=>({source,games:stats.filter(g=>g.source===source).length,seconds:stats.filter(g=>g.source===source).reduce((sum,g)=>sum+g.periodSeconds,0)})),
      games:stats,recentUnlocks:[...unlocks].reverse().slice(0,20),dayUnlocks:unlocks.filter(e=>e.date===query.day)};
  }
  return (name,{query,offset=0,limit=20})=>{
    if(name==='get_statistics')return snapshot(query);
    const {sessions}=filtered(query);const items=[...sessions].reverse().filter(row=>!query.day||row.date===query.day);
    return {items:items.slice(offset,offset+limit),total:items.length};
  };
}
function dateKey(date){return date.getFullYear()+'-'+String(date.getMonth()+1).padStart(2,'0')+'-'+String(date.getDate()).padStart(2,'0');}
