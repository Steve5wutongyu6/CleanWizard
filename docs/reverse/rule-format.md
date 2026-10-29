# 清理规则 XML 格式（rule-format）

> 证据来源：`LemonClener.framework/Versions/A/LemonClener` 的 `__TEXT,__cfstring` 常量池
> - 元素/属性区 `0x85df0-0x86130`：`garbage / category / item / path / cautions / caution / filters / filter / atom / value1 / level / column / relation / title / tips / cleanType / recommend / fastMode / showResult / defaultState / isCautious / garbage_appstore / cleanemptyfolder / cleanhiddenfile`
> - 过滤关键字区 `0x82a30-0x82cb0`：`filename / filepath / bundleid / contains / match / begin with / end with / filesize / greater / smaller / signed / exclude / subfilepath / subfilecount / time / is / sub / app`
> - 解析器：`QMXMLParseManager`（NSXMLParser 封装），回调 `xmlParseDidEndCategory:` / `xmlParseDidEndCaution:` / `xmlParseDidEndFilter:`
>
> 注意：原始分析中「<Category>/<Caution>/<Filter> 为三个顶层节点」的描述，是指**解析器的三个回调**；XML 实际只有一个根 `<garbage>`，下面挂 `<category>`、`<cautions>`、`<filters>` 三类节点。本文以 `<garbage>` 为准。

## 1. 整体结构

```xml
<garbage version="2" garbage_appstore="1">
  <category title="系统垃圾">
    <item title="应用缓存" type="appcache" cleanType="sys"
          defaultState="1" recommend="0" fastMode="0" showResult="1"
          cleanemptyfolder="0" cleanhiddenfile="0" isCautious="0"
          bundleId="com.example.app" appStoreBundleId="com.example.app"
          tips="缓存可安全删除">
      <path type="dir" value="~/Library/Caches/SPACE" level="0" filename="Cache"/>
      <path type="file" value1="~/Library/Application Support/SPACE/%@" level="1"/>
      <filters level="0">
        <filter column="filename" relation="contains" value=".log"/>
        <filter column="filesize" relation="greater" value="1 MB"/>
        <filters level="1">
          <filter column="signed" relation="is" value="0"/>
        </filters>
      </filters>
    </item>
  </category>

  <cautions>
    <caution cautionID="1001" column="filepath" bundleID="com.apple.mail" appName="邮件" value="Attachments"/>
  </cautions>

  <filters>
    <filter column="filepath" relation="contains" value="~/Documents"/>
  </filters>
</garbage>
```

## 2. 元素说明

| 元素 | 对应模型类 | 说明 |
|---|---|---|
| `<garbage>` | — | 根节点。属性 `version`（规则版本，如 "2"，与文件名 `<Version>.plist` 同名）；`garbage_appstore`（App Store 分发版本是否采用本规则，1/0） |
| `<category>` | `QMCategoryItem` | 一级清单分类。属性 `title`（显示名）、`tips`（可选说明） |
| `<item>` | `QMCategorySubItem` | 子分类。多数属性挂在 item 上（见 §3） |
| `<path>` | `QMActionPathItem` | 扫描根路径。属性 `type` / `value` / `value1` / `level` / `filename` |
| `<filters>` | `QMFilterItem` | 过滤器**组**。属性 `level`（逻辑层级），可嵌套自身 |
| `<filter>` | `QMFilterItem`（叶子） | 单条过滤条件。属性 `column` / `relation` / `value` |
| `<cautions>` | — | 慎删清单容器 |
| `<caution>` | `QMCautionItem` | 慎删条目。属性 `cautionID` / `column` / `bundleID` / `appName` / `value` |
| `<atom>` | `QMActionAtomItem` | 原子动作（清理执行单元），字段 `QMActionItem` / `QMActionPathItem` |

## 3. item / path 属性语义

| 属性 | 类型 | 语义 | 模型 ivar |
|---|---|---|---|
| `title` | String | 子分类显示名 | `QMCategorySubItem` |
| `type` | String | 模块类型标识（见 clean-modules.md，如 `appcache` / `derivedapp` / `wechatImage`） | — |
| `cleanType` | enum | **清理分类**：`sys` = 系统垃圾 / `app` = 应用垃圾 / `int` = 上网垃圾 | `cleanType` |
| `bundleId` | String | 归属应用 bundle id | `_bundleId` |
| `appStoreBundleId` | String | App Store 版本对应 bundle id（与 bundleId 不同时使用） | `_appStoreBundleId` / `_appstoreBundleID` |
| `defaultState` | Bool(0/1) | 扫描结果默认是否勾选 | — |
| `recommend` | BOOL | 是否「推荐清理」项 | `recommend`（`T@"NSNumber",C,N` 含义为 char） |
| `fastMode` | BOOL | 是否参与快速模式（快速扫描只跑带该标记的项） | `fastMode` |
| `showResult` | BOOL | 命中是否展示在结果列表 | `showResult`（`Tc` = char，T@"NSNumber"`C`） |
| `isCautious` | Bool | 该子分类是否属于慎删（命中即进 Caution 流程） | — |
| `cleanemptyfolder` | BOOL | 是否清理空文件夹 | `cleanemptyfolder` |
| `cleanhiddenfile` | BOOL | 是否把隐藏文件计为可清理 | `cleanhiddenfile` |
| `tips` | String | 条目提示文案 | `_tips` |
| `level`（path 上） | int | 路径层级/顺序，用于 path 间组合 | `_level` |
| `value1` | String | 第二取值，常用于 `%@` 拼接的应用名/版本路径 | `value1` |
| `filename`（path 上） | String | 该 path 针对的文件名约束 | `_filename` |

