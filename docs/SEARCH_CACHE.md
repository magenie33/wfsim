# 官方搜索热缓存交接

[热门武器的端到端实测、烘焙成本和流量记录](SEARCH_CACHE_PERFORMANCE.md)

## 宿主烘焙

Node.js 从本地发布目录 `site/` 读取 `release.json`、发布的 app 和 `/api/meta` 静态内容。`officialSearchRequest()` 用发布 app 自带的 `headlessScenarioOnto()` 和 `optimizeBody()` 构造请求。阵型成员按原顺序补上默认敌人，采用榜单的种子及最终复测场数。内置模板使用空白配装、官方数据和公开候选池，紫卡、私人敌人和其他席位为空。`four_elements_v1` 与 `regular_v1` 都取 5 个入围、每候选 10 场。如需改变派单模板，增加版本化 ID。

宿主可用 `four_elements_alt_arcane_v1` 扩大副武器赋能池的覆盖。这个公开模板从当前武器可装备的赋能中去除 `secondary_fortifier`，保留完整 MOD 池。最终仍按武器和场景生成一份供下载的包。

```powershell
node scripts/bake_search_cache.mjs plan --site site --weapon torid --ruler standard_multi_target__aimed_half --out target/hot/jobs/torid-half.json
node scripts/bake_search_cache.mjs bake --site site --job target/hot/jobs/torid-half.json --out target/hot/receipts/torid-half.json --workers 10
node scripts/bake_search_cache.mjs pack --site site --receipts target/hot/receipts --out target/hot/small --pack-bytes 131072
node scripts/bake_search_cache.mjs pack --site site --receipts target/hot/receipts --out target/hot/staging --pack-bytes 131072,524288,2097152,full
```

默认 runner 为 `scripts/run_search_cache.mjs` 的 `runSearchCache()`。`--runner` 可指定同接口的本地模块。默认 `bake` 和独立验收 `assemble` 都从空缓存开始，使用 `mode: 'capture'`、`official: ruler`、`storageOptions: { remote: false, indexedDB: null }`。`--wasm`、`--bind` 透传给 runner，WASM 必须对应 `--site` 声明的指纹。`onProgress` 原样写入 stderr 的 NDJSON，每行结构为 `{ jobId, phase, progress }`，`phase` 为 `bake` 或 `verify`。stdout 返回每个命令的 JSON 结果，包含实算 `wallMs` 或档位对应的 `cacheRoot` 和包统计。

宿主补烘第二模板时，`bake --warm-from` 接受同一发布下受信 staging 的 `cacheRoot`，即直接含 `manifest.json` 的本地叶目录。使用宿主自行烘焙或经 `assemble` 独立验收的结果生成这个目录。runner 以 `mode: 'capture'`、`remote: true`、`indexedDB: null` 从本机回环服务读取热包。读取器核对身份、大小和 SHA-256，缺包、损坏或分片缺失时由本次搜索实算。若要对照第二模板的补烘成本，可将同一 job 分别写入两个 receipt，比较各自的 `wallMs`。`assemble` 会拒绝 `--warm-from`。

```powershell
node scripts/bake_search_cache.mjs plan --site site --weapon ocucor --ruler standard_multi_target --out target/hot/jobs/ocucor-base.json
node scripts/bake_search_cache.mjs bake --site site --job target/hot/jobs/ocucor-base.json --out target/hot/ocucor-receipts/base.json --workers 10
$base = node scripts/bake_search_cache.mjs pack --site site --receipts target/hot/ocucor-receipts --out target/hot/seed --pack-bytes 16777216 | ConvertFrom-Json
node scripts/bake_search_cache.mjs plan --site site --weapon ocucor --ruler standard_multi_target --template four_elements_alt_arcane_v1 --out target/hot/jobs/ocucor-alt.json
node scripts/bake_search_cache.mjs bake --site site --job target/hot/jobs/ocucor-alt.json --warm-from $base.cacheRoot --out target/hot/ocucor-receipts/alternate.json --workers 10
node scripts/bake_search_cache.mjs pack --site site --receipts target/hot/ocucor-receipts --out target/hot/live-staging --pack-bytes 16777216 --update-index
```

`officialSearchRequest()` 返回 UI 原始请求，receipt 的请求 SHA-256 也绑定这份输入。runner 应先用当前 WASM 的 `/api/computation/input` 将请求冻结成正式任务输入，再传给 Quick Fleet。缓存 identity 取自正式任务输入的捕获结果。

已完成的官方 capture 可通过 `writeSearchCacheReceipt({ site, weapon, ruler, template, request, answer, out })` 写入相同格式。`answer` 是 `runSearchCache()` 返回值，`request` 是逐字段匹配官方模板的 UI 原始请求。可选 `answer.stats.heat` 提供 `[key, runs, hits, work]`。`hits` 表示该前缀的观察次数，`work` 为该前缀实算量，使用引擎 `work_of` 的单位。没有这个观测值时，工具使用 packed shard 自带的 gas 和 runs 估算 work，并以独立模板出现次数估算重复度。

## 分包与分发

