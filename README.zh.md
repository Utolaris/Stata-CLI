# stata-cli

**让 OpenAI Codex、Claude Code、Cursor、OpenCode 等 AI 编码代理在本机直接运行 Stata。**

`stata-cli` 是面向 Stata 18、Stata 19 和 StataNow 的原生 Rust CLI 与 Agent
Skill。AI 代理可以用它执行 Stata 命令和 `.do` 文件、查看 `.dta` 数据集、查询
Stata 帮助，并获得结构化 JSON 结果——不需要 Python、PyStata、Jupyter 或 Stata
GUI。引擎直接加载 Stata 自带的共享库（`libstata-mp.dylib` / `mp-64.dll`），
调用官方 PyStata 桥使用的同一组 `StataSO_*` C ABI，行为与所安装的 Stata 完全一致。

适用于 AI 辅助计量经济学、统计分析、可复现研究与自动化 Stata 工作流。非 REPL
命令返回结构化 JSON；同时提供带语法高亮和代码补全的独立 REPL 供人工交互。AI
代理还可以用 `stata-cli init` 一键初始化完整的分析工作区，无需依赖 VS Code。

## 安装

### 1. 安装 Stata 18 或 19

`stata-cli` 支持 Stata 18 和 Stata 19（含 StataNow 版本，例如 19.5）。
macOS 上会自动检测 `/Applications/StataNow` 和 `/Applications/Stata`。
Stata 17 及更低版本未经测试，不保证能正常运行。

Windows 下 CLI 会自动探测 `C:\Program Files` 下最新的安装，优先 `StataNow`
（订阅版）目录，其次经典 `Stata` 目录，版本从高到低——例如先
`C:\Program Files\StataNow19`，再 `C:\Program Files\Stata19`，然后是更旧的
`StataNow18`/`Stata18`。如果 Stata 装在别的位置，可以通过 `--stata-path`
指定，或者在 CLI 配置里设置。

### 2. 安装 skill 包（推荐）

仓库里有一个自包含的 skill 文件夹 `skill/stata-cli/`：`SKILL.md`、`bin/`
和 `boilerplate/` 放在同一个文件夹里，二进制与 init 模板一起分发，
用户不需要克隆完整仓库。

最快的安装方式是用官方 skills CLI，直接从本 GitHub 仓库拉取并安装到指定
agent：

```bash
npx skills add utolaris/stata-cli \
  --skill stata-cli \
  --agent codex \
  --agent claude-code \
  --global \
  --copy
```

`--global` 表示安装到用户级 skill 目录；`--copy` 表示复制文件而不是符号链接
（skill 包内含二进制和模板，需要实体文件）。可以按需增删 `--agent` 行
（支持 `claude-code`、`cursor`、`opencode` 等）。也可以从 GitHub Releases
页面下载 `stata-cli.skill` 压缩包，解压后放进任意 agent 的 skill 文件夹。

`stata-cli init` 会从二进制旁边的 `boilerplate/`（或
`STATA_CLI_TEMPLATE_DIR` 环境变量）定位模板，运行时不再依赖仓库。

如果要在仓库内开发，也可以把 `skill/stata-cli/bin/` 加入 shell 的 `PATH`：

```bash
export PATH="/absolute/path/to/stata-cli/skill/stata-cli/bin:$PATH"
```

如果希望永久生效，把这行写进你的 shell 配置文件。

如果你所在的平台还没有对应的 `bin/` 二进制，可以先本地构建，再复制进去：

macOS：

```bash
./scripts/update_repo_bin.sh
```

Windows PowerShell：

```powershell
cargo install cargo-zigbuild --locked
cargo zigbuild --release --target x86_64-pc-windows-gnu --manifest-path rust-cli/Cargo.toml
Copy-Item rust-cli\\target\\x86_64-pc-windows-gnu\\release\\stata-cli.exe skill\\stata-cli\\bin\\stata-cli.exe
```

