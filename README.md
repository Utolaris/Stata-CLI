<!-- markdownlint-disable MD013 MD033 MD041 -->

<h1 align="center">stata-cli</h1>

<p align="center">
  让Codex、Claude Code、Cursor、OpenCode 等 AI agent在本机直接控制 Stata，进行计量经济学分析的一体化工具包。
</p>

<p align="center">
  <a href="SECURITY.md">安全策略</a> ·
  <a href="#许可证">许可证</a>
</p>

<p align="center">
  <a href="https://github.com/Utolaris/Stata-CLI/actions/workflows/compatibility.yml"><img alt="Blocking CI tests" src="https://github.com/Utolaris/Stata-CLI/actions/workflows/compatibility.yml/badge.svg"></a>
  <img alt="Source version v1.0.1" src="https://img.shields.io/badge/version-v1.0.1-0099CC">
  <img alt="Native Rust engine" src="https://img.shields.io/badge/Rust-native--engine-B7410E?logo=rust&logoColor=white">
  <img alt="License MIT" src="https://img.shields.io/badge/license-MIT-6DB33F">
</p>

## 这是什么

`stata-cli` 是一个原生 Rust 写的 Stata CLI 和 Agent Skill，支持 Stata 18、
Stata 19 和 StataNow。AI 代理可以用它执行 Stata 命令和 `.do` 文件、查看
`.dta` 数据集、查询本地 Stata 帮助，拿到结构化 JSON 结果。

它直接加载 Stata 自带的共享库（`libstata-mp.dylib` / `mp-64.dll`），在进程内
调用官方 `StataSO_*` C ABI，行为和你装的 Stata 完全一致。不需要 Python、
PyStata、Jupyter 或 Stata GUI。

非 REPL 命令返回结构化 JSON。另外带一个独立 REPL，有语法高亮和代码补全，
面向人类设计。


## 快速开始

### 1. 安装 Stata 18 或 19

支持 Stata 18 和 Stata 19（含 StataNow，比如 19.5）。Stata 17 及更早的版本
没测过，不保证能用。

- macOS：自动检测 `/Applications/StataNow` 和 `/Applications/Stata`。
- Windows：自动探测 `C:\Program Files` 下最新的安装，优先 `StataNow`
  （订阅版），再考虑经典 `Stata` 目录，版本从高到低（比如 `StataNow19`、
  `Stata19`）。
- 装在别处：用 `--stata-path` 指定，或设 `STATA_PATH`（也可以在 CLI
  配置里写）。

### 2. 安装 skill 包（推荐）

仓库里放了一个自包含的 skill 文件夹 `skill/stata-cli/`：`SKILL.md`、`bin/`、
`boilerplate/` 一起分发，不用克隆仓库。最快的装法是用官方 skills CLI：

```bash
npx skills add utolaris/stata-cli \
  --skill stata-cli \
  --agent codex \
  --agent claude-code \
  --global \
  --copy
```

`--copy` 是复制文件而不是建符号链接（skill 包里带二进制和模板，需要实体
文件）。按需增删 `--agent` 行，`cursor`、`opencode` 等都可以。也可以从
GitHub Releases 页面下载 `stata-cli.skill` 压缩包，解压后放进任意 agent 的
skill 文件夹。

在仓库里开发的话，把本地二进制加进 `PATH` 就行：

```bash
export PATH="/absolute/path/to/stata-cli/skill/stata-cli/bin:$PATH"
```

平台上没有对应的 `bin/` 二进制？本地构建一个复制进 `skill/stata-cli/bin/`
（macOS：`./scripts/update_repo_bin.sh`；Windows PowerShell：
`cargo install cargo-zigbuild --locked`，然后
`cargo zigbuild --release --target x86_64-pc-windows-gnu --manifest-path rust-cli/Cargo.toml`
并复制生成的 `.exe`；有 Bash 就跑 `bash ./scripts/build_windows_bin.sh`）。

### 3. 验证

```bash
stata-cli doctor
```

### 4. 初始化工作区

```bash
mkdir my-analysis
cd my-analysis
stata-cli init
```

## skill 包里有什么

| 路径 | 说明 |
| --- | --- |
| `SKILL.md` | 面向代理的说明和参考资料路由表 |
| `bin/` | 仓库本地原生二进制（macOS arm64、Windows x86-64） |
| `references/`、`packages/` | Stata 语法与包参考资料（只读当前任务需要的 1–3 个文件） |
| `boilerplate/` | `stata-cli init` 复制的骨架源 |
| `init` 之后：`data/`、`do/`、`outputs/`、`scripts/`、`AGENTS.md` | 数据、Stata 代码、输出、辅助脚本、代理说明的统一目录结构 |

## 功能

| 命令 | 作用 |
| --- | --- |
| `stata-cli run --code '...'` | 执行内联 Stata 代码，返回结构化 JSON |
| `stata-cli file /path/to/script.do` | 运行 `.do` 文件；完整输出写到旁边的日志，JSON 里带部分失败明细 |
| `stata-cli data view` | 以 JSON 预览 `.dta` 文件（`--max-rows`、`--if-condition`） |
| `stata-cli data export-csv` | 把 `.dta` 导出为 CSV（`--replace` 覆盖） |
| `stata-cli doctor` | 诊断本地 Stata 引擎 |
| `stata-cli init` | 初始化 AI 友好的工作区 |
| `stata-cli repl` | 人工 REPL：Stata 风格提示符、语法高亮、代码补全、续行处理 |
| `help <主题>` | 在 REPL 和 `run` 里渲染本地 Stata 帮助文本（`.sthlp`） |

