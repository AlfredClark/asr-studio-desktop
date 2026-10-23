[English](docs/README_en.md) | 简体中文

# ASR Studio

基于 Tauri v2 + SvelteKit 2 + Svelte 5 的本地语音识别桌面应用。录音、模型、识别全部在本机运行，全程离线可用；联网仅用于获取模型与检查更新。

## 功能

- **本地离线识别**：基于 sherpa-onnx，录音、模型加载、解码全在本机完成，不出网。
- **录音与转写**：输入设备选择、录音启停、音频文件转写、转写取消与超时结算。
- **模型管理**：模型按需下载 / 取消 / 校验 / 删除、自定义模型目录、本地占用统计。
- **词库**：分组与词条管理、批量启用 / 复制 / 移动 / 删除、分组导入导出、拼音建议、选中文本快捷添加。
- **指令**：触发文本命中后回放按键序列（按下 / 松开平衡校验）；104 键录制键盘（按住 / 点按双模式），分步编辑、拖拽排序与效果预览；本平台不支持的按键不可录制，回放前直接拒绝。
- **结果输出**：输入法注入 / 剪贴板 / 模拟粘贴三通道按配置切换，失败自动降级；输出前做焦点预检，无可输入焦点时直写剪贴板。
  - 注意：模拟输入法通道仅在 Windows 环境下经过真机验证。
  - 注意：Windows 下若服务快捷键仅使用 Alt+[字母] 组合，可能因系统自身限制无法以模拟输入法方式输出，建议改用 Ctrl / Shift 组合或切换输出通道。
- **全局快捷键**：语音、快捷词库、指令三路，可按服务运行态自动对账。
- **桌面集成**：系统托盘、无边框自绘标题栏、开机自启、窗口状态恢复、单实例聚焦 + 深链（`asr-studio://`）。
- **自动更新**：对接 GitHub Releases 的 `latest.json`，支持检查、下载安装与重启。
- **双语与外观**：`en` / `zh-CN` 双语界面；明暗主题、配色主题、字体族 / 字重 / 字号可调。
- **崩溃兜底**：前端 `ErrorBoundary` + 后端 panic 钩子落盘，不静默丢失。

## 技术栈

- 前端：SvelteKit 2（SPA，`adapter-static`）+ Svelte 5（runes）+ Tailwind CSS v4 + shadcn-svelte + Paraglide（`en` / `zh-CN`）+ tauri-specta 生成的前后端契约绑定。
- 后端：Tauri v2 + Rust（edition 2024，工具链锁定 `1.99.1`）；语音 `sherpa-onnx`，录音 `cpal`，拼音 `pinyin`，按键回放 `enigo`。
- 工具：包管理器 `pnpm@12.14.0`（Node `>= 24`）；`pnpm-lock.yaml` 与 `Cargo.lock` 均提交。

## 快速开始

```bash
# 环境：Node >= 24，pnpm@12.14.0，Rust 1.99.1
# Linux 另需系统依赖（WebKitGTK 等），见 .github/workflows/release.yml 的 Dependencies 步骤

pnpm install --frozen-lockfile   # 按锁文件安装依赖

pnpm tauri:dev                   # 完整桌面联调（Vite 固定端口 1420 + Tauri 窗口）
pnpm dev                         # 仅前端（浏览器中 Tauri 命令不可用，会自动降级）

pnpm format                      # 每次修改后必跑：Prettier + ESLint + cargo fmt
pnpm validate                    # 全量门禁：test + lint + check（含 cargo test 重生成绑定）
```

改完 `messages/` 文案后必须跑 `pnpm i18n:compile`，否则构建与 CI 会失败。

## 页面一览

| 路由          | 内容                                                |
| ------------- | --------------------------------------------------- |
| `/`           | 工作台：无就绪模型时三步引导，否则状态卡 + 最近识别 |
| `/service`    | 语音服务：服务启停、录音与转写                      |
| `/models`     | 模型管理：下载、校验、删除、自定义目录              |
| `/lexicon`    | 词库：分组与词条管理、批量操作、导入导出            |
| `/directives` | 指令：触发文本与按键序列管理、回放                  |
| `/settings`   | 设置：通用、外观与推送偏好                          |
| `/about`      | 关于：应用 / 项目 / 平台信息、诊断、更新            |

## 目录结构

```text
src/                 前端 SvelteKit SPA：routes/（页面）· components/（common / layout / shadcn-svelte）
                     · libs/（commands 契约链 · i18n · hooks · navigation · utils）· tests/
src-tauri/           后端：src/{commands（薄封装）· cores（共享能力）· features（业务）· plugins/}
                     · locales/ · capabilities/ · tauri.conf.json
docs/                ARCHITECTURE.md（结构与设计不变量）· WORKFLOW.md（命令与流程）· README_en.md（英文版）
static/              原样拷贝资源；.github/workflows/（ci.yml / release.yml）
```

分层单向依赖：`Svelte 组件 → libs/commands → commands/ → features/ · cores/`，`cores` 不得反向依赖 `features`。完整目录树与边界见 `docs/ARCHITECTURE.md`。

## 配置与发版

- 三处版本号联动：`package.json` + `src-tauri/tauri.conf.json` + `src-tauri/Cargo.toml`，经 `pnpm release`（bumpp）升级，打 `vX.Y.Z` 标签后由 `release.yml` 自动打包（deb / AppImage / rpm / dmg / msi / nsis）并生成更新签名与变更日志。
- 外部 URL 新增必须同步 `capabilities/*.json` 与 `tauri.conf.json` 的 CSP；Updater 私钥只走环境变量，永不进仓库。
- 提交走 Conventional Commits（英文，小写 `type`），由开发者手动提交；命令契约与验证矩阵见 `docs/WORKFLOW.md`。

## 文档

- `AGENTS.md`：常驻开发规范（红线、架构不变量、工作流门禁）。
- `docs/ARCHITECTURE.md`：架构、目录职责与设计不变量。
- `docs/WORKFLOW.md`：常用命令、国际化 / 命令契约 / 发版流程与验证矩阵。
- `docs/README_en.md`：本文的英文版。

## 许可

GPL-3.0-only，作者 Alfred Clark。闭源分发前请先评估 GPL 传染性义务。
