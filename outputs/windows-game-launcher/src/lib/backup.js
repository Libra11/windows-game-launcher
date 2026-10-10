export function backupFileName(now=new Date()) {
  const part=value=>String(value).padStart(2,'0');
  return `游迹备份_${now.getFullYear()}${part(now.getMonth()+1)}${part(now.getDate())}_${part(now.getHours())}${part(now.getMinutes())}${part(now.getSeconds())}.youji-backup`;
}
export const backupPhases={snapshot:'正在保存计时检查点并读取数据',covers:'正在复制封面',data:'正在写入记录与外观',verify:'正在校验备份',safety:'正在创建恢复前安全备份',commit:'正在完成备份文件',restart:'即将重启并恢复',complete:'操作已完成'};
export const backupPathLabels={exists:'路径可用',missing:'文件缺失',inaccessible:'无法访问',not_configured:'未配置程序'};
export function needsRelocation(path) {
  return ['missing','inaccessible'].includes(path.status)||['missing','inaccessible'].includes(path.recordStatus)
    ||(path.source==='local'&&path.status==='not_configured');
}
