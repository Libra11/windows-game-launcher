# 影视协作接口与实现约定

使用说明见 [README](../../../README.md#影视库)，领域术语见 [CONTEXT](../../../CONTEXT.md)。本文件记录需要跨模块保持的接口与不变量；SQL 类型、默认值和索引以 [store.rs](store.rs)、`people.rs` 与 `metadata/retry.rs` 的初始化为准。

## 状态、维护与数据范围

`crate::media::State` 的 `pub dir: PathBuf` 为当前应用数据目录，另维护内部操作读写锁与维护标记。`store::open(&dir)` 初始化并连接独立 `media.sqlite`，返回 `Result<rusqlite::Connection,String>`；启用 WAL、foreign_keys 和 busy_timeout。每次业务操作独立连接，不占用游戏数据库锁。

应用入口和后台写入必须取得 `media::operation(&app).await` 的 owned read guard，安排游戏备份恢复时取得 `media::exclusive(&app).await` 的 owned write guard。安排恢复先 `begin_maintenance` 阻止新操作，失败 `end_maintenance`，成功保持直到重启。同一调用链不得重复取得读锁：扫描任务释放自己的许可后再启动独立元数据任务；blocking 工作线程需持有许可到写入结束。

秘密使用 `credential(dir,name)` / `save_credential(dir,name,value)`，Windows Credential Manager 中按应用目录分隔；snapshot/settings 仅返回存在标记。`name` 为 `tmdbApiKey` 或 `tmdbReadAccessToken`。

影视采用当前固定 schema，不迁移开发版数据库。影视库暂不参与应用数据备份与恢复；游戏 v3 备份恢复不读取或覆盖影视库、观看记录及登录资料，不支持开发期 v4 影视备份。游戏备份范围见 [备份格式](../../../BACKUP-FORMAT.md)。评分沿用 1–5 星，进度和时长保留浮点秒。资源目录不能互相包含，避免同一文件在两个启用来源之间反复归属。

## 数据表与设置

- `media_items(id TEXT PRIMARY KEY, kind TEXT, parent_id TEXT NULL, title TEXT, year TEXT, overview TEXT, poster_url TEXT, backdrop_url TEXT, genres_json TEXT, recognized INTEGER, independent INTEGER, season_number INTEGER NULL, episode_number INTEGER NULL, created_at TEXT, updated_at TEXT)`。条目不存在隐藏状态，实体存在即收录；不保留旧隐藏字段、快照或筛选，也不做旧隐藏数据迁移。
- `external_ids(provider TEXT,provider_type TEXT,external_id TEXT,item_id TEXT, PRIMARY KEY(provider,provider_type,external_id))`。
- `personal(item_id TEXT PRIMARY KEY,watched INTEGER,favorite INTEGER,rating REAL NULL,comment TEXT,marked_at TEXT,record_source TEXT,manual_fields_json TEXT,last_played TEXT)`。
- `sources(id TEXT PRIMARY KEY,path TEXT UNIQUE,label TEXT,kind TEXT,smart_folders INTEGER,available INTEGER,last_scan TEXT,error TEXT,created_at TEXT)`。
- `resources(id TEXT PRIMARY KEY,source_id TEXT,path TEXT UNIQUE,size INTEGER,modified_at TEXT,available INTEGER,missing INTEGER,technical_json TEXT,position_seconds REAL,duration_seconds REAL,updated_at TEXT)`。
- `resource_links(resource_id TEXT,item_id TEXT,method TEXT,locked INTEGER, PRIMARY KEY(resource_id,item_id))`。
- `view_sessions(id TEXT PRIMARY KEY,item_id TEXT,resource_id TEXT,started_at TEXT,ended_at TEXT,seconds REAL,position_seconds REAL)`。
- `media_collections(id TEXT PRIMARY KEY,name TEXT,sort_order INTEGER)`；`collection_items(collection_id,item_id,PRIMARY KEY(collection_id,item_id))`。
- `media_tags(id TEXT PRIMARY KEY,name TEXT UNIQUE)`；`media_item_tags(item_id,tag_id,PRIMARY KEY(item_id,tag_id))`。
- `settings(key TEXT PRIMARY KEY,value TEXT)` JSON 值；`jobs(id TEXT PRIMARY KEY,kind TEXT,state TEXT,completed INTEGER,total INTEGER,message TEXT,error TEXT,created_at TEXT,updated_at TEXT)`。
- `douban_records(account_key TEXT,subject_id TEXT,item_id TEXT,stars REAL,marked_at TEXT,comment TEXT,tags_json TEXT,imported_at TEXT,PRIMARY KEY(account_key,subject_id))`。
- `douban_source_data(account_key TEXT,subject_id TEXT,raw_json TEXT,PRIMARY KEY(account_key,subject_id))` 保留完整来源资料，不携带登录状态。
- `media_playback_coverage(resource_id TEXT PRIMARY KEY,segments_json TEXT)` 保存实际播放覆盖区间；`media_match_errors(item_id TEXT PRIMARY KEY,error TEXT,checked_at TEXT)` 保存最近识别错误。快照 `recognitionStatus` 为 `recognized`、`pending`、`unmatched` 或 `failed`，分别表示已识别、尚无分类结果、最近一次无可靠匹配和最近一次临时错误；旧版未分类错误暂显示 `pending`，不猜测原因。
- `media_credit_cache(provider_type TEXT,external_id TEXT,people_json TEXT,fetched_at TEXT,checked_at TEXT,error TEXT,PRIMARY KEY(provider_type,external_id))` 是可重建的完整演职员缓存，不进入备份。按外部作品身份保存，snapshot 只关联当前存在且已识别的电影／具体季；解除、合并或移除条目不会把旧关系套到新身份。
- `media_match_retry(item_id PRIMARY KEY,kind,attempts,next_at,fingerprint)` 保存识别退避状态，删除对应识别错误时由触发器清除，初始化位于 `metadata/retry.rs`。

公共 store 函数：`now()`、`setting(conn,key)->Result<Value,String>`、`set_setting(conn,key,&Value)`、`items(conn)->Result<Vec<Value>,String>`、`item(conn,id)->Result<Value,String>`、`detail(conn,id)`、`resource(conn,id)`、`resources_for(conn,item_id)`、`new_item(conn,&Value)->Result<String,String>`、`upsert_provider_item(conn,provider,provider_type,external_id,&Value,independent)->Result<String,String>`、`update_record(conn,item_id,&patch,manual)`、`merge_items(conn,source_id,target_id)`、`job_start(conn,kind,total)->Result<String,String>`、`job_update(conn,id,state,completed,total,message,error)`。

settings 常用键：`playerPath`、`playerKind`(potplayer/mpv)、`potplayerPollIntervalMs`(默认 500，50–2000 毫秒整数，下次播放生效)、`ffprobePath`、`autoScanIntervalMinutes`、`doubanAutoUpdate`、`doubanUpdateIntervalMinutes`；秘密不放 settings。

## 命令与事件

均为 `pub async fn xxx(app:tauri::AppHandle,input:serde_json::Value)->Result<Value,String>`。

- catalog: snapshot/detail/create_item/set_record/organization/update_settings。
- scan: add_source/scan/browse/remove_source。
- ignored: `media_ignored_paths` 列出规则（可选 sourceId），`media_ignore_path` 保存／取消规则（sourceId、path，remove=true 为取消；文件／目录类型由后端读取）。
- metadata: search/identify/refresh。
- probe: probe。

影视业务 IPC 命令统一为 `{input:{...}}`。观影记录使用 `patch` 对象，设置使用 `settings` 对象，标签整理统一使用 `itemIds` 数组；识别使用 `candidate` 对象，其中 `providerType` 为 movie/tv，`externalId` 为数字字符串，不接受开发期的扁平参数和候选字段别名。`media-changed`、`media-job-changed` 只提示刷新。`media-playback-changed` 携带与 player status 相同的受控播放快照（状态、作品／资源、位置、时长及单调时钟 `sampleAt`），前端直接更新进度并拒绝迟到快照，不按每条进度事件读取完整数据库。事件不附带密钥。

`media_settings_snapshot` 只返回 `settings`、`sources` 和最近任务 `jobs`，不读取或返回影视条目、演职员和收藏索引；影视设置页单独刷新此快照，切换到其他设置分类时停止轮询，并保留未保存的表单输入。

## 快照与人员索引

`media_snapshot` 默认返回 `people:[{id,name,aliases,profileUrl,departments,credits:[{itemId,scope,roles,departments}]}]` 与 `peopleIndex:{total,completed,failed}`；传入 `includePeople:false` 时只返回索引进度，不读取或返回完整名单。`media_people_snapshot` 单独返回名单与进度，供搜索和人员详情按需读取。人员按 TMDB ID 去重；角色／职务、已收录姓名／原名合并，完整名单独立于展示横排的最多 12 位演员与少量主创。`scope` 为 movie/season，每季使用 `/tv/{id}/season/{number}/aggregate_credits`，整剧关系不下放到季集。后台取得 operation 许可后逐部读取 credits/aggregate_credits，七天刷新、失败十五分钟重试，验证响应身份和数组完整性后原子替换，失败保留旧数据。头像从当前条目的受控缓存关系解析，不接受前端传入的下载地址。

`store/snapshot.rs`统一列表和单条快照的字段映射。`store::item`按主键读取本人字段，补充资料、来源IMDb、整理归属只查询当前ID；资源／播放汇总只遍历该条目后代并去重，不再先生成完整库再筛选。列表仍使用完整快照；资源排序、离线／缺失计数、播放完成状态、最近播放传播和可播放选择保持原规则。来源subject、收藏成员和资源会话查询配套索引，直接初始化，不修改业务列或个人记录。

## 季级展示与完整集目

影视列表与搜索以具体季为展示单位，整剧只用于父级上下文；存在的季条目即为已收录，不增加独立收录状态。前端 `mediaItemTitle` 在 TMDB `seasonCount=1` 且为第一季时省略季名，其他情况保留。详情仍取得整剧上下文以切换已存在的季，所有个人记录和整理操作跟随所选季，整剧与季集的已看不联动，不提供整剧已看入口。

识别或刷新具体季时，由 `metadata/seasons.rs` 验证并保存该季响应中的完整分集数组，按 TMDB season/episode 复合身份复用；缺失或无效列表不修改原资料，不由总集数虚构分集，也不创建其他季。只有当前识别文件关联到对应单集，其他集不生成资源、播放会话、进度、时长或已看。有资源或记录支撑的季保留完整集目及父剧上下文，缺少单集文件不会单独清理该季其余集目。

`media_item_metadata(item_id PRIMARY KEY REFERENCES media_items ON DELETE CASCADE,air_date TEXT,season_count INTEGER NULL)` 是 TMDB 可重建补充资料，不进入业务备份；初始化直接创建辅助表，不迁移或分摊旧整剧个人数据。snapshot 条目可携带 `airDate` 和 `seasonCount`；未知值不推断未播出或单季剧。

对外作品类型仅为 movie/series 的电影／电视剧概念，内部 season/episode 仍保留层级和独立观影记录。前端 `mediaKindName` 统一呈现电视剧；`mediaGenres` 在当前题材为空时沿父级读取，不回写或覆盖个人标签。统计 kind=series 包含 series/season/episode 的真实记录和会话；类型筛选不再单列季、单集，题材图表明确命名为题材分布。

季卡片或库内关联候选缺少海报时，显示所属整剧的海报并以整剧条目身份读取图片缓存；季自身有海报时优先显示。该回退只影响展示，不写入季资料或改变识别身份。

## 扫描、资源版本与目录监听

`file_filter::scan_candidate` 在枚举和本地浏览中排除 `._` 伴随文件，对 TS 检查连续有效包头，对 ISO/IMG 限制总读取 1MiB、检查 ISO9660/UDF（含 metadata partition）的影视目录结构，不挂载或遍历影片内容；读取错误使本轮不完整，不把旧资源误判丢失。普通视频后缀与原始 YUV 保持可索引。`automatic_rejection` 另拒绝明确附加片、原始 YUV、确认没有视频流的资源及不可靠镜像，不因时长短排除正常短片；手动关联与无资源来源资料不套用此自动门禁。

扫描写库由 `scan/write.rs` 负责；没有文件特征、来源、可用性或关联变化的资源仅做只读检查，不进入逐文件写事务，不更新 `resources.updated_at`。变化路径在立即事务内重查源状态与忽略规则；保留为历史的丢失资源不重复写入，完整扫描才处理普通首次丢失。来源 `last_scan` 与任务心跳独立更新；资源更新时间不是扫描心跳，不用于判断是否已枚举。

资源版本以 `size` 与 `modified_at` 共同判断，不计算文件 Hash。扫描检测到变化时，在同一事务中清空该资源的位置、时长、技术信息与播放覆盖区间，再更新文件特征；保留资源关联、条目个人记录与历史会话。播放检查点以启动时的文件特征为条件，在单个事务中写入进度、覆盖区间和观影记录；不匹配时拒绝旧会话写入。媒体探测结果同样只写回对应文件版本。

Windows 目录变化监听位于 scan/watch.rs：单独线程持有每个启用源的递归 FindFirstChangeNotificationW 句柄，仅订阅名称、目录、大小与最后写入变化，不订阅读取时间。静默 2 秒或持续变化达到 10 秒后请求既有完整扫描；afterCurrent 去重补扫扫描中的变化。每 2 秒同步来源及自动扫描开关，0 同时停止监听；无效通知句柄关闭，30 秒重试，重连成功主动补扫。变动通知仅作提示，不直接据此删除资源；权限失败、链接与扫描不完整仍沿用既有保护，定时扫描兜底不支持通知的目录。

媒体源停用使用单个立即事务写入停用标记、资源可用状态和条目可见性。扫描逐文件写入和最终收尾均在立即事务中检查源仍启用，停用后的旧批次返回取消，不重新激活资源。重新添加源与启用标记也在同一事务内完成。

## 忽略与资源移除

忽略规则保存在本机 settings 的 `source.ignored.<sourceId>`，值为 `[{path,directory}]`，不进入业务备份。按 Windows 大小写、扩展盘符／UNC 前缀及完整目录边界匹配，目录排除整棵子树，文件只匹配自身；新增规则验证规范路径必须在启用媒体目录内，禁止忽略源根目录。取消规则不依赖文件在线。sources 快照新增 ignoredCount；资源快照新增 ignored。

磁盘目录标记 `.youji-ignore` 不读取内容，不要求先在应用内添加忽略规则；检查限于媒体源根目录至目标路径之间的祖先，根目录也可标记。枚举目录前剪枝；逐文件写入、探测前后、自动识别入队／执行／提交与播放入口重新检查，权限错误保守拒绝，不缓存“没有标记”。标记不进入 settings 或备份，应用内忽略列表仅管理数据库规则。扫描收尾对已有索引重新检查标记，明确命中者即使扫描不完整也执行共用资源移除；未确认的普通缺失仍要求完整扫描，任一标记检查失败均暂停普通缺失清理。文件系统检查在收尾写事务前完成，事务内再校验源仍启用及资源仍属于该源。有记录者保留历史资源、个人记录及进度，移除标记后同一版本恢复可用性；无记录资源及空条目真实删除。

忽略事务将命中资源标为不可用／缺失并清除识别错误，只清理没有其他资源、独立收录、有效个人记录、收藏夹／标签、豆瓣来源、观看会话、续播位置或播放覆盖支撑的本地条目；以季为完整元数据子树检查，保留正常季的完整集目，不全库清理。无其他子条目或记录的父剧随之清理。有记录的资源作为历史引用保留，显示已忽略，不再播放、探测或自动识别。无引用和历史的资源物理删除，磁盘文件不变。扫描枚举进入目录时读取最新规则，逐文件写入时在立即事务内重查；探测写回和识别提交也检查路径／条目有效性，旧任务不能重建已清理条目。任务按媒体源／识别批次聚合，忽略只取消命中文件的工作，不取消其他文件的批次。`media_scan` 的 afterCurrent=true 支持当前扫描结束后追加一次扫描，同源追加请求去重，用于取消忽略。

`removal::resources` 是移除媒体源、普通确认丢失和忽略路径的共用清理入口，在调用方事务内按资源 ID 清理，不遍历磁盘或读媒体。先将命中资源标为不可用／缺失，再只处理关联的电影／季树与必要空父剧；其他未丢失资源（包括临时离线资源）、独立来源、个人记录、收藏夹／标签、观看历史或进度覆盖可支撑保留。只有 TMDB 匹配不保护空条目。无支撑树叶子优先真实删除；有支撑季保留完整集目，有记录的资源保留历史引用，无记录资源清理。`orphaned` 供解除资源关联后清理旧空条目，不停用已经转移的新资源。移除媒体源保留停用的源行以支撑历史外键，旧扫描批次事务内检查源停用；不触碰无关独立条目。没有资源的独立条目仍可获取资料，有历史资源但源已停用的自动元数据任务取消。

## 本地识别与资料合并

upsert_provider_item 支持 metadata JSON 字段 `{title,kind,parentId,year,overview,posterUrl,backdropUrl,genres,seasonNumber,episodeNumber,airDate,seasonCount}`；provider_type 区分 movie/tv/season/episode/subject。豆瓣没有可靠类型时使用 other，independent=true。provider 匹配创建/复用条目；只有成功获取 TMDB 元数据后才设置 recognized，豆瓣来源编号不代表识别。导入与元数据任务独立，新条目自动进入资料获取；无资源豆瓣剧集根据来源原始名称／别名、季号与季年份匹配到具体季，单季剧同样保留整剧→季，未取得唯一结果不写入部分层级。metadata/search 的 itemId 可省略（独立添加）或指定（现有条目识别），所有 TV 搜索结果直接展开为带 seasonNumber 的季候选；identify 共用同一入口，获取具体季资料后才关联，不将豆瓣季记录放到整剧或单集。自动失败在 media_match_errors 保存原因，media_match_retry 保存类型、失败次数、下次时间与识别线索指纹。可靠资料无匹配或歧义时暂停，线索变化才恢复；临时故障依次等待15分钟、1小时、6小时、24小时，之后每天重试。扫描索引提交后立即发出 media-changed，不等待 ffprobe；技术信息读取完成并释放扫描操作许可后，仅将本轮新建条目的 newItemIds 传给 automatic=true 的资料任务，通过 items_by_ids 限定读取，仍执行已有识别门禁及 retry_due；任务忙碌时由定时任务补充发现，不强制触发全库识别。手动识别不受退避限制。旧失败记录按原间隔重新判断一次。

`media_create_item` 只接受 TMDB 电影或带明确季号的电视剧候选，拒绝仅名称创建。已收录身份直接复用并保留个人记录；新条目先完整读取及验证元数据，之后以单事务保存电影或具体季及完整集目，失败不留下空条目或部分层级。选中电影／季设置 independent 以允许无资源展示；不将具体季的个人记录挂到整剧。

本地资源由 `metadata/local.rs` 精确核对作品名／官方别名，候选去重后至多检查 40 个身份；电影多类型首屏不足时补一次带年份的电影检索。季号明确时可用整剧首播年或所选季年份，未知季号仍只按季年份消歧，取到元数据后必须验证具体集存在。双语仅取同一文件自身的名称，优先同时得到多种精确证据的唯一作品；不会用不同父目录或其他季的名称证明当前作品／季。纯季目录可补季号，裸数字／泛名可从可靠祖先补作品，明确多季合集不猜具体季；同年同名或双语证据冲突仍拒绝自动关联。

`metadata/client.rs`负责原有TMDB缓存、超时与重试。`metadata/prefilter.rs`排除搜索结果中年份明确不符的电影，未知／非法年份仍核对详情，电视剧不按整剧首播年排除后续季。豆瓣名称匹配先汇总全部来源名称／别名的搜索身份并去重；有明确年份时优先核对显示名或原名精确的候选，仍验证详情中的名称、电影年份或具体季年份／季号。该层以详情正式名／原名精确命中的独立名称数消歧，重复标点／大小写不加分；最高证据唯一时直接采用，不读取其他候选的官方别名。同年同名、电影与剧集或不同来源别名仍有并列最高身份／季时保持歧义，不以排序／热度选第一项。精确层无有效结果时才扩大到其余候选详情及官方别名；缺年份时直接完整核对。IMDb优先与最终写库验证不变。此分层只用于豆瓣来源名称识别，本地双语评分仍沿用原遍历规则。

季／整剧的手动识别使用所选候选的季范围，不从首个子资源推断目标单集。`metadata/reidentify.rs` 在 save_selection 前递归校验原子树的 TMDB 归属；同季复用既有子项，跨剧／跨季或身份未知的子项阻止直接识别归并。targetItemId 合并也执行同样子树检查；只改变父条目 recognized 不足以放行。季搜索候选显式包含 seriesTitle／seasonTitle；前端以所属剧名预填搜索，完整作品与季名用于显示。

`media_identify.targetItemId` 仅允许未识别来源合并到已识别目标，且双方同 provider/provider_type 的编号不能冲突；通用底层 merge_items 仍供自动确认身份后的内部归并使用。已识别条目手动选择其他 TMDB 身份须先解除识别。

条目合并不再因评分／短评／日期不同中断。逐字段采用非空本地人工记录、本地记录、豆瓣来源记录的顺序；同优先级保留目标条目，空缺补齐，保留人工字段保护。已看与收藏合并保留已有事实，标签、收藏夹、资源、来源身份和历史会话一并转移，原始豆瓣来源资料保留。父子条目合并和不同 TMDB 身份更正的约束继续生效。

## 单季目录匹配

`media_match_season` 以 `action` 分阶段执行单季目录匹配，入口仅本地主窗口：

- `inspect`：传入 `sourceId,path,recursive`，只读已扫描且可用资源，返回文件范围与数量；默认不含子目录。路径按 Windows 目录边界、大小写与扩展前缀核对，禁止越界、重解析资源和已忽略路径；资源读取不枚举整库条目或读取范围外 technical_json。
- `preview`：同范围加 `candidate:{providerType:"tv",externalId,seasonNumber}`；通过现有完整资料校验取得所选季，不从文件首资源推断身份，不写库。返回15分钟有效、资料目录隔离的随机 `token`、本季分集及文件原关联／建议集号。未知、冲突季、附加片、合并多集默认不建议；`canOverrideSeason` 允许用户显式按集号更正季号差异，仍不自动采纳多季合集或附加片。
- `review`：`token,mappings:[{resourceId,episodeNumbers:[n,...]}],recordTargets?:{原itemId:目标key|"keep"}`，跳过的文件不提交。只读计算记录迁移、保留范围和评分／短评／日期冲突，返回 `reviewId,effects,conflicts`。目标key为 `tv:<id>`、`season:<id>:<季>` 或 `episode:<id>:<季>:<集>`。没有剩余文件的原季／分集默认迁移到明确目标；缺集或多集无法唯一归属时由用户指定或保留。所选目标作品本身的既有条目不作为待删除来源。多个来源汇入同一目标也核对来源身份冲突。
- `apply`：`token,reviewId,resolutions:{冲突key:所选原itemId}`；播放期间拒绝。一次性消费预览，事务内重查文件版本、原关联及记录／范围快照；更改后必须重新预览，过期／不同资料目录／并发旧review拒绝。只重绑选中文件并锁为人工关联。按预览迁移个人记录、整理、非冲突的非TMDB来源身份及历史；不复制旧TMDB身份、不搬运旧分集树、不重置资源进度或播放覆盖。评分／短评／日期冲突必须显式选择，已看和收藏取并集，历史日期不重新生成。清理没有剩余文件与记录的旧条目，保留其他文件、显式保留及来源冲突的条目；同季其他文件的识别失败／退避状态不重置。任一校验或写入失败整批回滚，成功广播 media-changed。
- `discard`：`token`，关闭／更换选择时释放内存预览；未关闭者过期回收，内存最多8份，不写磁盘预览或新增业务表。

## 条目清理

`media_cleanup` 只处理用户点选的条目及其子树，不提供启动或整库自动清理：

- `preview:{itemId}` 在只读事务快照中返回 `allowed,entries,summary,hasRecords,hasDoubanSource,fingerprint,blockedReason`。条目与子项全部纳入预览；只追加没有其他子项、没有自身业务记录及独立收录标记的空父级。目标独立收录标记本身不阻止用户显式清理。任何关联资源（包括离线／缺失）都阻止清理。摘要列出条目及空父级、有效个人记录、已看、评分、短评、收藏、观看历史、豆瓣观看记录、收藏夹／标签归属及关联资源数量。
- `apply:{itemId,fingerprint,deleteRecords}` 在立即事务中重新计算完整条目／子项、记录、来源与整理快照；与预览不一致时拒绝，不能仅凭数量相同绕过。存在个人／观看／来源记录或整理归属时，必须传入 `deleteRecords:true`，否则不写库；播放期间拒绝。先清理限定条目的历史、豆瓣观看记录与整理归属，再按叶到根清理个人记录、外部身份及条目；错误／重试和元数据按既有依赖删除。只回收不再被其他条目或账号记录引用的豆瓣原始来源缓存。不修改资源表、资源进度／播放覆盖、共享收藏夹／标签定义、磁盘媒体或无关条目。任一步失败整批回滚，成功广播media-changed并返回移除数和摘要。
- 前端入口为卡片右键与详情「清理条目」，有记录时显式勾选相关记录删除后才启用确认；取消／预览不删除数据，失败后仅能重新预览，不复用旧确认。清理后从浏览历史移除已删除详情帧，返回原列表而不重新请求已删除条目。豆瓣来源提示后续重新导入可能再次收录；不增加跨设备墓碑或忽略来源机制。

## 豆瓣增量导入与 IMDb 识别

豆瓣只导入新增身份：`external_ids` 已存在相同豆瓣 subject 时整条跳过，原始来源和个人记录均不更新。按时间倒序从列表开头读取，以 settings 的 `doubanImportCursor.<account>`（上次成功轮次的首条 subject ID）为边界；已存在条目只跳过、不作为停止条件。首次无边界或边界已删除时可能遍历至末页，不再每七天全量检查或只读前三页，不使用页码断点。分页批次使用立即写事务，取得写锁后再检查豆瓣身份并写入，避免WAL读快照在并行元数据写入时升级失败；不持有数据库事务等待网络。整轮成功后同样以立即事务提交新边界与任务完成状态；失败／取消保留旧边界及已导入内容，下次从头去重补齐。边界按账号保存，断开登录不清除；边界属于本机导入状态，不随备份携带。登录失效／验证仍暂停，网络有限重试，默认每六小时检查新增。

列表导入仍只写新豆瓣身份及首次个人记录，不等待逐条详情。`metadata::refresh` 仅在现有自动匹配返回无可靠候选／多候选时调用 `douban/details.rs`，后者复用隔离 WebView、与列表导入／登录互斥，验证所请求 subject ID 和详情结构；公开详情不校验登录状态或账号。限制／验证页面暂停，代次失效或取消后禁止旧任务落盘；沿用 metadata 任务的 operation 读锁，不嵌套申请。

`store/douban_source.rs` 单独负责来源查询、补充详情缓存及快照 `imdbId`。缓存追加到 `douban_source_data.raw_json` 的 detail／detailCheckedAt／detailError，不覆盖原列表字段和个人记录，不增加表迁移；仅保存在本机影视库，不进入游戏备份。成功编号长期复用，成功无编号缓存 7 天，网络／结构失败至少退避 15 分钟。IMDb 是来源线索，不放入全局唯一 external_ids，允许不同季共享整剧编号；条目合并仍保留来源信息。

`metadata/imdb.rs` 解析 `/find/{imdb}?external_source=imdb_id` 并给出候选，不承担豆瓣抓取或写库。响应中的电影、整剧、单集按来源范围分别判断；来源明确为电影且已找到唯一电影身份时，不让同编号的剧集／单集副本否决电影，也不请求这些无关详情。来源明确为剧集时忽略电影副本；同类多个合理身份仍拒绝自动选择。整剧身份先按来源季号／年份核对，无法匹配时可由唯一季年份纠正站点间不同季号；来源自身季号矛盾仍拒绝。

`metadata/imdb/episodes.rs` 处理单集来源范围。独立电影／OVA若仅对应TMDB单集，可定位到具体季集（包括第0季），候选携带episodeNumber及IMDb查询返回的单集TMDB ID；selected_data从完整季资料校验真实集号、ID与本地文件已有集号后才保存，不额外请求单集接口。个人记录只合并到该集，其他分集、特别篇季及整剧不补已看。整季来源仍定位季，不把整季已看写到单集；IMDb映射季号与豆瓣不同但季年份一致时允许重排，豆瓣一季只是TMDB跨年合并季中的一段时保留范围冲突，不能据一个单集编号把合并整季标已看。结果按作品／季／集身份去重且必须唯一；响应缺失、网络错误不当作无结果。完全无结果才回退名称，最终校验、完整季集保存及事务合并仍由selected_data／identify_item负责。

## 手动豆瓣资料与作品链接

`media_douban_metadata` 使用 `{input:{action:"preview",itemId,subject}}` 读取公开 subject 详情并返回 `{token,detail}`；仅接受豆瓣电影 HTTPS subject 链接或数字编号。与导入共用浏览窗口 busy／generation 门禁，不依赖 TMDB 凭据。`{action:"apply",itemId,token}` 只接受后端内存中绑定条目且未超过15分钟的预览；立即事务重查作品字段及来源身份，资料已变化时拒绝保存，个人记录变化不使预览失效。主界面命令权限与维护锁沿用现有约定。

简介提取覆盖 `#link-report-intra` 和 `#link-report`，优先完整折叠内容。预览海报以条目 ID 和 `previewToken` 调用 `media_artwork`，后端验证内存预览的条目、令牌、15分钟时效与资料目录，仅接受 poster；从已规范化预览中读取图片地址，不接受前端任意 URL。复用图片域名限制、豆瓣 Referer 和现有磁盘缓存，获取预览不写业务资料；预览令牌纳入前端图片缓存键，避免更换预览复用旧请求。

电影原地写为 movie，豆瓣剧原地写为无父级 season；不创建整剧、分集或占位。仅修改作品资料、recognized、外部豆瓣身份和人工资源锁，清理旧识别错误；不改个人记录、资源 ID、播放进度、覆盖或会话。`douban_source_data` 的空 `account_key` 专门保存手动详情，独立于非空账号的导入原始数据，沿用既有来源表，不新增当前元数据来源字段。解除识别保留豆瓣来源身份。刷新同样先预览确认，不自动把豆瓣转换成 TMDB；后台 TMDB 写回重查识别状态／外部身份，避免覆盖已完成的人工结果。

`media_open_source` 接收 provider/providerType/externalId，在后端构造固定豆瓣、TMDB 电影／剧／季／集或 IMDb 页面地址，再由系统浏览器打开；不接受任意 URL。详情没有分集的豆瓣季隐藏空选集区，保留季级记录与按文件选择播放，未知季号不显示为第一季。

## 技术探测与截图缓存

媒体探测只输出统一结构 `technical:{state,container,durationSeconds,totalBitrate,video:[],audio:[],subtitles:[],chapters,probedAt}`；轨道字段使用 camelCase（码率为 `bitrate`），前端与公开预览直接使用该结构，不接受原始 ffprobe streams 或旧轨道字段。

`tools::discover` 统一 ffprobe/ffmpeg 的显式路径、另一工具所配置路径同目录、PATH 与常见目录查找规则，设置分别为 `ffprobePath`/`ffmpegPath`；显式路径无效不静默回退。`media_artwork` 对未识别、缺少所请求 poster/backdrop 的本地视频按需生成 JPEG 备用图，不生成 logo/profile，不覆盖业务图片。快照 `localArtworkKey` 由资源 ID、size、modified_at 构成版本标识；图片存于现有 covers 缓存作用域，识别后不再使用，缓存不进入业务库或备份。抽帧在阻塞工作线程串行执行，有 35 秒总期限和 15 分钟失败退避，先缩放再用 thumbnail 选帧，最多 1280×720；未知时长尝试 10 秒后回退开头。生成前后验证文件版本、资源可用性、忽略和识别状态；不支持直接读取 ISO/IMG/YUV 截图。扫描不会因截图增加全量解码工作。

截图加载补充约定：列表与详情使用相同的有效视频筛选（跳过 ISO/IMG 与已确认无视频轨的资源），截图资源按路径稳定选取，与续播资源排序分开。`media_artwork` 可用 `cacheOnly` 只查询截图，命中不进入解码信号量；前端仅缺失截图进入两个并发的请求队列，后端继续串行抽帧，启动前丢弃已离屏请求，poster/backdrop 共用一次截图请求。完成的请求从 pending 删除，URL 缓存维持 500 项，离屏图片节点共最多 96 个，失败节点保留并重新按可见性请求（最短一分钟，后端真实失败仍退避十五分钟）。状态及资源版本纳入图片复用签名，识别过程中迟到的截图仍需通过前后状态校验；识别／解除识别／资源重绑均不复制或删除业务资源与记录，截图按资源版本复用。解除识别保留已有海报和背景，其优先级不变。未识别且缺背景时，详情额外提供无主视觉遮罩的截图区。抽帧磁盘缓存以 512 MiB 为回收预算，生成后至多每分钟按最近使用时间检查，只删除名称严格匹配的 media-frame-<64位十六进制>.jpg，保护当前输出，不处理远程图片、游戏封面或临时文件。

## 个人记录、播放与统计

`store::update_record` 是个人记录的统一写入入口，手动、播放完成、豆瓣首次导入和条目合并都通过它更新已看。`watched` 不保存人工保护标记，不按手动／自动区分优先级；评分、短评、收藏、显式日期等其他字段仍遵守人工修改保护。已看操作未提供日期时仅补空日期，来源导入显式提供空日期时保持未知；同一播放会话只自动标记一次，之后用户取消已看不会被该会话的重复检查点反复覆盖。

播放实现拆为 `playback.rs` 的会话与检查点、`playback/windows.rs` 的 PotPlayer 适配、`playback/mpv.rs` 的有序事件快照、`playback/pipe.rs` 的双向 Windows OVERLAPPED I/O、`playback/playlist.rs` 的剧集队列。mpv 在每个文件独立设置初始位置，避免首集续播位置套用后续集；不允许播放器自己的历史位置覆盖游迹的本次播放指令。PotPlayer 按 `potplayerPollIntervalMs` 轮询，默认 500ms，每次启动播放时读取配置；续播位置确认按累计时间判断，支持短采样间隔；mpv 消费事件而非轮询播放器，接收时刻用不含休眠的本机单调时间记录。生命周期边界保留最后有效位置，快速切集不会覆盖上一文件样本。资源和作品快照的 `playbackCompleted` 仅由有效位置达到已知总时长的 95% 推导，自动已看共用此判定，快进达到阈值同样生效；实际播放覆盖保留记录但不参与完成判定，不新增业务字段或根据已看标记推断实际完成。

PotPlayer 优先使用启动参数 `/seek=HH:MM:SS.mmm`。至少两次有效就绪采样后检查落点，在目标前后 5 秒容差内连续稳定至少 300ms 即确认，不发送额外跳转；向前容差另计检查期间的正常播放推进（最多 10 秒），暂停时不增加该容差。首次有效就绪后至少留出 1 秒稳定时间，仍有明显偏差才通过窗口接口补跳，最多再重试一次，整体确认期限仍为 20 秒。未确认前不保存加载位置或累计观看时间，保留原有进度保护。

电视剧 `playback/playlist.rs` 默认从当前分集／季追溯到所属剧，加载所有已收录可播放分集及季／整剧直接关联文件，包括第0季；按季号／集号和路径排序，缺失、离线或忽略资源跳过而不截断。每集沿最近播放资源、路径、资源ID选择一个有效版本，当前集保留本次指定版本；直接关联季的多个文件全部加入，同一物理路径在队列内去重。mpv 和 PotPlayer 均保留完整前后列表；PotPlayer 临时DPL使用 `playname` 指定当前文件、`topindex`指定位置，`saveplaypos=0`，仍沿原有命令行续播及落点确认，不改播放器配置。临时文件随会话释放。

统计记录的 `activityDates` 保存所选范围内的本机日期集合，包含来源允许的标记日期及每次实际播放日期；会话的 `activityDate` 与日历按同一本机时区计算。前端按这些字段筛选所选日期，展示日期不覆盖原始 `markedAt`。本机来源筛选可展示豆瓣条目的真实本机播放，但不将其豆瓣评分或已看标记计入本机汇总。

## 目录浏览、搜索与页面复用

`media_browse` 在 `scan/browse.rs` 中读取实际目录；智能折叠只读取当前媒体源的轻量身份层级，来源资源关系只查询一次并按当前目录的直接子目录分组。完整作品资料通过 `store::items_by_ids(conn,ids)` 按展示范围批量获取，与整库／单条快照使用同一映射；父子范围重叠时资源按各条目去重，不改变续播选源、截图键、个人记录与排序。空 ID 集合不退化为整库查询。

本地目录导航纳入浏览历史，历史最多40层；同目录刷新不入栈、不重置位置。返回先显示保存的目录／搜索结果；目录条目资料变化时后台重新读取。虚拟列表先完成当前列数布局并挂载原可见条目的分块，再恢复滚动值及条目相对视口的位置，适用于窗口和内容容器滚动。搜索异步刷新期间保留已显示结果，避免空页夹断滚动。加载错误独立于整库错误，保留目录导航和重试入口，离开后的旧请求不得改写当前页。

前端聚合搜索以整个资料库为范围，人员作品页使用独立筛选；两页与详情通过浏览历史栈覆盖主内容区，category/collectionId 保持侧栏原选择，返回只恢复浏览状态，不回滚资料快照与播放进度。侧栏导航清空临时路径，分类固定的 watched/recognized 维度覆盖对应隐藏过滤器。

影视与本地目录搜索由 Enter 或搜索按钮提交，输入期间不查询。搜索索引复用作品父级和人员关系，按分段任务构建与筛选，过期请求取消；电影／季和演职员结果各自分页，每页30项，保留总数与完整结果。搜索页暂停自动整库轮询并提供「刷新结果」，离开后恢复原有刷新规则；浏览历史保留两组页码。同名但不同 TMDB ID 的人员继续独立展示。

前端 `media-view.js` 按区域／作品身份与显示数据缓存当前挂载节点，协调时仅插入／移动／删除实际变化的子节点；离屏且不再挂载的区域缓存及时释放。播放位置的连续推进与 updatedAt 不触发区域替换，零位置／续播状态和真实资料变化仍更新，位置文本与进度条使用原有局部补丁。缓存卡片的播放、收藏及菜单操作解析当前快照，避免使用旧个人状态。详情自动读取仅比较当前作品的完整季集范围，并丢弃迟到请求；显式刷新仍重读，避免自动与显式重复读取。艺术字／演职员完成只更新相关区域，不再额外请求 snapshot。侧栏计数省略排序，片库排序在一次查询内复用作品与父级名称索引。

## 托盘最近记录与弹窗

托盘 `desktop_lifecycle/recent.rs` 从本机游戏last_played及影视view_sessions只读查询最近记录，每区最多5项；影视按电影／季归组，保留最近有效分集和资源的精确ID，不使用豆瓣标记日期充当播放记录。去除已归档游戏、离线／缺失／来源停用或关联已变化的影视。菜单显示名称（没有独立小图标时省略作品图标），不含「打开」或「继续」动作文字；左键打开窗口，末项文案为「退出」。右键通过 `desktop_lifecycle/popup.rs` 显示共用主界面亮暗主题、主题色和字体的独立WebView弹出菜单，按记录数量调整高度，以鼠标落点作为可见卡片起点，越过当前屏幕工作区时向左／上翻转并限位；可见期间记录变化使用原鼠标落点重新计算尺寸和位置。高度增加2逻辑像素舍入余量，根节点显式覆盖公共主题背景为透明且不绘制窗口外阴影，保留内部图标和卡片样式；失焦、Escape或点击外部关闭，方向键、Home／End与Enter可操作。前端只接收稳定动作ID和名称，启动、续播及退出仍由后端原入口执行。查询不读取全库作品／人员快照，事件触发并每3秒补查已落库的播放会话，仅内容变化时更新菜单快照。稳定动作ID不随排序改变，点击仍经原游戏启动／影视播放入口检查维护状态、重复运行、文件及关联；失败打开主窗口并广播既有launcher-error。数据库暂不可读时保留上次菜单，不修改业务资料。

## 耗时诊断

`metadata/worker.rs`只编排单条任务，详情补取仍由豆瓣模块负责，最终写库仍走原事务。`metadata/diagnostics.rs`通过任务局部指标记录单条总耗时、API耗时与次数、网络尝试、缓存命中、单条数据库读取耗时／次数、候选详情与跳过数。慢于2秒或失败的API另记接口类别及耗时，HTTP错误／连接异常记状态及重试序号。日志不包含查询词、片名、文件路径、账号、请求URL／参数、凭据或完整错误文本，条目ID仅允许UUID。日志写入应用数据目录logs/media-metadata.jsonl，单文件上限2MiB，另保留一份previous；写入失败不影响识别，不进入业务备份。

## 预览与回归入口

前端示例资料为 `src/lib/media-preview-metadata.json` 与 `media-preview-presentation.json`；`test/media-design-preview.html` 检查多季、离线集与多版本播放参数，`test/media-player-preview.html` 检查播放状态。预览不读取本机凭据、不启动真实播放器。

浏览器回归集中在 `test/media-*.browser.cjs` 与 `test/tray-menu.browser.cjs`，使用示例数据覆盖渲染复用、十万人员搜索、目录返回、截图、豆瓣预览、单季匹配、清理和托盘。启动预览及 Playwright／Edge 配置见 [README 验证](../../../README.md#验证)。Rust 的真实文件通知及抽帧用例只使用临时目录／合成视频；真实库、真实播放器与桌面焦点验收结果分别记录在 [项目交接](../../../../../PROJECT-HANDOFF.md)。
