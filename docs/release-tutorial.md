# JeikCode 自动化发版与 Release 指南

> **核心原则**：当前项目已全面收敛归一至 **GitHub Actions 一键打 Tag 自动化流水线**（定义于 `.github/workflows/build.yml`）。
> 官方代码仓为 **`JeikCode/JeikCode`**，主干发版与在线安装严格基于 **`main`** 分支（`local-dev` 仅为历史特性兼容分支）。
> 禁止且无需在本地机器手动交叉编译二进制。

---

## 1. 发版流水线触发机制

项目内置了完整的全平台自动构建流水线，触发条件为：
```yaml
on:
  push:
    tags:
      - "v*"
```

当向 GitHub 远程仓库推送任意 `v*` 格式的 Git Tag（如 `v7.0.1`）时，GitHub Actions 会自动启动以下矩阵作业：
1. **前端预构建 (`build-webui`)**：
   - 检出源码，在 `webui` 目录下执行 `npm run build`；
   - 生成完整静态资源包并上传为 `webui-dist` 构件，作为后续各物理端编译时的内嵌依赖。
2. **三端 6 平台矩阵编译**：
   - **macOS Runner**：编译 `darwin-arm64`（Apple Silicon）与 `darwin-x64`（Intel）；
   - **Linux Runner**：使用 `cargo-zigbuild` 编译纯静态 musl 二进制（`linux-arm64` 与 `linux-x64`），零动态 glibc 依赖，兼容所有 Linux 发行版与容器；
   - **Windows Runner**：编译 `windows-arm64.exe` 与 `windows-x64.exe`。
3. **自动发布 GitHub Release**：
   - 使用 `softprops/action-gh-release@v2` 自动创建 Release 并上传 6 平台制品；
   - 资产包命名规范：`jeikcode-<tag>-<platform>[.exe]`（例如 `jeikcode-v7.0.1-linux-x64`、`jeikcode-v7.0.1-windows-x64.exe`）。

---

## 2. 标准发版执行步骤 (Agent / 开发者通用)

任何 Agent 或维护者发版时，严格按照以下 4 个阶段执行：

### 阶段一：更新版本元数据

修改以下关键文件的版本号（以升级至 `v7.0.1` 为例）：

1. **`Cargo.toml`**：
   ```toml
   [workspace.package]
   version = "7.0.1"
   ```
2. **`Cargo.lock`**：
   运行编译检查，触发 Cargo 自动更新工作区所有 12 个 crate 的依赖版本：
   ```bash
   cargo check --workspace
   ```
3. **技术文档版本徽章**：
   更新 `README.md`、`README.zh-CN.md`、`README.en.md` 中的徽章版本：
   ```markdown
   <img src="https://img.shields.io/badge/version-7.0.1-blue.svg" alt="version">
   <img src="https://img.shields.io/badge/Releases-v7.0.1-00f2fe?style=for-the-badge&logo=github&logoColor=black" alt="Releases" />
   ```
4. **一键安装脚本默认版本**：
   - `scripts/install.ps1`：`$DefaultVersion = "v7.0.1"`
   - `scripts/install.sh`：`DEFAULT_VERSION="v7.0.1"`
5. **客户端升级清单**：
   - `latest.json`：更新 `"version": "v7.0.1"`，`"released_at"` 设为当天日期。

---

### 阶段二：提交发版 Commit 并合入 `main`

按照规范创建发版提交，合并至主干 `main`：

```bash
# 1. 提交发版改动（必须严格遵循 Conventional Commits 与强制共同署名）
git add Cargo.toml Cargo.lock README*.md scripts/install.* latest.json
git commit -m "release: v7.0.1 - 升级 workspace 版本与发版元数据

- 升级 workspace 整体版本至 v7.0.1
- 同步更新 Cargo.lock 所有 crate 依赖版本至 7.0.1
- 更新 latest.json 清单与全套中英文技术文档版本徽章
- 更新安装脚本默认下载与解析目标为 7.0.1

Co-Authored-By: JeikCode <331041501+JeikCode@users.noreply.github.com>"

# 2. 合入主干 main 分支并推送
git checkout main
git merge local-dev
git push origin main
```

---

### 阶段三：打 Tag 推送，触发流水线

从 `main` 上的发布提交打上版本 Tag 并推送至 GitHub：

```bash
# 创建 Tag 并推送到远程
git tag v7.0.1
git push origin v7.0.1
```

> **流水线观察**：访问 `https://github.com/JeikCode/JeikCode/actions` 观察进度，通常耗时 5~8 分钟完成全部 6 个平台的并行构建与归档。

---

### 阶段四：发版后补充 SHA256 校验清单 (推荐)

流水线构建完成并在 Releases 页面生成资产后，可使用项目内置工具自动拉取真实产物的校验值并更新 `latest.json`：

```bash
# 自动抓取 Releases 页面资产的真实 sha256 与文件大小并写入 latest.json
bash scripts/release-self-update.sh v7.0.1 JeikCode/JeikCode

# 提交清单并推送至 main 分支
git add latest.json
git commit -m "fix(release): 补充 v7.0.1 官方发布制品 sha256 校验清单

Co-Authored-By: JeikCode <331041501+JeikCode@users.noreply.github.com>"
git push origin main
```

至此，新版本发布全部闭环，全网所有新老用户即可通过在线安装脚本或 `/upgrade` 立即升级。

---

## 3. 常见异常与排错指引

| 问题现象 | 可能原因 | 解决办法 |
| :--- | :--- | :--- |
| 推送 Tag 后 Actions 未触发 | Tag 命名未以 `v` 开头 | 流水线规则要求 `v*`，请使用 `v7.0.1` 格式而非 `7.0.1` |
| `build-webui` 报错 node/npm 异常 | `webui/package.json` 依赖损坏 | 本地先进入 `webui` 运行 `npm run build` 验证前端产物是否正常 |
| 某个物理 Runner 编译超时或断网 | GitHub Runner 偶发网络抖动 | 在 GitHub Actions 页面对应 failed job 点击 `Re-run failed jobs` 即可 |
| 发现严重 Bug 需要撤回发布 | 制品已公开需紧急止血 | 1. 登录 GitHub 网页编辑/删除该 Release；<br>2. 本地删除并推送删除 Tag：`git tag -d v7.0.1 && git push origin :refs/tags/v7.0.1` |
