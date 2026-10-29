# 清理模块与流程（clean-modules）

## 1. 模块清单

来源：LemonClener 字符串表 + 独立框架名 + LemonClean.db 实测数据。

| 模块 | 对应框架 | 关键类 / 字符串 | 说明 |
|---|---|---|---|
| 主清理（系统垃圾 / 应用垃圾 / 上网垃圾） | LemonClener | `QMCleanManager`、`QMScanDelegate`、`QMLiteCleanerManager` | 三大 cleanType |
| 大文件（> 50MB） | LemonBigOldFile | — | 阈值 50MB |
| 重复文件 | LemonDuplicateFile | — | 按尺寸+哈希分组 |
| 相似照片 | LemonPhotoClean | — | 视觉相似 |
| 隐私清理 | LemonPrivacyClean | `_noPrivacyBtn` | 浏览器痕迹 |
| 卸载残留 | LemonUninstaller | — | bundle id 反查 |
| 登录项 | LemonLoginItemManager / QMAppLoginItemManage | — | launchd / LoginItems |
| 磁盘分析 | LemonSpaceAnalyse | `YMTreeMap` | 树图可视化 |

## 2. 扫描队列与并发

| 项 | 值 | 来源 |
|---|---|---|
| 串行化队列标识 | `com.lemon.action.scan.queue` | LemonClener 字符串表 |
| 子队列 | `_actionDelegateOperationQueue`（`NSObject<OS_dispatch_queue>`） | ivar 编码 |
| 文件大小缓存 | `QMScanFileSizeCacheManager`（带 rwlock） | 类名 + 日志 `Failed to initialize rwlock` |

- 扫描按 **category → subItem** 两级并行展开；每个 subItem 的结果通过 `QMScanDelegate` 回调上抛。
- 进度通过 `_scantimer` / `_cleantimer` 与 `QMProgressView` 驱动。
- `QMRemoveManager` 负责实际删除，回调 `<QMRemoveManagerDelegate>`。

## 3. 结果模型

| 类 | 角色 |
|---|---|
| `QMCategoryItem` | 一级分类，持有 `_categorySubItemArray` |
| `QMCategorySubItem` | 子分类，持有 `_subItems`、`subCategoryID`、`scanFilters` |
| `QMActionItem` / `QMActionAtomItem` / `QMActionPathItem` | 清理动作单元（`actionID` / `atomItem` / `path` / `filterID`） |
| `QMResultItem` / `QMResultBrowerItem` | 结果行 / 结果浏览项 |
| `QMCleanWarnItem` | 慎删警告项（`<QMCleanWarnItemDelegate>`） |

聚合结果结构（ivar）：`_currentCleanSizeInfo`（NSMutableDictionary）、`_m_subCategoryDict`、`_installBundleDic`。

## 4. LemonClean.db 表结构（实测）

路径：`~/Library/Application Support/com.tencent.lemon/LemonClean.db`

```sql
CREATE TABLE clean_result (
  result_id   INTEGER PRIMARY KEY AUTOINCREMENT,
  total_size  UNSIGNED BIG INT NOT NULL,
  sys_size    UNSIGNED BIG INT NOT NULL,
  app_size    UNSIGNED BIG INT NOT NULL,
  int_size    UNSIGNED BIG INT NOT NULL,
  clean_type  TINYINT          NOT NULL,
  file_num    UNSIGNED BIG INT NOT NULL,
  oprate_time UNSIGNED BIG INT NOT NULL,
  createTime  UNSIGNED BIG INT NOT NULL
);

CREATE TABLE clean_status (
  status_id  INTEGER PRIMARY KEY AUTOINCREMENT,
  subcate_id varchar(20) NOT NULL,
  status     integer     NOT NULL
);
```

### 4.1 clean_result 字段语义

| 字段 | 语义 | 实测观察 |
|---|---|---|
| result_id | 自增主键 | 1..9 |
| total_size | 可清理总量（字节） | 与 sys+app+int 基本吻合 |
| sys_size / app_size / int_size | 三类分项字节数 | 纯清理（clean_type=2）时三项均为 0 |
| clean_type | 1 / 2 | 见下表 |
| file_num | 文件数 | 1 与 2 记录同 file_num（同一次扫描的清理与预演） |
| oprate_time | 操作耗时（秒） | 4 / 11 等 |
| createTime | Unix 毫秒时间戳 | 1791949624 等 |

### 4.2 clean_type 实测映射

| clean_type | 实测特征 | 推断含义 |
|---|---|---|
| 1 | 三项均有值（sys_size 占大头） | **扫描/清理总量记录** |
| 2 | sys/app/int 全为 0，oprate_time 较大 | **仅文件数记录（清理执行）** |

实测样本（本机）：

```
result_id  total        sys          app          int          type  files
8          38989124856  34056138600  2826953128   2106033128   1     3536
9          5478986000   0            0            0            2     3536
```

### 4.3 clean_status

记录每个 `subcate_id` 的勾选状态（`status` 1/2）。实测 subcate_id 形如 `1001/1002/1003/1020/2101/20011/20012/200110`，可作为「子分类 ID 命名规则」反推依据：

- 4 位 `1xxx`：主清理系统类子分类（1001..1009）
- 4 位 `2xxx`：应用类（2101）
- 5-6 位 `20xxx`：工具类子分类（20011..20018 / 200110）

## 5. 清理流程

1. **触发**：主界面点击扫描 → 向 `com.lemon.action.scan.queue` 提交任务。
2. **装载规则**：从 Knowledge 目录读 `<Version>.plist` → `QMXMLParseManager` 解析为 `QMCategoryItem` 树。
3. **展开路径**：`QMActionPathItem` 展开 `~` 与 `SPACE` 宏，定位扫描根。
4. **并发扫描**：按 category 并发，文件大小走 `QMScanFileSizeCacheManager`。
5. **过滤**：对每个候选执行 `<filters>` 求值（见 rule-format.md §4/§5）；`exclude` 优先生效。
6. **慎删判定**：命中 `<cautions>` 或 `isCautious` 的项进入 `QMCleanWarnItem` 流程。
7. **结果聚合**：按 cleanType 汇总 sys/app/int 三项，产出 `QMResultItem`。
8. **执行清理**：`QMRemoveManager` 删除，`QMActionItem` 为最小单元。
9. **落库**：写 `clean_result`（一次 type=1 汇总 + 一次 type=2 执行记录）；勾选态写 `clean_status`。

## 6. 附加观察

- 删除日志 `~/Library/Application Support/com.tencent.lemon/trashLog`，用于「恢复」能力。
- `dict2` 为 plist（bundle id → bool），记录用户对每个应用的清理选择，如 `com.tencent.xinWeChat`、`com.google.Chrome`。
- 通知名 `com.tencent.lemon.main_cleaner_clean_success`：主清理完成事件。
