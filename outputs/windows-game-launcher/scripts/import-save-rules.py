"""Build the conservative Windows save-discovery subset. Requires PyYAML, offline input.
Usage: python scripts/import-save-rules.py manifest.yaml
Only supported, non-recursive save patterns are included; this is not a full Ludusavi implementation.
"""
import hashlib,json,re,sys
from pathlib import Path
import yaml
raw=Path(sys.argv[1]).read_bytes()
manifest=yaml.safe_load(raw)
prefixes={'<winAppData>/':'roamingAppData','<winLocalAppData>/':'localAppData','<winLocalAppDataLow>/':'localLow','<home>/AppData/LocalLow/':'localLow','<winDocuments>/':'documents','<home>/Documents/':'documents','<home>/Saved Games/':'savedGames','<base>/':'gameInstall','<root>/userdata/<storeUserId>/':'steamUserData'}
games=[]
for title,item in manifest.items():
 patterns=[]
 for path,info in (item.get('files') or {}).items():
  if 'save' not in info.get('tags',[]):continue
  conditions=[w for w in info.get('when',[{}]) if w.get('os','windows')=='windows' and w.get('store','') in ('','steam','epic','gog')]
  if not conditions:continue
  for prefix,base in prefixes.items():
   if not path.startswith(prefix):continue
   relative=path[len(prefix):].replace('<storeUserId>','{account}')
   if '<storeGameId>' in relative:
    if not item.get('steam',{}).get('id'):break
    relative=relative.replace('<storeGameId>',str(item['steam']['id']))
   if not relative or any(c in relative for c in '<>:[\\') or '**' in relative:break
   if any(p in ('','.','..') for p in relative.split('/')):break
   # Never expand a wildcard directly under a shared Windows base.
   if base!='gameInstall' and ('*' in relative.split('/')[0] or '?' in relative.split('/')[0]):break
   stores=[] if any('store' not in w for w in conditions) else sorted({w['store'] for w in conditions})
   patterns.append({'base':base,'relative':relative,'stores':stores});break
 if patterns:games.append({'title':title,'steam':item.get('steam',{}).get('id',0),'patterns':patterns})
output={'source':'https://github.com/mtkennerly/ludusavi-manifest','sha256':hashlib.sha256(raw).hexdigest(),'games':games}
path=Path(__file__).resolve().parents[1]/'src-tauri/data/save-rules.json'
path.write_text(json.dumps(output,ensure_ascii=False,separators=(',',':'))+'\n',encoding='utf-8')
print(f'{len(games)} games, {sum(len(g["patterns"]) for g in games)} patterns, {path.stat().st_size} bytes')
