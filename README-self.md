# JeikCode 独立重构与自更新系统说明

> **归一声明**：当前项目所有架构重构、功能增强与正式制品发布已**全部合入官方仓库 `JeikCode/JeikCode` 的 `main` 分支**。
> `local-dev` 分支仅作为历史特性过渡与旧版兼容通道保留，不再作为正式发版与分发基准。

---

## 1. 架构统一概览

JeikCode 在保持完全开源中立的 Agent 内核之上，构建了现代化的终端 AI Coding 运行时体系：

1. **统一的官方代码仓**：`https://github.com/JeikCode/JeikCode`
2. **统一的发版渠道**：基于 Git Tag 的 GitHub Actions 自动化矩阵流水线（`main` 分支）
3. **统一的安装脚本**：
   - Linux / macOS：`curl -fsSL https://raw.githubusercontent.com/JeikCode/JeikCode/main/scripts/install.sh | bash`
   - Windows：`irm https://raw.githubusercontent.com/JeikCode/JeikCode/main/scripts/install.ps1 | iex`
4. **统一的客户端自更新机制**：
   - 客户端通过内置 `/upgrade` 命令检查更新；
   - 默认读取 `https://raw.githubusercontent.com/JeikCode/JeikCode/main/latest.json` 版本清单；
   - 从 `https://github.com/JeikCode/JeikCode/releases/download` 下载经过 SHA256 校验的跨平台预编译二进制，支持热替换当前进程执行文件。

---

## 2. 开发者发版与安装指南导航

为了避免文档分散与陌生 Agent / 开发者迷航，项目的安装与发布流程现已统一定位到以下权威文档：

- 📘 **安装与部署权威指南**：详见 [`docs/install-tutorial.md`](./docs/install-tutorial.md)
- 🚀 **自动化发版与 Release 指南**：详见 [`docs/release-tutorial.md`](./docs/release-tutorial.md)
- ⚙️ **项目全局开发与贡献约束**：详见 [`AGENTS.md`](./AGENTS.md)
