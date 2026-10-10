const cached=new WeakMap();

export function gameMetadata(game) {
  if(!game||typeof game!=='object')return {};
  const value=game.metadataJson||'{}';
  const previous=cached.get(game);
  if(previous?.value===value)return previous.info;
  let info;
  try{info=JSON.parse(value);}catch{info={};}
  if(!info||typeof info!=='object'||Array.isArray(info))info={};
  cached.set(game,{value,info});
  return info;
}