```bash
stata-cli run --code 'display 1+1'
stata-cli file /absolute/path/to/script.do
stata-cli data view --input-dta /absolute/path/to/data.dta --max-rows 20
stata-cli data view --input-dta /absolute/path/to/data.dta --if-condition 'iq > 110' --max-rows 10
stata-cli data export-csv --input-dta /absolute/path/to/data.dta --output /absolute/path/to/out.csv --replace
```

## AI 优先工作流

代理驱动分析时，简洁至上，避免浪费TOKEN：

- 主要的 Stata 逻辑放 `do/analysis.do`（保留 `capture log close` 和
  `set more off`）
- 完整文本输出写到 `outputs/result.txt`
- 用 `stata-cli file do/analysis.do` 跑分析
- 从 JSON 里看 `status`、`error`、`partial_failure_count`、
  `partial_failures`、`log_file`、`graphs`
- `data view` 只用来做结构检查和小预览，别整表 dump
- 最终图表用 `scripts/` 下的 Python 脚本保存到 `outputs/`
- 用户明确要 Stata 图时，在 `.do` 里写 `graph export "outputs/..."`，
  别指望 CLI 自动抓图
- 用第三方 Stata 包之前先 `which <command>`，装之前先问
- 需要语法、包说明或常见模式时，翻已安装 skill 的 `references/` 和
  `packages/`

## 常见问题

| 现象 | 应该做的事 |
| --- | --- |
| 找不到 `stata-cli` | 装 skill，或把 `skill/stata-cli/bin/` 加进 `PATH` |
| 找不到 Stata | 检查 `--stata-path`、`STATA_PATH` 和 macOS 默认路径（`/Applications/StataNow`、`/Applications/Stata`） |
| `.do` / `.dta` 路径不存在 | 用绝对路径，确认文件存在 |
| 交互命令警告 | 包含 `browse`/`edit`/`shell`/`winexec`/`pause` 的 `.do` 文件需要显式确认，这是设计行为 |

可以上来就先跑：

```bash
stata-cli doctor
```

## 兼容性与限制

- 支持 Stata 18 / 19 / StataNow（例如 19.5），平台是 macOS（arm64）和
  Windows（x86-64）；Stata 17 及更早没测过。
- 每个 OS 进程只能有一个 Stata 引擎（Stata 用进程级全局状态）。并行会话
  需要独立进程；C 引擎崩了，整个 CLI 进程也会退出。
- 非 REPL 命令之间不共享会话状态。
- 输出捕获有界：累计 64 MiB，更大的输出截断并加标记，完整结果另存日志。
- Windows 二进制由 CI 交叉编译（zigbuild）并冒烟测试，但 CI 跑不了许可版
  Stata；依赖 Stata 的测试在本机跑（CI 用 `SKIP_STATA_TESTS=1` 跳过）。
- 内嵌引擎不是沙箱。用户提供的 Stata 代码按设计以调用用户的权限运行。

## Unsafe FFI（内部设计）

这个 crate 平时禁止 `unsafe`，只有一个例外：`rust-cli/src/atom/stata_engine.rs`
通过 Stata 导出的 `StataSO_*` C ABI 在进程内驱动 Stata。官方 `pystata` 用
`ctypes` 干同样的事。该模块对外只暴露安全 API：

- `StataEngine::new(stata_home, edition)`：加载 `libstata-{mp,se,be}.dylib`，
  不附加 Python 初始化引擎（不传 `-pyexec`）。进程级单例守卫拒绝第二个引擎。
- `execute(cmd)` / `run_block(code)`：跑单行命令或临时 do-file 块，返回
  `(rc, output)`。输出从 Stata 缓冲区（已扩大到 512 MB）循环排空，累计上限
  64 MiB。
- `set_break()`：预留给未来的 stop/timeout 监控线程。原子守卫保证每次执行
  最多一次 break。
- `shutdown()`：会调用 Stata 的 `_sexit` 并终止进程，只在 REPL 退出时用，
  和执行中的调用互斥。

`data view` 的预览走临时 `export delimited` CSV（`nolabel`），再按 `describe`
读到的存储类型转换：前导零字符串保持字符串，value label 返回数值码，全缺失列
保持真实类型，浮点数用 Stata 最短往返文本表示。

## 版本发布

- **v1.0.1**：支持 Stata 18/19（含 StataNow）；Skill 更新，新增 Stata 19
  功能参考；新增 Stata 19 功能 e2e 测试；macOS 构建自动 ad-hoc 签名。

## 参与贡献与安全报告

漏洞走 GitHub 私密漏洞报告渠道
（[Utolaris/Stata-CLI](https://github.com/Utolaris/Stata-CLI/security/advisories/new)），
流程见 [SECURITY.md](SECURITY.md)。别在公开 Issue 里贴凭证、配置或私人路径。

## 许可证

MIT
