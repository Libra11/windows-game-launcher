-- 容量在正式对象与 GC 预留之间转移；只有物理删除成功才释放。
ALTER TABLE save_object_gc ADD COLUMN user_id uuid REFERENCES users;
ALTER TABLE save_object_gc ADD COLUMN bytes bigint;

-- 已发布对象本身已计费，不再保留重复的 GC 预留。
DELETE FROM save_object_gc g USING save_objects o WHERE g.object_key=o.object_key;
UPDATE save_object_gc g
 SET user_id=split_part(g.object_key,'/',2)::uuid,
     bytes=COALESCE((SELECT u.bytes FROM save_uploads u
                     WHERE u.staging_key=g.object_key OR u.object_key=g.object_key LIMIT 1),
                    2147483648);
-- 旧孤立尝试无法还原大小时按单对象上限保守预留，清理后释放。
ALTER TABLE save_object_gc ALTER COLUMN user_id SET NOT NULL;
ALTER TABLE save_object_gc ALTER COLUMN bytes SET NOT NULL;
ALTER TABLE save_object_gc ADD CONSTRAINT save_gc_bytes CHECK(bytes>0 AND bytes<=2147483648);
CREATE INDEX save_gc_user_quota ON save_object_gc(user_id);
