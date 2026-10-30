# Shred vs Shred 使用说明

一个 `shred-comparison` 命令，分别选择测量口径和两端输入。无需安装、启动或连接
`jito-shredstream-proxy`；如果服务商使用名为 `ShredstreamProxy` 的 Entry RPC，
这里只是直接调用兼容协议，不依赖 Jito 的基础设施或认证服务。

| 目的 | `--mode` | 输入组合 |
| --- | --- | --- |
| 谁先看到某个 slot | `slot` | UDP、Entry gRPC、混合均可 |
| 同一个 shred 谁先送达 | `shred` | UDP vs UDP |
| 同一笔交易何时可读 | `transaction` | UDP、Entry gRPC、混合均可 |

保留默认 `slot` 兼容旧用法。对比 raw shred 送达速度建议用 `shred`；
对比业务可读交易的时间建议用 `transaction`。三种模式衡量的阶段不同，不要混在一起排名。

## 配置文件（推荐）

在仓库或下载包根目录执行：

```bash
cp configs/shred-udp-shred.env.example shred-comparison.env
# 编辑配置，然后直接运行
./run-shred-comparison.sh
```

五种模板见 [README 配置表](../README_CN.md#shred-vs-shred)，覆盖逐 shred、UDP 交易、
Entry 对 Entry、UDP 对 Entry 和 slot 首达。配置使用现有 `SHRED_*` 变量名；
`SHRED_RETENTION_SLOTS`、`SHRED_MAX_KEYS`、`SHRED_MAX_SAMPLES` 也可配置。
选择文件可用 `--config 路径` 或 `SHRED_COMPARISON_CONFIG`；不支持递归包含文件。
所有相对路径（包括 JSON 输出路径）相对于进程工作目录，启动脚本会进入脚本所在目录。
优先级：命令行 > 进程环境变量 > `.env` > 测速配置文件 > 默认值。
未知或重复配置项、语法错误、无效值及显式指定的缺失文件均报错；默认文件缺失则沿用旧行为。
`.env` 仍按旧逻辑加载。若旧 `.env` 中有 `SHRED_*`，它们会覆盖测速配置，请清除冲突项。
认证值可写在个人配置中，或者通过环境变量提供；不要提交凭据。
模板中的认证项默认注释掉，不需要认证时保持注释即可。需要时填写非空值。
文件由 dotenv 解析，支持引号和变量展开，不执行 shell 命令；包含 `$` 的字面凭据请用单引号。

下列命令行示例仍然可用，也可以用来临时覆盖配置。


## 五种常用方式

先运行 `cargo build --release --bin shred-comparison`。下面的命令也可以通过
`./run-shred-comparison.sh` 传入同样参数运行；脚本自动使用 release 构建。
下载的发行包也支持脚本透传参数，无需 Cargo。

### 1. UDP vs UDP：同一个 shred

先在两家服务商处，把 raw shred 推送目的地配置到**同一台服务器的不同 UDP 端口**。
本程序只监听，不会自动向服务商注册接收地址。

```bash
./target/release/shred-comparison --mode shred \
  --shred-bind-1 0.0.0.0:20000 --shred-name-1 Provider_A \
  --shred-bind-2 0.0.0.0:20001 --shred-name-2 Provider_B \
  --duration-sec 60 --json-output raw-shreds.json
```

按 `(slot, index, data/code 类型)` 对齐，同源重传只保留首次时间。不解码 Entry，不调用 RPC。

### 2. UDP vs UDP：解码后同一笔交易

```bash
./target/release/shred-comparison --mode transaction \
  --shred-bind-1 0.0.0.0:20000 --shred-name-1 Provider_A \
  --shred-bind-2 0.0.0.0:20001 --shred-name-2 Provider_B \
  --duration-sec 60 --json-output udp-transactions.json
```

两端独立重组、按支持的 Merkle 格式做 FEC 恢复、解码 Entry，按 `(slot, 交易首签名)` 对齐。
时间戳记录在本地解码完成后，包含重组、恢复和解码成本；Entry 的分批方式不同不影响交易对齐。
这不是交易执行、确认或上链延迟。

### 3. Entry gRPC vs Entry gRPC

```bash
export SHRED_URL_1="https://provider-a.example:443"
export SHRED_URL_2="https://provider-b.example:443"
export SHRED_TOKEN_1="your-token-a"
export SHRED_TOKEN_2="your-token-b"
./target/release/shred-comparison --mode transaction \
  --source-1 entries-grpc --source-2 entries-grpc \
  --shred-name-1 Entry_A --shred-name-2 Entry_B \
  --duration-sec 60 --json-output entries.json
```

无需认证的服务可省略 token。默认认证头 `x-token`，可通过
`--shred-token-header-1 x-api-key` / `--shred-token-header-2 x-api-key` 修改。
使用 bearer 认证时，头设为 `authorization`，对应 token 环境变量填写完整 `Bearer ...`。
HTTPS 使用系统信任根。端点和认证信息不会写入报告。

### 4. UDP vs Entry gRPC

```bash
export SHRED_URL_2="https://provider-b.example:443"
export SHRED_TOKEN_2="your-token-b"
./target/release/shred-comparison --mode transaction \
  --source-1 udp --shred-bind-1 0.0.0.0:20000 --shred-name-1 Raw_A \
  --source-2 entries-grpc --shred-name-2 Entry_B \
  --duration-sec 60 --json-output mixed.json
```

UDP 端包含本地恢复/解码，Entry 端包含服务商预处理、gRPC 传输及本地反序列化。
这是应用侧“何时拿到交易”的比较，不能当成 UDP 与 TCP 的纯网络延迟对比。
两端可以对调。

### 5. 同 slot 首达

把上述任意组合改成 `--mode slot` 即可。UDP 使用该 slot 首个可解析 data/code shred 的收包时间；
Entry gRPC 使用该 slot 首个有效 Entry 消息的接收时间。两个输入代表的处理阶段不同，
也不要求出现的是同一个 shred 或交易。

## Entry gRPC 协议要求

默认 server-streaming RPC 路径为：

```text
/shredstream.ShredstreamProxy/SubscribeEntries
```

请求为空 protobuf 消息，响应需要符合：

```protobuf
message Entry {
  uint64 slot = 1;
  bytes entries = 2; // bincode Vec<solana_entry::entry::Entry>
}
```

可分别通过 `--entries-method-1 /package.Service/Method` / `SHRED_ENTRIES_METHOD_1`
及对应的 `-2` 自定义路径，但请求/响应结构必须兼容。
这不代表自动兼容任意 JSON、Yellowstone 交易流、WebSocket 或其他 protobuf 结构。

wire 解码器支持 legacy、v0（含地址查找表）和 v1 交易。
v1 使用 `0x81` 前缀，签名位于消息之后；UDP 与 Entry gRPC 都用首个签名对齐交易。
UDP 还支持 DATA_COMPLETE 边界间的 Agave 4.x block marker，包括跨 shred 分片和零填充；marker 不计为交易。
Entry gRPC 仍须发送完整的 `Vec<Entry>`，不能直接传入原始 block marker 或任意其他流格式。
截断、尾随字节、非法数量或超大消息会拒绝并计入 `invalid` / `decode_errors`；
没有样本时应先核对格式，不能当成服务商延迟结果。单条消息上限 64 MiB、Entry 数量上限 50,000。

解析器只读取边界和签名，不执行密码学验签、交易语义校验或交易执行，也不代表支持未来的交易版本。
wire 格式参考上游 [v1 message](https://github.com/anza-xyz/solana-sdk/blob/master/message/src/versions/v1/message.rs)、
[versioned transaction](https://github.com/anza-xyz/solana-sdk/blob/master/transaction/src/versioned/mod.rs)
及 [Agave block component](https://github.com/anza-xyz/agave/blob/master/entry/src/block_component.rs) 实现。

UDP 默认 `--shred-decode-mode-N ledger`，即 raw Solana 格式；`compat` 是原有固定偏移兼容模式。
已知前缀用 `--shred-udp-payload-prefix-skip-N N` 跳过，不是任意传输封装的自动识别器。

## 如何读统计

- 1 端为 A，2 端为 B。两端均准备完成后，共用预热和测量窗口。
- `--warmup-ms` 默认 1000；预热期间持续接收，但不计入结果。
- `--duration-sec` 默认 30；测量结束后再接收一个等待窗口，只接受已有样本的另一端副本，不接收新样本。
- `--settle-window-ms` 默认 1000，是接受两端时间差的上限。超过上限的对应副本记为 `late`，原样本仍为单边。
- `matched`：双方在等待窗口内均收到的样本。领先率只以它为分母，单边样本不自动判胜。
- `only`：在等待窗口内仅该端出现的样本。服务商覆盖范围可能不同，不能直接称为网络丢包率。
- `duplicates`：同源重复；`late`：超出等待窗口、已经结算或落后保留窗口的观测。
- `b_minus_a_us`：B 时间减 A 时间，**正数 A 快，负数 B 快**。P50/P95/P99 为升序 nearest-rank 分位数。
- `loser_gap_us`：绝对时间差的分布，平局计 0；可结合它看尾部差距。
- 时间戳使用同机单调的应用层时钟；UDP 收包后打点，交易模式解码后打点。
  未声称使用内核/硬件时间戳，调度、socket 排队及解码成本会影响结果。
- header 可解析或交易可解码，并不证明属于最终主链。raw 路径不验证 leader 签名，
  匹配键不区分同 ID 的分叉冲突，不通过 RPC 验证，也不发送付费交易。

## 输出、异常和资源限制

终端和 `--json-output path.json` 包含统一统计，以及消息数、非法消息、解码错误、
本地观测队列丢弃数、FEC 恢复 shred 数。JSON `schema_version: 1`，包含模式、输入类型和测量设置，
不包含 URL 或 token。

无匹配样本、本地队列溢出或 Entry 消息解码失败时，输出 `valid: false` 和 `invalid_reasons` 并以非零状态退出；单边结果仍可查看。
绑定失败、订阅/认证失败、流提前结束或达到容量上限同样返回错误，不会假装正常完成。
观测通道有界、发送不阻塞收包路径；关注 `queue_dropped`，发生溢出的结果不能用来排名。

接收计数包含预热和结束等待阶段；对齐及领先统计只针对测量窗口。UDP 端口需要独占；不能和已有应用共用同一个端口。

UDP 请求的 socket buffer 默认 128 MiB，操作系统可能限制实际大小；本工具未统计内核/NIC 丢包，
需要另外观察服务器 UDP 和网卡计数器。

已结算 ID 默认保留 `--retention-slots 128` 个 slot，按 slot 批次回收；更旧的观测记为迟到。
未结算样本保留完整等待时间。默认 `--max-keys 1000000`、`--max-samples 5000000`，
达到上限会明确失败，不静默抽样。长时间或高流量测试按内存调整上限，或拆成较短测试。
UDP 解码器还受 `--shred-max-tracked-slots` 和 `--shred-reassembly-gap-timeout-ms` 约束；
恢复可能跨较长间隔时应适当增加 gap timeout。

原有 `SHRED_BIND_N`、`SHRED_NAME_N`、decode-mode/prefix、buffer、duration 环境变量保持支持。
新增 `SHRED_COMPARISON_MODE`、`SHRED_SOURCE_N`、`SHRED_URL_N`、`SHRED_TOKEN_N`、
`SHRED_TOKEN_HEADER_N`、`SHRED_ENTRIES_METHOD_N`、`SHRED_WARMUP_MS`、
`SHRED_SETTLE_WINDOW_MS`、`SHRED_JSON_OUTPUT`。二进制读取当前目录 `.env`，CLI 参数优先。
完整参数执行 `--help`。

## 本地验证

`cargo test --all-targets` 覆盖对齐、正负差值、单边/重复/迟到、预热及结束边界和容量限制。
UDP 测试把真实签名的 legacy/v0（含 ALT）交易放入合成的 data shred header，覆盖分片、乱序、前缀、重传和两种解码模式。
本地认证 gRPC 服务验证真实流式传输、自定义 RPC 路径、非法 payload，以及同一笔签名交易通过
UDP 和 Entry gRPC 对齐。测试还覆盖缺失数据片的真实 Reed-Solomon 恢复（纠删码片先到或后到）及五种命令行组合。这些属于本地正确性验证，不是生产服务商的性能实测。

另以真实 legacy/v0/v1 样本验证签名，并通过 Entry gRPC 和两种 UDP 解码模式对齐，覆盖含零填充的分片 block marker。
回归测试检查每个交易样本的所有截断位置，拒绝非规范/溢出的 compact-u16 长度，以及截断或未知版本的 marker。
贡献者提供的主网回放数据未在这些本地测试中复测。