如果 Windows 上有 Bash，也可以运行：

```bash
bash ./scripts/build_windows_bin.sh
```

### 3. 验证安装

```bash
stata-cli doctor
```

## 版本发布

- **v1.0.1** —— 支持 Stata 18/19（含 StataNow）；Skill 更新（新增 Stata 19
  功能参考）；新增 Stata 19 功能 e2e 测试；macOS 构建自动 ad-hoc 签名。

## 功能

`stata-cli` 让本地 Stata 工作对 AI 和人工用户都更顺手：

- 使用 `stata-cli run` 执行内联 Stata 命令
- 使用 `stata-cli file` 运行 `.do` 文件
- 使用 `stata-cli data view` 和 `stata-cli data export-csv` 查看和导出 `.dta` 数据
- 使用 `stata-cli doctor` 诊断本地 Stata 引擎
- 使用 `stata-cli init` 初始化一个适合 AI 协作的项目骨架
- 在 REPL 和 `run` 里用 `help <主题>` 渲染真实的本地 Stata 帮助文本
- 使用已安装的 `stata-cli` skill 参考资料库获取 Stata 语法和包说明
- 使用 `stata-cli repl` 打开面向人工交互的独立 REPL

非 REPL 命令是刻意为 AI 设计的：它们返回结构化 JSON，不会把大量无关终端噪声直接刷到 stdout。

### 初始化 AI 友好的工作区

```bash
mkdir my-analysis
cd my-analysis
stata-cli init
```

`stata-cli init` 会把随二进制一起分发的 `boilerplate/` 骨架复制到当前目录，为数据、Stata 代码、输出、辅助脚本和代理说明提供统一结构。
骨架保持精简；Stata 参考资料库位于已安装的 `stata-cli` skill 包中
（`references/` 和 `packages/`），不会复制进每个工作区。

### 运行 Stata 代码

```bash
stata-cli run --code 'display 1+1'
```

适合短小的内联命令。返回值是结构化 JSON，AI 可以稳定读取状态、输出、日志和错误信息。

### 运行 `.do` 文件

```bash
stata-cli file /absolute/path/to/script.do
```

适合较大的 Stata 分析，也是代理驱动工作的首选方式，因为代码、日志和生成文件都留在项目工作区里。
JSON 响应里的 `output` 只保留 Stata 日志的最后一段，方便快速定位错误；如果需要完整结果，请查看 `log_file`。

### 启动 REPL

```bash
stata-cli repl
```

REPL 是一个单独面向人工的交互界面，带有 Stata 风格提示符、语法高亮、代码补全、续行处理和过滤后的输出。
在 REPL 里输入 `help <主题>` 会把真实的本机 Stata 帮助（从 Stata 安装目录的
`.sthlp` 文件读取并转成纯文本）打印到终端。裸 `help`、`search` 和 `findit`
会返回指引消息，因为这些命令在 Stata 里打开的是 GUI 窗口，不会输出到终端。
在 `.do` 文件内部，`help` 保持 Stata 原生行为。

### 诊断本地环境

```bash
stata-cli doctor
```

用 `doctor` 确认仓库内的 Rust CLI 能加载 Stata 共享库并执行探针命令。

### 处理数据

```bash
stata-cli data view --input-dta /absolute/path/to/data.dta --max-rows 20
stata-cli data view --input-dta /absolute/path/to/data.dta --if-condition 'iq > 110' --max-rows 10
```

`data view` 适合对显式指定的 `.dta` 文件做小规模预览和结构检查。非 REPL 命令之间不共享 session state，所以不要指望 `data view` 能看到上一条命令已经加载到内存里的数据。

## AI 优先工作流

做代理驱动的分析时，建议先这样开始：

```bash
mkdir my-analysis
cd my-analysis
stata-cli init
```

然后尽量保持工作方式简单：

