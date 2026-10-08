export const defaultFontFamilies=['Segoe UI','Microsoft YaHei','PingFang SC','system-ui','sans-serif'];
const storageKey='launcher-font-settings';
const generics=new Set(['serif','sans-serif','monospace','system-ui','cursive','fantasy','ui-serif','ui-sans-serif','ui-monospace','ui-rounded','emoji','math','fangsong']);

function normalize(families) {
  const seen=new Set();
  return families.filter(name=>typeof name==='string').map(name=>name.trim()).filter(name=>{
    if(!name||seen.has(name.toLowerCase()))return false;
    seen.add(name.toLowerCase());return true;
  });
}
export function serializeFontFamilies(families) {
  return families.map(name=>generics.has(name.toLowerCase())?name.toLowerCase():JSON.stringify(name)).join(', ');
}
export function getFontFamilies() {
  try {
    const saved=JSON.parse(localStorage.getItem(storageKey)||'null');
    if(Array.isArray(saved?.families)){const fonts=normalize(saved.families);if(fonts.length)return fonts;}
  } catch { /* 无法读取偏好时使用应用默认字体。 */ }
  return [...defaultFontFamilies];
}
export function setFontFamilies(families) {
  const fonts=normalize(families);
  if(!fonts.length)throw new Error('请至少保留一个字体或通用字体族。');
  const css=serializeFontFamilies(fonts);
  document.documentElement.style.setProperty('--app-font-family',css);
  try { localStorage.setItem(storageKey,JSON.stringify({families:fonts,css})); }
  catch { /* 存储不可用时，当前窗口仍使用所选字体。 */ }
  return fonts;
}
export function parseFontFamilies(value) {
  const fonts=[];let token='',quote='',escaped=false;
  const source=value.trim().replace(/^font-family\s*:\s*/i,'').replace(/;\s*$/,'');
  for(const char of source){
    if(escaped){token+=char;escaped=false;}
    else if(char==='\\')escaped=true;
    else if(quote){if(char===quote)quote='';else token+=char;}
    else if(char==='"'||char==="'")quote=char;
    else if(char===','){fonts.push(token);token='';}
    else token+=char;
  }
  if(quote||escaped)throw new Error('字体名称的引号或转义未结束。');
  fonts.push(token);return normalize(fonts);
}
