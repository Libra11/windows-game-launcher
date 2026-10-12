const editions={
  '1123050':'官方中文|V1.3.2+全DLC|支持手柄',
  '1245620':'官方中文|V1.16.1+全DLC|支持手柄',
  '367520':'官方中文|V1.5.78+全DLC|支持手柄',
  '1086940':'官方中文|Build.20261012|全DLC',
  '1145360':'官方中文|V1.38|支持手柄',
  '2379780':'官方中文|V1.0.1',
  '413150':'官方中文|V1.6.15',
  '588650':'官方中文|V35.0+全DLC|支持手柄',
};

export function createOnlineSearchPreview(games) {
  return (name,args)=>{
    if(name==='search_gamer520') {
      const query=args.query.trim();const term=query.toLocaleLowerCase();
      const items=games.filter(game=>`${game.title} 游戏`.toLocaleLowerCase().includes(term)).map((game,index)=>{
        const metadata=JSON.parse(game.metadataJson);
        return {id:game.appid,title:`${game.title}|${editions[game.appid]}`,url:`https://www.gamer520.com/${game.appid==='367520'?'21602':game.appid}.html`,
          image:metadata.banner || metadata.cover || '',categories:['PC PLAY'],isPc:true,publishedAt:new Date(Date.UTC(2026,9,12-index)).toISOString()};
      });
      const searchUrl=new URL('https://www.gamer520.com/');searchUrl.searchParams.set('s',query);
      return {query,page:args.page,items,hasNext:false,searchUrl:searchUrl.href};
    }
    const id=new URL(args.url).pathname.replace(/\D/g,'');
    const game=games.find(game=>game.appid===(id==='21602'?'367520':id)) || games[2];
    const metadata=JSON.parse(game.metadataJson);
    return {id,title:`${game.title}|${editions[game.appid]}`,url:args.url,cover:metadata.banner || metadata.cover || '',categories:['PC PLAY'],
      blocks:[{kind:'heading',text:'游戏介绍',image:''},{kind:'text',text:metadata.description || '这里展示原站游戏简介、配置要求和截图。',image:''},
        {kind:'heading',text:'版本与配置',image:''},{kind:'text',text:'桌面版会按原站资料展示当前版本、配置要求与补充说明。此处为界面示例，不代表实际版本或配置。',image:''},
        {kind:'heading',text:'游戏配图',image:''},{kind:'image',text:'',image:metadata.libraryHeroes?.[0] || metadata.cover || ''}],
      downloads:{sourceUrl:'https://520.gamers520.com/13352.html',filePassword:'8915',archivePassword:'example',error:'',
        notes:['当前为界面示例，二维码和链接不代表可用下载资源。'],links:[
          {label:'百度网盘',url:'https://pan.baidu.com/s/preview?pwd=abcd',code:'abcd'},
          {label:'夸克网盘',url:'https://pan.quark.cn/s/preview',code:''},
          {label:'迅雷网盘',url:'https://pan.xunlei.com/s/preview',code:''},
          {label:'UC 网盘',url:'https://drive.uc.cn/s/preview',code:''},
        ]}};
  };
}
