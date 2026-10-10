import test from 'node:test';
import assert from 'node:assert/strict';
import { backupFileName, needsRelocation } from '../src/lib/backup.js';

test('备份文件名使用本机日期且无需用户输入扩展名',()=>{
  assert.equal(backupFileName(new Date(2026,9,10,8,9,3)),'游迹备份_20261010_080903.youji-backup');
});

test('路径预览区分本地缺失、记录缺失和平台客户端启动',()=>{
  assert.equal(needsRelocation({source:'steam',status:'not_configured',recordStatus:'not_configured'}),false);
  assert.equal(needsRelocation({source:'local',status:'not_configured',recordStatus:'not_configured'}),true);
  assert.equal(needsRelocation({source:'local',status:'exists',recordStatus:'inaccessible'}),true);
  assert.equal(needsRelocation({source:'local',status:'exists',recordStatus:'exists'}),false);
});
