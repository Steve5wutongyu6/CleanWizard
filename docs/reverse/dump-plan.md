# CleanWizard 规则 dump 方案（dump-plan）

> 核心原则：**只读本地，绝不联网**。不发起任何对腾讯域名的请求；仅读取本机文件与已安装 App 包的静态内容。

## 1. 目标

在「本机规则包为空」的前提下，为 CleanWizard 产出一份结构合法、语义明确的 Seed 规则库（seed-xml），并保留将来本机出现真实规则包时的导入能力。

## 2. 数据来源（按优先级）

| 优先级 | 来源 | 路径 / 方式 | 当前状态 |
|---|---|---|---|
| P0 | 原版规则包 | `~/Library/Application Support/com.tencent.lemon/Knowledge/2/<Version>.plist`（及其 Ch/En、LemonLite 变体） | **本机不存在** |
| P1 | App 包静态字符串 | `/Applications/Tencent Lemon.app/Contents/Frameworks/*/…` 的 `__TEXT,__cfstring` 段 | 可用 |
| P2 | 本机数据库 | `LemonClean.db`（subcate_id / clean_type 实测分布） | 可用 |
| P3 | 人工种子 | 基于逆向语义手写的通用清理项（Clener 语义） | 需维护 |

## 3. 流程

```
[1] 探测 Knowledge 目录
     ├─ 存在 → 选最新的 <Version>.plist（数值比较 Version，取最大）
     │          → XML 直接解析为内部模型 → 归一化 → seed-xml
     └─ 不存在（当前情况）→ 跳过，进入 [2]

[2] 扫描 App 包静态字符串（只读文件）
     ├─ 定位 framework 二进制（Entries/Frameworks/*.framework/Versions/A/*）
     ├─ 提取 __cfstring 常量池中符合「路径候选」的字符串
     │    规则：以 "~/" 或 "/" 开头，且包含 Library / Caches / Logs / Containers / Developer 等
     ├─ 提取模块名（appcache / derivedapp / leftcache / leftlog / wechatXxx / InstallPackage …）
     ├─ 提取 bundle id（com.apple.Safari / com.google.Chrome …）
     └─ 汇总为「候选规则素材」，**不直接生成规则**（需人工/启发式归类）

[3] 读取 LemonClean.db（只读）
     └─ 取 clean_status.subcate_id 与 clean_result.clean_type 分布 → 校准分类与 ID 规则

[4] 合成 seed-xml
     ├─ 每条规则必须带来源标注（provenance：knowledge-dump / binary-strings / authored）
     ├─ 校验：XML 合法、garbage 根节点、version 属性、cleanType ∈ {sys,app,int}
     └─ 输出 docs/../Resources/SeedRules.xml（工程内实际路径由 Lead 统一分配）
```

## 4. 只读安全约束（硬性）

1. 代码中**不得出现**任何 `lemon.guanjia.qq.com` / `*.qq.com` 的 URL 或请求构造；dump 模块不含任何网络类型（无 URLSession / NWConnection / curl）。
2. 仅使用 `FileManager` 读枚举、`NSXMLParser` 解析、`strings` 等价的内存扫描（读取 Mach-O 的 `__TEXT` 段）。
3. 不修改原版任何文件；不写入原版目录。输出只落 CleanWizard 自己的容器目录。
4. 若目标目录不可读（TCC 权限），按「来源缺失」处理并记录诊断，不重试越权访问。

## 5. 规则包格式识别

- 文件名 `<Version>.plist`，宏 `%@` 即 `[data objectForKey:@"Version"]`。
- 内存中先得到 NSData，再交给 `NSXMLParser`；若为二进制 plist 需先解归档。
- 版本比较用 `compareVersion:`（数值分段比较，非字符串字典序）。

## 6. 未验证项与降级策略

| 风险 | 现状 | 降级策略 |
|---|---|---|
| 规则版本是否另有存储（MMKV） | 本机 MMKV 目录实测只含 RDelivery 广告配置 | dump **不依赖 MMKV**，只认 Knowledge 目录 |
| Knowledge 目录不存在 | 已实测确认（cleanconfig 返回 `data:{}`） | 走 P1/P2/P3 合成 Seed |
| 规则包是否加密 | 有 `cssm_decryptData` 迹象 | 若解析失败，视为加密包，跳过并记录，不实现解密 |
| 版本目录 ch/en 选择 | 字符串表有 KnowledgeCh / KnowledgeEn | 优先读当前系统语言对应目录，回退 Knowledge/2 |

## 7. 产物结构（seed-xml）

```xml
<garbage version="1" garbage_appstore="1">
  <category title="系统垃圾">
    <item title="应用缓存" type="appcache" cleanType="sys" defaultState="1" fastMode="1">
      <path type="dir" value="~/Library/Caches/SPACE"/>
      <filters level="0">
        <filter column="filename" relation="end with" value=".zip"/>
      </filters>
    </item>
  </category>
  <category title="应用垃圾">…</category>
  <category title="上网垃圾">…</category>
  <cautions>…</cautions>
  <filters>…</filters>
</garbage>
```

## 8. 验收标准

- `dump` 单元测试：给定目录树（含/不含规则包两种），输出确定且不触网。
- XML 通过 `NSXMLParser` 往返解析无异常。
- 三分类（sys/app/int）均有条目。
- 有 provenance 字段可追溯到来源文件。
