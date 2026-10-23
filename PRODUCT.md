# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

> 打包形态是桌面应用（Tauri v2 + 系统 WebView），但界面语言与设计基座是 Web（shadcn-svelte + Tailwind），不按任一移动端 OS 的原生设计语言适配。

## Users

**主要用户：需要在本机完成语音转文字的人**（会议记录、字幕制作、写作输入）。

- 场景：在 Windows / macOS / Linux 桌面上录音，转写为文字后取用；全程无网络也能用。
- 他们的任务：开始录音 → 停止后得到转写文本 → 复制或推送到正在用的应用。

## Product Purpose

提供**完全本地离线的桌面语音识别**：录音、模型、识别全部在本机运行，转写全程不出网。

成功 = 用户在无网络环境下完成一次录音转写，拿到可用文本。

## Positioning

**「本地优先」**——识别能力不依赖云端。联网只用于获取模型与检查更新，拿到模型后即可断网使用。

## Operating Context

- **平台与分发**：仅桌面三平台（Windows / macOS / Linux），打包目标 deb / AppImage / rpm / app / dmg / msi / nsis；自动更新对接 GitHub Releases `latest.json`。
- **开发工作流**：`pnpm`（Node ≥ 24、pnpm@12.14.0）+ 锁定 Rust 工具链；日常入口 `pnpm tauri:dev` / `pnpm format` / `pnpm validate`；提交走 Conventional Commits（英文、小写），手动提交；CI（`ci.yml`）与发布（`release.yml`）已就位。
- **初始化流程**：改名、换协议、换更新密钥、换图标、文档收尾、验收矩阵已按初始化方案执行，见初始化提交。
- **评估面**：`README.md`（简体中文）+ `docs/README_en.md`、`docs/ARCHITECTURE.md` / `docs/WORKFLOW.md`，以及运行中的应用本身（`/` 工作台、`/service`、`/models`、`/lexicon`、`/directives`、`/settings`、`/about`）。
- **页面分工**（已确认）：`/` 工作台承载状态一览与最近识别；`/service` 承载启停、录制测试与输出设置；`/models` 承载模型获取；`/lexicon` 承载分组与词条管理；`/directives` 承载触发词与按键序列的录制回放；`/settings` 承载通用、外观与推送偏好；`/about` 承载信息 / 诊断 / 更新；各页改动后必须仍然真实可用。

## Capabilities and Constraints

**已确认能力：**

- 本地离线语音识别：录音转写全程在本机完成，不出网。
- 体验优化（不展开）：模型按需获取、结果呈现与推送、热词后处理。
- 桌面外壳：无边框窗口 + 自绘标题栏、系统托盘、关闭行为（prompt / exit / minimize_to_tray）、开机自启、窗口状态恢复、单实例聚焦 + 深链转发、更新检查（120s 超时）。
- 双语界面（en / zh-CN）与前后端崩溃兜底沿用现状。

**约束：**

- 许可证 GPL-3.0-only（本项目闭源分发前必须评估传染性义务）；作者 Alfred Clark。
- 语言仅 en / zh-CN 两个（`Locale` 枚举，禁裸 string；繁体归 zh-CN）。
- 生成物禁手改：`bindings.ts` 只经 `cargo test` 重生成，`paraglide/` 只经 `pnpm i18n:compile`。
- `AGENTS.md` 的安全红线与架构不变量为最高效力规则，任何产品改动不得触碰。

**未决事实：**

- 移动端（iOS / Android）：当前仅桌面三平台，不承诺移动端可迁移；如要扩展需单独确认。
- GPU 加速：可选优化，以 CPU 可用为发布基准；覆盖平台待验证结论。

## Brand Commitments

- 名称：仓库 / 包名 `asr-studio-desktop`，展示名 `ASR Studio`。
- 标识：波形图标（`src-tauri/app-icon.svg` 与内联 `app-icon.svelte` 同源几何），页脚只留版权行（`© {year} Alfred Clark. All rights reserved`）。
- 声音与语气：技术、精确、直接；README 简体中文为主 + 英文镜像（`docs/README_en.md`），文档中的术语与代码标识符保留英文。
- 字体：Geist Variable 为默认界面字体（可被用户系统字体偏好覆盖）。

## Evidence on Hand

- `README.md` + `docs/README_en.md`：功能清单、快速开始、页面一览、配置说明。
- `docs/ARCHITECTURE.md`（结构、分层、设计不变量）与 `docs/WORKFLOW.md`（命令契约、i18n、CI、验证矩阵）。
- 初始化方案与验收矩阵（已执行，见初始化提交）。
- 可用模型以实际集成为准，本文不列具体名。
- **不存在且禁止编造**：用户评价、采用数据（star / 下载量）、客户案例、性能基准、企业背书。

## Product Principles

1. **离线优先**：录音、模型、识别全本地，不出网；联网仅用于模型获取与更新检查。
2. **真实可用**：能力以真实可用的接线存在，而不是文档承诺或玩具示例。
3. **页面即证据，但不是全部证据**：`/`、`/models`、`/settings`、`/about` 是页面自身承载的那部分能力的验证面，改动后必须仍然真实可用、可被截图取证；不为观感牺牲真实性。托盘推送等能力**不要求有界面对象**——它们的证据是接线代码与测试，不是页面；不要为了让能力「可见」而给界面加多余控件。
4. **桌面三平台为准**：一切设计决策以 Windows / macOS / Linux 桌面使用场景为前提，除非另行确认扩展移动端。
5. **规则优先于便利**：`AGENTS.md` 的安全红线与架构不变量高于局部实现便利；生成物、门禁、配置唯一来源不因产品需求绕过。

## Accessibility & Inclusion

- 双语（en / zh-CN）为产品级要求，任何界面文案改动必须同步两份。
- 界面字体族 / 字重 / 字号为用户可调项（系统字体选择器 + 滑块），须保留可用性；自绘无边框标题栏必须保持窗口控制（置顶 / 最小化 / 最大化 / 关闭）可识别可操作。
- 录音中必须有持续可见的状态指示，开始与停止可识别可操作。
- 未发现其他已确认的无障碍标准或用户需求；后续如需 WCAG 级别目标需单独确认。
