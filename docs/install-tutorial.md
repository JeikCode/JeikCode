# JeikCode 安装与部署统一指南

> **核心原则**：当前项目已全面收敛归一至 **`JeikCode/JeikCode`** 官方组织，主干发布与在线脚本基于 **`main`** 分支。
> 推荐所有用户优先使用官方一键安装脚本（自动检测平台并配置环境变量）。

---

## 1. 快速一键安装 (推荐)

一键安装脚本会自动识别操作系统架构（macOS Intel/ARM、Linux x64/ARM64、Windows x64/ARM64），从官方 GitHub Releases 下载对应的预编译静态二进制包，并将其放置于 `$HOME/.local/bin`（或系统 PATH）中。

### 1.1 Linux / macOS / HarmonyOS PC

打开终端执行以下单行命令：

```bash
curl -fsSL https://raw.githubusercontent.com/JeikCode/JeikCode/main/scripts/install.sh | bash
```

> **说明**：
> - 脚本优先安装至 `/usr/local/bin`（若有权限）或 `$HOME/.local/bin`；
> - 会自动检测并向 `~/.bashrc` 或 `~/.zshrc` 追加 PATH 环境变量配置；
> - 支持安装特定版本：`JEIKCODE_VERSION=v7.0.1 curl -fsSL ... | bash`。

### 1.2 Windows (PowerShell)

以普通用户或管理员身份打开 PowerShell 运行：

```powershell
irm https://raw.githubusercontent.com/JeikCode/JeikCode/main/scripts/install.ps1 | iex
```

> **说明**：
> - 默认安装至 `$HOME\.local\bin\jeikcode.exe`；
> - 会自动写入当前用户的系统环境变量 PATH，并在当前会话中即刻生效，无需重启终端；
> - 支持安装特定版本：`$env:JEIKCODE_VERSION="v7.0.1"; irm ... | iex`。

---

## 2. 源码编译安装 (适合开发者与二次定制)

适用于需要深度参与开发或运行最新开发中特性的场景：

### 2.1 环境准备
- **Rust 工具链**：Rust 1.88+（`rustup update`）
- **Node.js**：Node 18+ 与 npm（用于编译 WebUI 前端）

### 2.2 编译与全局安装
```bash
# 1. 克隆官方仓库
git clone https://github.com/JeikCode/JeikCode.git
cd JeikCode

# 2. 预构建 WebUI 前端资产 (Rust 编译内嵌强依赖)
cd webui && npm run build && cd ..

# 3. 编译并安装 CLI 工具至 Cargo bin 目录
cargo install --path crates/jeikcode-cli --bin jeikcode --locked
```

验证安装是否成功：
```bash
jeikcode --version
```

---

## 3. GitHub Releases 手动二进制下载

你也可以直接访问官方发布页面下载预编译制品：
🔗 **Release 下载页**：[https://github.com/JeikCode/JeikCode/releases/latest](https://github.com/JeikCode/JeikCode/releases/latest)

| 平台与架构 | 资产文件名 | 适用设备 |
| :--- | :--- | :--- |
| **macOS (Apple Silicon)** | `jeikcode-<tag>-darwin-arm64` | M1 / M2 / M3 / M4 系列 Mac |
| **macOS (Intel)** | `jeikcode-<tag>-darwin-x64` | Intel CPU Mac |
| **Linux (x86_64)** | `jeikcode-<tag>-linux-x64` | x86_64 架构 Linux 服务器 / PC (静态 musl，无需动态 glibc) |
| **Linux (ARM64)** | `jeikcode-<tag>-linux-arm64` | aarch64 架构 Linux (树莓派、鲲鹏、飞腾等) |
| **Windows (x64)** | `jeikcode-<tag>-windows-x64.exe` | 64 位 Intel / AMD Windows PC |
| **Windows (ARM64)** | `jeikcode-<tag>-windows-arm64.exe` | 骁龙 X Elite / ARM Windows 平板与笔记本 |

下载后赋予执行权限并将文件移动至你的 PATH 路径下即可。

---

## 4. 历史/离线自构建兼容脚本 (说明)

在仓库 `scripts/` 下保留的兼容脚本：
- `scripts/install-self.sh` / `scripts/install-self.ps1`：历史版本遗留入口，默认已重定向至 `JeikCode/JeikCode` 官方 `main` 分支源，支持通过 `JEIKCODE_MANIFEST_URL` 和 `JEIKCODE_DOWNLOAD_BASE` 环境变量指定自建内网镜像源。

---

## 5. 卸载与清理

项目提供了完整的卸载清理脚本：
- **Linux / macOS**：
  ```bash
  curl -fsSL https://raw.githubusercontent.com/JeikCode/JeikCode/main/scripts/uninstall.sh | bash
  ```
- **Windows (PowerShell)**：
  ```powershell
  irm https://raw.githubusercontent.com/JeikCode/JeikCode/main/scripts/uninstall.ps1 | iex
  ```
- **手动清理**：删除二进制文件 `$HOME/.local/bin/jeikcode` 及配置目录 `~/.jeikcode`。
