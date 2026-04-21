# stata-cli

[English](README.md)

`stata-cli` 是一个面向 AI Agent 设计的 Stata 命令行工具，基于本仓库内的 Python/PyStata 后端来运行 Stata 代码、`.do` 文件和 `.dta` 数据。

这个仓库的设计目标是让 AI Agent 可以快速理解项目、安装依赖、初始化分析工作区，并在本地运行 Stata（无需 VS Code）。

同时，这个 CLI 也提供了一个面向人工使用的 REPL，支持在任意目录执行 Stata 命令并提供语法高亮。

## 安装

### 1. 安装 Stata 18

请先安装 Stata 18。

在 Windows 上，推荐使用默认安装目录：

```text
C:\Program Files\Stata18
```

在 macOS 上，Stata 通常会安装到默认目录，一般不需要额外配置路径。

如果 Stata 安装在自定义目录，请通过 `--stata-path` 参数传入，或写入 CLI 配置。

### 2. 准备 Python 后端

`stata-cli` 依赖本仓库中的本地 Python 后端。请使用 Python 3.11，因为 Stata Python bridge 与更高版本运行时不兼容。

```bash
uv sync --all-extras --python 3.11
```

### 3. 将仓库内二进制目录加入 `PATH`

本项目在 `bin/` 下提供仓库内本地二进制，因为 CLI 依赖同仓库中的 Python 后端。

克隆仓库后，将其 `bin/` 目录加入 `PATH`。

macOS / Linux：

```bash
export PATH="/absolute/path/to/stata-cli/bin:$PATH"
```

Windows PowerShell：

```powershell
$env:Path = "C:\absolute\path\to\stata-cli\bin;$env:Path"
```

Windows Command Prompt：

```bat
set PATH=C:\absolute\path\to\stata-cli\bin;%PATH%
```

如果希望永久生效，请将对应平台的命令写入 shell/profile 配置文件。

`bin/` 内的二进制会根据自身位置解析仓库根目录，因此将其保留在仓库中即可，无需额外全局安装步骤。

如果你的平台在 `bin/` 下没有现成可用的二进制，请本地构建并复制到该目录：

macOS / Linux：

```bash
./scripts/update_repo_bin.sh
```

Windows PowerShell：

```powershell
cargo build --release --manifest-path rust-cli/Cargo.toml
Copy-Item rust-cli\\target\\release\\stata-cli.exe bin\\stata-cli.exe
```

### 4. 安装 Codex skill

将仓库内的 skill 复制到 Codex 本地 skill 目录：

```bash
mkdir -p ~/.codex/skills/stata-cli
cp skills/stata-cli/SKILL.md ~/.codex/skills/stata-cli/SKILL.md
```

### 5. 验证安装

```bash
stata-cli doctor
```

## 功能

### 初始化 AI 就绪工作区

```bash
stata-cli init ./my-analysis
```

- 创建面向 Agent 的 Stata 工作目录
- 生成 `AGENTS.md`、`data/`、`do/`、`outputs/`、`scripts/`、`do/analysis.do`、`scripts/plot.py` 和 `stata-packages.md`
- 若脚手架文件已存在则直接失败，不会静默覆盖

### 运行 Stata 代码

```bash
stata-cli run --code 'display 1+1'
```

- 执行内联 Stata 代码
- 可按需传入 `--working-dir`、`--timeout`、`--stata-path`、`--stata-edition`
- 使用 `--json` 获取结构化输出

### 运行 `.do` 文件

```bash
stata-cli file /absolute/path/to/script.do
```

- 执行本地 `.do` 文件
- 在可用时返回输出、有效 session id、日志路径和图表产物

### 启动最小 REPL

```bash
stata-cli repl
```

- 在面向人工的交互式 shell 中一次执行一条 Stata 命令
- 适合快速手工探索，不建议用于 AI 工作流
- 当 `PATH` 中可找到 `stata-cli-backend` 时可在任意目录运行；也可通过 `--python` 指向已安装后端的 Python 3.11 环境
- 使用 Stata 风格提示符、语法高亮和过滤后的输出，避免额外 CLI 日志噪音

### 诊断本地环境

```bash
stata-cli doctor
stata-cli --json doctor
```

- 检查仓库根目录解析
- 检查后端脚本是否存在
- 检查 uv 管理的 Python 3.11 环境
- 运行最小后端探测

### 预览数据

```bash
stata-cli --json data view --input-dta /absolute/path/to/data.dta --max-rows 20
stata-cli --json data view --if-condition 'iq > 110' --max-rows 10
stata-cli --json data view
```

- 预览当前数据集中的行
- 通过 `--input-dta` 直接预览 `.dta` 文件中的行
- 通过 `--if-condition` 过滤
- 通过 `--max-rows` 限制行数
- 默认 `50` 行，避免 AI Agent 在对话上下文中倾倒大表

## AI-first 工作流

面向 Agent 的工作建议从这里开始：

```bash
stata-cli init ./my-analysis
```

然后保持简单工作模式：

- 将主要 Stata 逻辑放在 `do/analysis.do`
- 包含 `capture log close` 和 `set more off`
- 将完整文本输出写入 `outputs/result.txt`
- 用 `stata-cli file do/analysis.do --json` 运行分析
- 仅使用 JSON 响应检查 `status`、`error`、`log_file`、`graphs`
- 用 `data view` 做 schema 检查和小样本预览，不做整表导出
- 使用 `scripts/` 下的 Python 脚本生成最终图表并保存到 `outputs/`
- 使用第三方 Stata 包前先运行 `which <command>`，安装前先确认

### 导出数据到 CSV

```bash
stata-cli data export-csv --input-dta /absolute/path/to/data.dta --output /absolute/path/to/out.csv --replace
```

- 将 `.dta` 文件转换为 CSV
- 将当前数据集导出为 CSV
- 通过 `--replace` 覆盖已有 CSV

## 常见失败原因

- `stata-cli` 未安装或不在 `PATH` 中
- uv 管理的 Python 3.11 环境缺失
- 二进制被移出仓库，导致无法定位 Python 后端
- Stata 18 未安装，或 `--stata-path` 指向错误位置
- PyStata 或本地 Stata Python bridge 不可用
- 目标 `.do` 或 `.dta` 文件路径不存在

如果环境看起来不正确，请先运行：

```bash
stata-cli doctor
```

## License

MIT

## 致谢

本项目的设计受到 [stata-mcp](https://github.com/hanlulong/stata-mcp) 的启发，感谢原项目提供的思路和结构。
