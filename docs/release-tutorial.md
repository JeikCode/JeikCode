# JeikCode 本地编译与自动化发版权威指南

> **核心声明**：
> 1. 本文档为 JeikCode 项目**唯一的本地编译与发布权威指南**，所有历史旧版文档与手动交叉编译流程已全部废除。
> 2. 官方代码仓为 **`https://github.com/JeikCode/JeikCode`**，发布主干严格以 **`main`** 分支为准（`local-dev` 仅作为向下兼容与阶段性研发分支）。
> 3. 本文仅保留日常开发与发版最核心的 **3 个标准场景**。

---

## 场景一：改动了前端 (WebUI) 时，如何快速编译出最终 Windows 成品

当你修改了 `webui/` 目录下的 React 前端代码、组件或样式，需要输出包含最新界面的 Windows 可执行程序成品时：

### 1. 执行命令
在项目根目录下依次执行：

```powershell
# 步骤 1：构建 WebUI 前端生产静态包
cd webui
npm run build
cd ..

# 步骤 2：编译 Windows 最终 Release 成品
cargo build --release --bin jeikcode
```

### 2. 成品输出路径
- **可执行文件**：`target/release/jeikcode.exe`

### 3. 底层机制与注意事项
- **打包内嵌原理**：`crates/jeikcode-cli` 使用了 `rust-embed`，在 Rust 编译期会将 `webui/dist/` 目录下的所有 HTML/JS/CSS 资源直接压缩内嵌进生成的 `jeikcode.exe` 单一二进制文件中，运行时由 Axum 本地 Web 服务直接在内存中提供。
- **为什么必须先 `npm run build`**：如果仅运行 `cargo build` 而不重新执行前端构建，Rust 编译器只会将**上一次旧的** `webui/dist` 资源打包进去，导致你在浏览器或 Web 视图中看不到前端改动。因此改了前端后，必须先执行 `npm run build` 生成新的 `dist`，再编译 Rust 成品。

---

## 场景二：没改前端 (仅后端/核心逻辑) 时，如何快速复用缓存秒级编译

当你只修改了 Rust 后端代码（例如 `jeikcode-coding`、`jeikcode-capabilities`、`jeikcode-kernel` 等），没有动 `webui/` 前端时：

### 1. 执行命令

- **输出 Release 正式成品**：
  ```powershell
  cargo build --release --bin jeikcode
  ```
- **日常快速调试运行 (Debug 模式，编译速度最快)**：
  ```powershell
  cargo build --bin jeikcode
  ```

### 2. 成品输出路径
- **Release 模式**：`target/release/jeikcode.exe`
- **Debug 模式**：`target/debug/jeikcode.exe`

### 3. 底层机制
- **无需构建前端**：Rust 编译器在编译 `crates/jeikcode-cli` 时，会自动复用已经存在的 `webui/dist` 资源。
- **Cargo 增量构建缓存**：未变动的 crate、中间构件以及第三方依赖全部直接命中 `target/` 缓存，仅重新编译有代码变动的 crate，通常 5~15 秒即可快速产出最新程序。

---

## 场景三：如果要彻底发版到新版，标准发版流程应该是怎样

当前项目已全面收敛至 **GitHub Actions 一键打 Tag 自动化流水线**（配置位于 `.github/workflows/build.yml`），无需任何人工在本地繁琐地进行跨平台交叉编译或打包。

### 1. 发版流水线机制 (CI Trigger)
- **触发源**：`.github/workflows/build.yml` 监听 `push: tags: - "v*"`；
- **全自动构建矩阵**：
  1. `build-webui`：在 Ubuntu 环境下独立构建 WebUI SPA 并生成构件；
  2. 三端物理 Runner 并发编译 6 套目标架构：
     - **macOS**：`jeikcode-<tag>-darwin-arm64`（Apple Silicon）与 `jeikcode-<tag>-darwin-x64`（Intel）
     - **Linux**：`jeikcode-<tag>-linux-arm64` 与 `jeikcode-<tag>-linux-x64`（基于 zigbuild 的纯静态 musl，无 libc 依赖）
     - **Windows**：`jeikcode-<tag>-windows-arm64.exe` 与 `jeikcode-<tag>-windows-x64.exe`
  3. 通过 `action-gh-release` 自动创建 GitHub Release 并上传全套 6 平台二进制。

### 2. 标准发版执行闭环 (4 步标准操作)

以发布版本 **`v7.0.1`** 为例：

#### 第一步：元数据版本号同步
修改以下关键版本标识：
1. **`Cargo.toml`**：
   ```toml
   [workspace.package]
   version = "7.0.1"
   ```
2. **`Cargo.lock`**：
   运行 Cargo 检查命令，自动更新所有工作区 crate 的依赖锁定版本：
   ```bash
   cargo check --workspace
   ```
3. **技术文档徽章**：
   更新 `README.md`、`README.zh-CN.md`、`README.en.md` 中的 `version-7.0.1` 与 `Releases-v7.0.1` 徽章。
4. **一键安装脚本默认目标**：
   - `scripts/install.ps1`：`$DefaultVersion = "v7.0.1"`
   - `scripts/install.sh`：`DEFAULT_VERSION="v7.0.1"`
5. **客户端自更新清单**：
   - `latest.json`：`"version": "v7.0.1"`, `"released_at": "2026-09-19"`。

#### 第二步：提交发布 Commit 并推送至 `main`
```bash
git add Cargo.toml Cargo.lock README*.md scripts/install.* latest.json
git commit -m "release: v7.0.1 - 升级 workspace 版本与发版元数据

- 升级 workspace 整体版本至 v7.0.1
- 同步更新 Cargo.lock 所有 crate 依赖版本至 7.0.1
- 更新 latest.json 清单与全套中英文技术文档版本徽章
- 更新安装脚本默认下载与解析目标为 7.0.1

Co-Authored-By: JeikCode <331041501+JeikCode@users.noreply.github.com>"

git checkout main
git merge local-dev
git push origin main
```

#### 第三步：打 Tag 并推送触发流水线
```bash
git tag v7.0.1
git push origin v7.0.1
```
> 推送后可在 `https://github.com/JeikCode/JeikCode/actions` 查看实时矩阵构建进度，约 5~8 分钟后 GitHub Releases 页面自动发布完成。

#### 第四步：补充制品真实 SHA256 校验 (收尾)
流水线构建完成后，执行内置脚本抓取正式制品的 SHA256 与文件大小写入 `latest.json`，并推送至 `main`：
```bash
bash scripts/release-self-update.sh v7.0.1 JeikCode/JeikCode
git add latest.json
git commit -m "fix(release): 补充 v7.0.1 官方发布制品 sha256 校验清单

Co-Authored-By: JeikCode <331041501+JeikCode@users.noreply.github.com>"
git push origin main
```

---

## 附：用户端官方安装一键命令 (参考)

- **Linux / macOS**：
  ```bash
  curl -fsSL https://raw.githubusercontent.com/JeikCode/JeikCode/main/scripts/install.sh | bash
  ```
- **Windows (PowerShell)**：
  ```powershell
  irm https://raw.githubusercontent.com/JeikCode/JeikCode/main/scripts/install.ps1 | iex
  ```