- 把主要的 Stata 逻辑放进 `do/analysis.do`
- 保留 `capture log close` 和 `set more off`
- 把完整文本输出写到 `outputs/result.txt`
- 用 `stata-cli file do/analysis.do` 运行分析
- 从 JSON 响应里检查 `status`、`error`、`partial_failure_count`、`partial_failures`、`log_file` 和 `graphs`
- 用 `data view` 做结构检查和小预览，不要直接 dump 整张表
- 最终图表优先用 `scripts/` 下的 Python 脚本保存到 `outputs/`
- 如果用户明确要 Stata 图，请在 `.do` 文件里写明确的 `graph export "outputs/..."`，不要依赖 CLI 自动抓图
- 使用第三方 Stata 包前先运行 `which <command>`，安装前要先询问
- 需要 Stata 语法、包说明或常见模式时，阅读已安装 `stata-cli` skill 的 `references/` 和 `packages/`

### 导出为 CSV

```bash
stata-cli data export-csv --input-dta /absolute/path/to/data.dta --output /absolute/path/to/out.csv --replace
```

- 将 `.dta` 转成 CSV
- 如果目标 CSV 已存在，加 `--replace` 覆盖

## 常见失败原因

- `stata-cli` 没有安装，或者没有加入 `PATH`
- 没有安装 Stata 19.5，或者 `--stata-path` 指向了错误的位置
- 在 `--stata-path`、`STATA_PATH` 或 macOS 默认路径（`/Applications/StataNow`、`/Applications/Stata`）中找不到 Stata
- 目标 `.do` 或 `.dta` 文件路径不存在

如果你觉得环境配置有问题，先运行：

```bash
stata-cli doctor
```

## Unsafe FFI 说明

本项目一般禁止 `unsafe` 代码（`unsafe_code = "warn"`），但有一个经过批准的例外：
`rust-cli/src/atom/stata_engine.rs` 通过 Stata 共享库导出的 `StataSO_*` C ABI
在进程内驱动 Stata。Stata 没有官方 Rust API，而进程内桥接是唯一不需要独立进程的
本地方案（官方 `pystata` 通过 `ctypes` 做同样的事）。

例外被严格限制在该模块内，对外只暴露安全 API：

- `StataEngine::new(stata_home, edition)` —— 加载 `libstata-{mp,se,be}.dylib`
  并初始化引擎（不传 `-pyexec`，因此不附加任何 Python）。进程级单例守卫
  拒绝同一进程内创建第二个引擎。
- `execute(cmd)` / `run_block(code)` —— 执行单行命令或临时 do-file 块，
  返回 `(rc, output)`。输出从 Stata 缓冲区（已扩大到 512MB）循环排空，
  超过 2MB 或用户自行 `log`/`capture` 都不会丢失。
- `set_break()` —— 从监控线程中断正在执行的命令（预留给未来的 stop/timeout 功能）。
  原子守卫保证每次执行最多一次 break；取消状态来自该标志而非匹配
  `--Break--` 文本。
- `shutdown()` —— 注意：它会调用 Stata 的 `_sexit` 并直接终止当前进程，
  因此只在 REPL 退出时使用，且与执行中的调用互斥。

已知约束与风险：

- 每个 OS 进程只能有一个 Stata 引擎（Stata 使用进程级全局状态），并行会话需要独立进程。
- `StataSO_Execute` 不可重入，调用已用互斥锁串行化。
- C 引擎崩溃可能导致整个 CLI 进程退出。
- `data view` 预览通过临时 `export delimited` CSV（带 `nolabel`）生成，并按
  `describe` 读到的存储类型转换（前导零字符串保持字符串、value label 返回
  数值码、全缺失列保持真实类型）。浮点数使用 Stata 的最短往返文本表示
  （float32 存储约 8 位有效数字、double 全精度），可以精确还原存储值；
  与 pandas 对 float32 列的 float64 展开只是文本长度差异；整数列输出为 JSON 整数。

## 许可证

MIT
