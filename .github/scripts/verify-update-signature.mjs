import { createHash,createPublicKey,verify } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { pathToFileURL } from 'node:url';
function decode(value){
  const text=String(value).trim();if(!text||!/^([A-Za-z0-9+/]{4})*([A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/.test(text))throw new Error('签名编码无效');
  return Buffer.from(text,'base64');
}
// 使用 Node 的 Ed25519 与 BLAKE2b 实现，格式遵循 Minisign 双签名结构。
export function verifyUpdateSignature(bytes,encodedSignature,encodedPublic,version){
  const keyLines=decode(encodedPublic).toString('utf8').trim().split(/\r?\n/);
  const lines=decode(encodedSignature).toString('utf8').trim().split(/\r?\n/);
  if(keyLines.length!==2||lines.length!==4||!lines[2].startsWith('trusted comment: '))throw new Error('签名结构无效');
  const rawKey=decode(keyLines[1]),rawSignature=decode(lines[1]),globalSignature=decode(lines[3]);
  if(rawKey.length!==42||rawSignature.length!==74||globalSignature.length!==64||rawKey.subarray(0,2).toString()!=='Ed'||!rawKey.subarray(2,10).equals(rawSignature.subarray(2,10)))throw new Error('签名公钥不匹配');
  const algorithm=rawSignature.subarray(0,2).toString();
  if(algorithm!=='ED')throw new Error('只接受预哈希 Minisign 更新签名');
  const key=createPublicKey({key:Buffer.concat([Buffer.from('302a300506032b6570032100','hex'),rawKey.subarray(10)]),format:'der',type:'spki'});
  const signature=rawSignature.subarray(10),trusted=lines[2].slice(17);
  if(!verify(null,createHash('blake2b512').update(bytes).digest(),key,signature)||!verify(null,Buffer.concat([signature,Buffer.from(trusted)]),key,globalSignature))throw new Error('更新包或可信版本注释的签名验证失败');
  if(trusted.split('\t').find(field=>field.startsWith('version:'))!==`version:${version}`)throw new Error('更新签名版本不一致');
}
if(process.argv[1]&&import.meta.url===pathToFileURL(process.argv[1]).href){
  try{
    const [configPath,installerPath,signaturePath]=process.argv.slice(2);
    const config=JSON.parse(readFileSync(configPath,'utf8'));
    verifyUpdateSignature(readFileSync(installerPath),readFileSync(signaturePath,'utf8'),config.plugins.updater.pubkey,config.version);
  }catch(error){process.stderr.write(`更新包验签失败：${error.message}\n`);process.exitCode=1;}
}