每个武器与官方榜单组合生成独立包。同一组合可合并多个官方模板的分片，重复的 `(key, runs)` 必须逐字节一致。预算档按 `观察次数 × 实算 work ÷ 单条 gzip 估算字节` 排序，并用实际完整 gzip 字节数控制上限。压缩估计是选样启发式，跨前缀可省的工作量存在重叠，完整墙钟收益由 bench 测量。

`--pack-bytes` 和 `--raw-bytes` 均以字节计。单包上限为 24 MiB gzip、128 MiB 解压 JSON 和 100,000 条分片，manifest 上限为 128 KiB。`--total-bytes` 限制同一档的 gzip 总量，按武器和榜单 ID 排序分配。`full` 收录所有已验收、已捕获的条目，任一包超出上限时该档报错。上面的 128 KiB 小包示例可以直接复用同一批 receipt。

| 模式 | `siteRoot` | `cacheRoot` |
| --- | --- | --- |
| 单档 | `<out>` | `<out>/search-cache/v1/<RELEASE_ID>/` |
| 多档 | `<out>/<budget>` | `<out>/<budget>/search-cache/v1/<RELEASE_ID>/` |

每档 stdout 输出 `siteRoot`、`cacheRoot` 和 `basePath`。浏览器默认请求 `/search-cache/v1/<RELEASE_ID>/`，其中 `<RELEASE_ID>` 来自本站 `release.json` 的 `release` 字段。静态站点或 CDN 用选定档位的 `siteRoot` 保留相对路径。现有 bench runner 显式覆盖基路径时，`cacheRoot` 仍传到含 manifest 的叶目录。

### 单组更新与撤包

`pack --update-index` 每次接受一个预算。宿主将一组武器和榜单所需的已验收 receipt 放入本次输入目录，包含要合并的各个模板。相同 `--out` 和 `--site` 会定位到同一个可信发布叶目录。此命令将本次生成的包按完整 identity 写入已有 manifest。其他 identity 保留，同一 identity 再次提交相同内容时索引字节相同。每条 stdout 的 `packs` 列出本次打包条目，`indexEntries` 是合并后的总数。下面两次更新分别给托里德分配 128 KiB、给赤毒奴寇微波枪分配全量预算。

```powershell
node scripts/bake_search_cache.mjs pack --site site --receipts target/hot/by-weapon/torid --out target/hot/live-staging --update-index --pack-bytes 131072
node scripts/bake_search_cache.mjs pack --site site --receipts target/hot/by-weapon/kuva_nukor --out target/hot/live-staging --update-index --pack-bytes full
```

增量与整表写入都先准备不可变 gzip，再在发布叶目录独占 `.manifest.lock` 并原子替换索引。增量模式在持锁期间读取原 manifest 并检查合并后的 128 KiB 上限和 `--total-bytes`。同键并发更新以最后一次取得锁并成功提交的内容为准。等待锁 10 秒超时后可重试。若宿主进程异常退出，先确认该目录已经没有写入者，再清理遗留的空锁目录。部署到 CDN 时通过同一宿主 staging 汇聚更新，并依次推送已提交的索引。

默认 `pack` 按输入 receipt 生成完整索引。显式 `pack --replace-index` 可用保留清单替换整表并撤下遗漏的键。传入已存在的空 receipt 目录可发布空索引，原有哈希文件保留供旧 manifest 完成读取。

目录包含 `manifest.json` 和 `<sha256>.json.gz`。manifest 是 `{version:1,packs:{[identitySha256]:{bytes,sha256}}}`。`identitySha256` 为按键排序的 identity JSON 的 SHA-256，`sha256` 为 gzip 文件原始字节的 SHA-256。gzip 内容为 `{version:1,identity,entries:[[key,runs,packed],...]}`。各档含 `full` 均使用 gzip level 9。相同输入、预算及 Node/zlib 版本可复现相同字节，压缩头不含时间和平台标识。更新目录时先安装哈希命名的 gzip，再原子替换 manifest。

站点服务器须把 `.json.gz` 当作二进制资源发送，保留实际 `Content-Length`，省略 `Content-Encoding` 或使用 `identity`。关闭该路径的自动压缩和 SPA 回退，缺文件返回 HTTP 404。哈希资源可设 `Cache-Control: public, max-age=31536000, immutable`，manifest 可设 `Cache-Control: no-cache`。浏览器遇到缺包、错误编码、字节数或 SHA-256 不匹配、解压失败或超限时会跳过官方包，继续使用可用的本地缓存并实算其余缺项。恢复时重新生成正确的哈希资源和 manifest。CLI 只写本地文件，部署由宿主流程管理。

## 跨发布复用

现有 nginx 配置位于 `deploy/website.nginx`。把选定 `siteRoot` 下的 `search-cache/` 映射到同源相对路径，并在 SPA 回退前为此前缀配置 `try_files $uri =404`。若由边缘 Worker 提供资源，在两份 Wrangler 配置的 `run_worker_first` 加上 `/search-cache/*`，并把前缀加入 `worker/pages.js` 中已有的静态缺包处理。可以把公开对象桶挂在相同路径，页面的读取协议相同。上线前用一个存在和一个缺失的请求分别检查二进制响应头与 404。前端看到网页的 HTML 响应头会立即停止读取并继续本地搜索。

