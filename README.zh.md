# stata-cli

`stata-cli` 是一个本地命令行工具，用来通过本仓库里的 Python/PyStata 后端运行 Stata 代码、`.do` 文件和 `.dta` 数据。

这个仓库的目标是让 AI 代理能快速理解项目、安装依赖、初始化分析工作区，并在本机直接运行 Stata，而不必依赖 VS Code。

它也提供了一个独立的 REPL，方便人工交互，支持语法高亮和代码补全。

## 安装

### 1. 安装 Stata 18

请先安装 Stata 18。Windows 下建议使用默认路径：

```text
C:\Program Files\Stata18
```

如果 Stata 装在别的位置，可以通过 `--stata-path` 指定，或者在 CLI 配置里设置。

### 2. 准备 Python 后端

`stata-cli` 依赖仓库内的本地 Python 后端。请使用 Python 3.11，因为 Stata 的 Python bridge 与更高版本不兼容。

```bash
uv sync --all-extras --python 3.11
```

### 3. 把仓库内的 `bin/` 加入 `PATH`

这个项目把可执行文件放在仓库根目录下的 `bin/`，因为 CLI 需要和同仓库里的 Python 后端配合工作。

克隆仓库后，请把 `bin/` 目录加入 shell 的 `PATH`：

```bash
export PATH="/absolute/path/to/stata-cli/bin:$PATH"
```

如果希望永久生效，把这行写进你的 shell 配置文件。

`bin/` 里的二进制会从自身位置反推仓库根目录，所以把它放在仓库内，就不需要额外做全局安装。

如果你所在的平台还没有对应的 `bin/` 二进制，可以先本地构建，再复制进去：

macOS / Linux：

```bash
./scripts/update_repo_bin.sh
```

Windows PowerShell：

```powershell
cargo install cargo-zigbuild --locked
cargo zigbuild --release --target x86_64-pc-windows-gnu --manifest-path rust-cli/Cargo.toml
Copy-Item rust-cli\\target\\x86_64-pc-windows-gnu\\release\\stata-cli.exe bin\\stata-cli.exe
```

如果 Windows 上有 Bash，也可以运行：

```bash
bash ./scripts/build_windows_bin.sh
```

### 4. 验证安装

```bash
stata-cli doctor
```

## 功能

`stata-cli` 让本地 Stata 工作对 AI 和人工用户都更顺手：

- 使用 `stata-cli run` 执行内联 Stata 命令
- 使用 `stata-cli file` 运行 `.do` 文件
- 使用 `stata-cli data view` 和 `stata-cli data export-csv` 查看和导出 `.dta` 数据
- 使用 `stata-cli doctor` 诊断本地 Python/Stata 后端
- 使用 `stata-cli init` 初始化一个适合 AI 协作的项目骨架
- 使用 `stata-cli init` 放入工作区的 `skills/stata-cli/` 本地 Stata skill
- 使用 `stata-cli repl` 打开面向人工交互的独立 REPL

非 REPL 命令是刻意为 AI 设计的：它们返回结构化 JSON，不会把大量无关终端噪声直接刷到 stdout。

### 初始化 AI 友好的工作区

```bash
mkdir my-analysis
cd my-analysis
stata-cli init
```

`stata-cli init` 会把仓库根目录下的 `boilerplate/` 骨架复制到当前目录，为数据、Stata 代码、输出、辅助脚本和代理说明提供统一结构。
这个骨架里也包含了 `skills/stata-cli/` 的本地参考资料，供 AI 代理使用。

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

### 诊断本地环境

```bash
stata-cli doctor
```

用 `doctor` 确认仓库内的 Rust CLI、Python 后端和 Stata 安装能够正常联通。

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
- 需要 Stata 语法、包说明或常见模式时，优先阅读工作区里的 `skills/stata-cli/` 参考资料

### 导出为 CSV

```bash
stata-cli data export-csv --input-dta /absolute/path/to/data.dta --output /absolute/path/to/out.csv --replace
```

- 将 `.dta` 转成 CSV
- 如果目标 CSV 已存在，加 `--replace` 覆盖

## 常见失败原因

- `stata-cli` 没有安装，或者没有加入 `PATH`
- 缺少 uv 管理的 Python 3.11 环境
- 二进制被移出了仓库，导致它找不到 Python 后端
- 没有安装 Stata 18，或者 `--stata-path` 指向了错误的位置
- PyStata 或本地 Stata Python bridge 不可用
- 目标 `.do` 或 `.dta` 文件路径不存在

如果你觉得环境配置有问题，先运行：

```bash
stata-cli doctor
```

## 许可证

MIT