> 编码精度证据：`_recommend/_fastMode/_showResult/_cleanemptyfolder/_cleanhiddenfile` 均为 `Tc`（char，即 `NSNumber`+BOOL），`level` 为 `Ti`（int），`logicLevel` 为 `Ti`。

## 4. 匹配器（filter.column）语义表

`column` 取值来自 `__cfstring` 区，**原文小写**。`relation` 决定比较方式。

| column | 合法 relation | 语义 | 实现要点 |
|---|---|---|---|
| `filename` | `match` / `contains` / `begin with` / `end with` | 对**文件/目录名**（basename）匹配。`match` = 正则全匹配（`assertRegex:matchStr:`）；`contains` = 子串；`begin with` = 前缀；`end with` = 后缀 | basename 取 `lastPathComponent`，注意排除 `.app` 目录本体 |
| `filepath` | `contains` / `begin with` / `end with` | 对**全路径**匹配 | 先做 `~` 与 `SPACE` 宏展开再比较 |
| `bundleid` | `match` / `is` | 应用 bundle id 精确/正则 | 对 app 路径反查 `CFBundleIdentifier` |
| `contains` | — | 子串（同 filename+contains 的简写形式） | 归并到 filename 处理 |
| `match` | — | 正则（同 filename+match 的简写形式） | 归并到 filename 处理 |
| `begin with` | — | 前缀（简写） | 同上 |
| `end with` | — | 后缀（简写） | 同上 |
| `filesize` | `greater` / `smaller` | 文件大小阈值，`value` 形如 `"1 MB"` / `"50 MB"` | 解析 <数字><空格><单位>，支持 B/KB/MB/GB，内部按字节比对 |
| `signed` | `is` | 二进制签名状态（codesign 校验） | 用 `SecStaticCode`/`codesign` 判定，失败视为未签名 |
| `exclude` | — | 排除项：命中则**不**计入清理 | 优先于所有 include 条件求值 |
| `subfilepath` | `contains` | 子目录路径过滤（对目录内相对子路径匹配） | 相对扫描根构造相对路径 |
| `subfilecount` | `greater` / `smaller` | 子文件数量阈值 | 统计目录内文件数后比较 |
| `time` | `greater` / `smaller` | 时间阈值（mtime），用于「N 天未访问」类规则 | 秒级时间戳比较 |
| `is` | — | 精确相等 | 通用 |
| `sub` / `app` | — | 子项 / 应用维度判断 | 作用域标记 |

## 5. 组合逻辑（level / relation / and / or）

- 每个 `<filters level="N">` 组内的 `<filter>` 由该组的 `relation`（`and` | `or`）组合；组本身可嵌套 `<filters>`。
- `level` **数值越大表示逻辑层级越深，越先求值**（内层先算，再与外层组合）。
- 等价伪代码：

```
eval(filters):
    if filters.hasChildren:
        parts = children.map(eval)          // 递归
        return and(parts) / or(parts) per filters.relation
    else:
        return match(filter.column, filter.relation, filter.value)
```

- 顶层 `<filters>`（garbage 直属）为全局白名单/排除，对所有 category 生效。
- `exclude` 类型条件为否定式：任一命中即整体不清理。

## 6. 路径宏

| 宏 | 展开 |
|---|---|
| `~` | `$HOME`（当前用户家目录） |
| `SPACE` | 应用名（在路径中作为目录名占位，如 `~/Library/Caches/SPACE` → `~/Library/Caches/<AppName>`） |
| `%@` | 与 `<Version>` 同名宏：规则包文件名即 `<Version>.plist`；在路径中亦复用该格式符 |

- 路径可 `/` 开头（系统级，需提权）或 `~/ ` 开头（用户级）。

## 7. 清理类型 cleanType

| 值 | 含义 | 对应数据库列 |
|---|---|---|
| `sys` | 系统垃圾 | `clean_result.sys_size` |
| `app` | 应用垃圾 | `clean_result.app_size` |
| `int` | 上网垃圾（浏览器缓存/Cookie） | `clean_result.int_size` |

## 8. 解析器行为与异常处理

- `QMXMLParseManager` 使用 `NSXMLParser`，`:didStartElement:namespaceURI:qualifiedName:attributes:` 收集属性字典，`:didEndElement:` 时按元素名回调：
  - `xmlParseDidEndCategory:` → 产出 `QMCategoryItem`
  - `xmlParseDidEndCaution:` → 产出 `QMCautionItem`
  - `xmlParseDidEndFilter:` → 产出 `QMFilterItem`
  - 各回调均有 `xxx exception = %@` 的 `@try/@catch` 包裹，**单条异常不中断整体解析**。
- `QMXMLParseDelegate`、`QMFilterParse`、`QMCleanXMLItemParseEnd` 为解析侧协作对象。

## 9. 推断点（CleanWizard 采用的处理）

1. `relation` 缺省值：推断为 `and`（组内全部满足才清理，偏保守）。
2. `level` 缺省值：推断为 0，且「数值大者先算」。
3. `cleanemptyfolder` / `cleanhiddenfile` 缺省 **false**（宁可漏扫，不可误伤）。
4. `defaultState` 缺省 false（未显式勾选不默认清理慎删项）。
5. `subfilecount` 统计口径推断为「直接子项数（递归前）」，与 `subfilepath` 的「相对子路径」独立求值。
6. `signed` 的 `value` 推断 `"1"`/`"0"` 表示是否有有效签名。
7. `fake GPU` 类 `time` 条件推断为按 mtime 比较，单位秒。