`reuse` 按一把武器、一个官方榜单和一个模板迁移受信宿主的 capture。准备从可信发布渠道归档的旧、新 `site/`，以及宿主在原始烘焙时记录的 `receiptSha256`。该值覆盖整个 receipt 文件的原始字节，包括热度与耗时字段。已有 capture 可用受宿主管理的原始备份计算此值。工作者随返回文件提供的摘要不能证明宿主来源。外部结果走 `assemble` 的独立实算验收。

```powershell
node scripts/bake_search_cache.mjs reuse --old-site snapshots/old/site --site snapshots/new/site --receipt archive/torid-half.json --trusted-sha256 $hostReceiptSha256 --out target/hot/new-receipts/torid-half.json
node scripts/bake_search_cache.mjs pack --site snapshots/new/site --receipts target/hot/new-receipts --out target/hot/new-staging --pack-bytes 131072
```

宿主核对发布页、包清单和实际计算资产，按发布方的算法从 `engines/<id>.json` 重新计算引擎 ID。两版 WASM、绑定、worker 以及公共计算摘要 `files.rest` 必须相同。WASM 和 `rest` 绑定了 RNG、评分与 gas 算法，`protocol`、数据格式和发布声明的其他协议字段也必须相同。两套基础模板都要在旧、新发布上产生一致的 UI 请求及 WASM 冻结请求。复用其他覆盖模板的 receipt 时，其版本化模板也要通过这项对照。原 receipt 对应的模板还需通过请求 SHA-256 和两版场景摘要校验。此过程仅执行 `/api/computation/input`。

引擎或数据指纹变化时，新发布必须保留旧引擎清单，且 `accepted` 中恰有一条仍在有效期的旧引擎 `changed` 授权。宿主对实际数据包逐文件比较，核对武器、Kitgun 或 Zaw 文件的摘要，并从两版 YAML 核对继承、变形组、紫卡家族和组装部件的影响范围。宿主使用发布工具的 Python 和 PyYAML，可用 `WFSIM_PYTHON` 指定解释器。目标武器受影响、`join`、共享代码或公共数据变化、仅有文案或元数据推断、证据缺失都会返回 `rebake required`。只有页面发布号变化时，仍须通过相同的资产、请求和场景检查。

成功后，新 receipt 沿用旧场景摘要、原始 packed 字节、热度及原始烘焙耗时，重新计算目标发布的 job ID 与包 SHA-256。stdout 输出新旧记录的摘要、完整 WASM SHA-256 和核准的变动路径。新 receipt 可直接交给 `pack`，继续按发布指纹和预算生成包。

## 分布式接入点

缓存只接入前台个人优化。官方身份由创建任务时选中的榜单 ID 和引擎冻结的实际战斗条件共同确定。自定义场景使用本机的独立空间。用户关闭网站再打开时，浏览器从 IndexedDB 读取个人新增的分片，默认每武器 32 MiB、全站 128 MiB。热包请求只读取公开 manifest 和所选包。最终入围配装的复测和独立验证任务都使用实算。

引擎、数据、WASM 或分片协议变化时，客户端使用新身份。宿主可用 `reuse` 生成通过跨发布检查的新 receipt，并优先重烘失效且使用频繁的武器和场景组合。共享战斗代码或公共 MOD 的改动影响整个队列。

`plan` 输出 `version`、`release`（站点、引擎、数据和 WASM 指纹）、`weapon`、`ruler`、`template`、`jobId`。宿主在 `--jobs` 目录保存实际派出的任务。工作者使用同一发布和模板调用 `bake`，将生成的 receipt 交回宿主。派单、传输、重试与贡献计量保留给上游调度系统。receipt 包含请求 SHA-256、规范化包 SHA-256、包、可选热度观测和宿主墙钟日志字段。

```powershell
node scripts/bake_search_cache.mjs assemble --site site --jobs target/hot/jobs --receipts target/hot/returned --out target/hot/verified --pack-bytes 524288,full
```

`assemble` 核对派单归属、发布指纹、官方请求 SHA-256、包 SHA-256 和 packed v1 分片结构。它用干净的同版 runner 独立实算，逐条比较 identity、key、runs 和原始 packed 字节，然后仅从独立结果取分片和热度。`pack` 是本机受信 capture 的打包快捷路径。接收外部结果使用 `assemble`。独立实算与候选产生同等搜索成本，属于派单接入前须衡量的成本。

## 快速检查

```powershell
$env:WFSIM_CHECK_BAKE_PAGE = '1'
node --test scripts/bake_search_cache.test.mjs
node --test scripts/bake_search_cache_reuse.test.mjs
```

检查覆盖发布页面请求逐字段对照、确定性 gzip、真实压缩预算、单档与多档目录、默认下载路径、404 和损坏包拒绝，以及独立实算验收。打包测试用合成分片避免启动搜索。完整优化耗时、实际省下的 runs/work 和本机浏览器复用由 bench 记录。
