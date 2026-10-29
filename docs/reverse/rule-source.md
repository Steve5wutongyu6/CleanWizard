# 清理规则来源逆向（rule-source）

> 目标：说明原版「腾讯柠檬清理 5.3.7」如何获得清理规则，以及本机当前的实测状态。
> 结论先行：**App 包内不含任何内置规则文件，规则 100% 在线下发**；且本机当前规则包为空。

## 1. 证据基线

| 证据 | 路径 / 方法 |
|---|---|
| 应用版本 | /Applications/Tencent Lemon.app，Info.plist 版本 5.3.7 |
| 二进制格式 | universal（x86_64 + arm64），主程序 6.2 MB，核心逻辑在 Contents/Frameworks/*.framework |
| 规则链路代码 | QMCoreFunction.framework/Versions/A/QMCoreFunction（symbol / 字符串表） |
| 规则解析/扫描代码 | LemonClener.framework/Versions/A/LemonClener |
| 本机实测请求 | ~/Library/Caches/com.tencent.Lemon/Cache.db（NSURLCache 落盘 SQLite） |
| 本机规则目录 | ~/Library/Application Support/com.tencent.lemon/Knowledge/2（**不存在 / 空**） |

## 2. 规则获取链路

### 2.1 配置下发接口

- 基址常量 `https://lemon.guanjia.qq.com`，请求路径由格式串 `%@/cleanconfig` 拼接（QMCoreFunction 字符串表）。
- 方法 `POST`，`Content-Type: application/x-www-form-urlencoded`。

### 2.2 本机实测的真实请求（关键证据）

取自 `cfurl_cache_blob_data.request_object`（entry_ID = 6，CFURLRequest 归档 plist，字段 19 = Header、字段 22 = HTTP Body）：

```
URL    : https://lemon.guanjia.qq.com/cleanconfig
Method : POST
User-Agent: Tencent Lemon/5.3.7 (Mac OS X Version 27.2 (Build 26B123))
Content-Type: application/x-www-form-urlencoded
Content-Length: 123
Body   : appVersion=5.3.7&channel=100003&macInfo=f63d87ac37e7157b717fbf77394ec063&macName=Mac16%2C13&systemVersion=27.2&updateType=0
```

响应体（`cfurl_cache_receiver_data`，41 字节）：

```json
{"code":0,"msg":"请求成功","data":{}}
```

> 注：`data` 为空对象，即服务端对该机型/渠道**不返回规则包**。这与 Knowledge 目录不存在完全吻合。

### 2.3 字段语义

| 字段 | 实测值 | 语义 | 来源 |
|---|---|---|---|
| appVersion | 5.3.7 | CFBundleShortVersionString | 实测 body |
| channel | 100003 | 渠道号，硬编码 | 实测 body |
| macInfo | f63d87ac37e7157b717fbf77394ec063 | 设备**序列号 MD5**（32 位小写 hex） | 实测 body + QMCoreFunction `deviceSerialNumberMD5` |
| macName | Mac16,13 | 机型标识符（hw.model） | 实测 body |
| systemVersion | 27.2 | SystemVersion.plist 的 ProductVersion | 实测 body |
| updateType | 0 | 请求类型（0 = 常规检查） | 实测 body |

### 2.4 响应结构（data 非空时）

QMCoreFunction 字符串表暴露了下载器消费的键：

- `Version`（版本号，整数或字符串，用于文件名与比较 `compareVersion:`）
- `fileurl` / `fileUrl`（规则包下载地址）
- `md5` / `_currentMd5` / `hash_md5`（完整性校验）

对应方法符号：

```
responseSucceededWithVersion:fileMD5:     // 成功回调：版本 + MD5
skipDownloadWithLocalVersion:LocalFileMD5: // 本地已有同版本+同 MD5 时跳过下载
needUpdateWithNetVersion:                  // 是否需要更新
loadCacheWithVersion:                      // 按版本装载磁盘缓存
decryptedFileSucceededWithVersion:         // 解密/解包完成
queryFileWithKey:version:                  // 按 key + 版本取缓存文件
```

## 3. /time 接口（服务器授时）

- 同样 `POST`，`Content-Type: application/x-www-form-urlencoded`；实测 **Body 为空**（Content-Length 未出现）。
- 实测响应（`cfurl_cache_receiver_data` entry_ID = 1，22 字节）：

```json
{"time":1793282847840}
```

毫秒时间戳，用于签名/时效校验（请求头中另有 `__hhaa__` 归档字段，是被剥离的 `__hhaa__` 内部校验数据，非业务规则）。

## 4. Knowledge 目录结构

QMCoreFunction 与 LemonClener 的字符串表均出现下列字面量：

```
~/Library/Application Support/com.tencent.lemon/Knowledge/2
~/Library/Application Support/com.tencent.lemon/KnowledgeCh/2
~/Library/Application Support/com.tencent.lemon/KnowledgeEn/2
~/Library/Application Support/com.tencent.LemonLite/KnowledgeCh/2
~/Library/Application Support/com.tencent.LemonLite/KnowledgeEn/2
```

| 目录 | 含义 |
|---|---|
| Knowledge/2 | 默认/无语言区分 |
| KnowledgeCh/2 | 简体中文规则 |
| KnowledgeEn/2 | 英文规则 |
| LemonLite/* | 极速版（LemonLite）命名空间 |

- 目录被 `setKnowledgePath:` 设置（ivar `_knowledgePath`）。
- 规则文件名：`<Version>.plist`。文件名为宏 `%@`，取 `[data objectForKey:@"Version"]`。
- 同目录另有 `^*.plist`（`.{1,}\.plist` 正则用于枚举），记录已下载版本/MD5，供 `skipDownloadWithLocalVersion:LocalFileMD5:` 比对。
- 附带发现：`%@/latestversion`（另一条版本探测路径）、`/usr/bin/unzip` 与 `ditto` unzip（解包）、`cssm_decryptData` / `CSSMPublicKeyDecryptorErrorDomain`（规则包解密，CryptoTokenKit/CommonCrypto CSSM 接口）。

## 5. 本机实测缓存（当前为空规则的直接证据）

`~/Library/Caches/com.tencent.Lemon/Cache.db` 为 NSURLCache 落盘库，表 `cfurl_cache_response` 中相关条目：

| entry_ID | request_key | time_stamp |
|---|---|---|
| 1 | https://lemon.guanjia.qq.com/time | 2026-10-14 03:45:57 |
| 6 | https://lemon.guanjia.qq.com/cleanconfig | 2026-10-14 03:46:07 |

- entry 1 响应 22 字节：`{"time":1793282847840}`
- entry 6 响应 41 字节：`{"code":0,"msg":"请求成功","data":{}}`
- 其余条目为 rdelivery 配置 / banner 图片，与清理规则无关。

同时 `~/Library/Application Support/com.tencent.lemon/` 实际只有 `LemonClean.db`、`dict2`、`trashLog`、`Owl/`、`com.tencent.bugly/`，**没有 Knowledge 目录**。

## 6. MMKV 相关

MMKV 存储位于：

```
~/Library/Caches/com.tencent.Lemon/RDelivery/mmkv/
  RDConfigKVStoragePrefix_92e1ec0d97_10001_release_0(+.crc)
  RDConfigKVStoragePrefix_data_92e1ec0d97_10001_release_0_F63D87AC37E7157B717FBF77394EC063_(+.crc)
  statistic_report_92e1ec0d97_10001_release_0(+.crc)
```

- 文件名为 MMKV 的 mmap 布局：前 4 字节 `0xA9 0x01` 为 MMKV 布局标记（`0x1A9` = MMKV_ID_LENGTH `0x07` 等常量组合），其后是 `key\x12\x11value` 变长记录。
- 实测键：`__[RDelivery]RDConfigLastPullTimestampKey` = `1791949560.602332`、`RDConfigPullFreqSoftLimitKey` = `0`、`RDConfigPullFreqHardLimitKey` = `0`、`__[RDelivery]RDRequestContextKey`。
- **重要更正**：该 MMKV 目录从文件名到键名都属于 **RDelivery 广告/推荐位配置 SDK**（`recommend_banner_page_*_config`，值是 NSKeyedArchiver 的 RDConfigInfo），**不承载清理规则版本**。
- 原始任务描述中的「sha1+版本号存于 MMKV」在本机无对应文件落地；规则版本的实际持久化位置推测为 Knowledge 目录旁的文件 + `skipDownloadWithLocalVersion:LocalFileMD5:` 磁盘比对（因无规则包，未能实测确认）。此为**未验证推断**，已在 dump-plan 中按「不依赖 MMKV」设计。

## 7. 对 CleanWizard 的约束

1. **绝不联网**：代码中不出现 lemon.guanjia.qq.com 或任何 *.qq.com 请求逻辑。
2. 规则来源只剩两条本地通道：
   - 本机 Knowledge 目录里的 `<Version>.plist`（当前不存在）；
   - App 包静态字符串（框架二进制）中提取的路径/过滤项，作为种子规则补充。
3. 因此必须自建 Seed 规则库（见 rule-format.md / dump-plan.md），格式与原版 XML 保持一致。
