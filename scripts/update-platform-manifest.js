#!/usr/bin/env node
/**
 * 单平台构建后即时元数据与版本同步器 (Per-Platform Manifest Updater)
 *
 * 当单个平台（如 Windows、Linux 或 macOS）构建完成后，立即计算其真实 SHA256 与文件大小，
 * 合并更新到 latest.json 中，同步更新版本号，并安全推送到 main 分支。
 * 用户无需等待其他平台构建完毕，即可立刻升级该平台！
 */
const fs = require('fs');
const path = require('path');
const crypto = require('crypto');
const { execSync } = require('child_process');

const tag = process.env.TAG_NAME || process.argv[2];
const platformArgs = process.argv.slice(3);

if (!tag || platformArgs.length === 0) {
  console.error('Usage: node scripts/update-platform-manifest.js <tag> <target:filepath> [target:filepath ...]');
  process.exit(1);
}

const cleanVersion = tag.replace(/^v/, '');
console.log(`🚀 Updating platform manifest for tag ${tag} (${cleanVersion})...`);

function hashFile(filePath) {
  if (!fs.existsSync(filePath)) {
    throw new Error(`File not found: ${filePath}`);
  }
  const stat = fs.statSync(filePath);
  const buffer = fs.readFileSync(filePath);
  const sha256 = crypto.createHash('sha256').update(buffer).digest('hex');
  return { sha256, size: stat.size };
}

// 1. 读取或初始化 latest.json
let manifest = {
  version: tag,
  released_at: new Date().toISOString().slice(0, 10),
  binaries: {}
};

if (fs.existsSync('latest.json')) {
  try {
    const existing = JSON.parse(fs.readFileSync('latest.json', 'utf8'));
    // 继承已有平台的二进制信息（如果是同一个主版本号）
    if (existing.binaries && typeof existing.binaries === 'object') {
      manifest.binaries = { ...existing.binaries };
    }
  } catch (err) {
    console.warn('⚠️ Could not parse existing latest.json, creating fresh:', err.message);
  }
}

manifest.version = tag;
manifest.released_at = new Date().toISOString().slice(0, 10);

const updatedTargets = [];
for (const arg of platformArgs) {
  const [target, filePath] = arg.split(':');
  if (!target || !filePath) {
    console.warn(`⚠️ Skipping invalid argument: ${arg}`);
    continue;
  }
  console.log(`   Computing hash for ${target} (${filePath})...`);
  const info = hashFile(filePath);
  manifest.binaries[target] = info;
  updatedTargets.push(target);
  console.log(`   -> ${target}: size=${info.size}, sha256=${info.sha256}`);
}

fs.writeFileSync('latest.json', JSON.stringify(manifest, null, 2) + '\n');
console.log(`✅ latest.json updated with platform(s): ${updatedTargets.join(', ')}`);

// 2. 自动同步 Cargo.toml
if (fs.existsSync('Cargo.toml')) {
  let c = fs.readFileSync('Cargo.toml', 'utf8');
  c = c.replace(/^version = ".*"/m, `version = "${cleanVersion}"`);
  fs.writeFileSync('Cargo.toml', c);
  console.log(`   Cargo.toml version updated to ${cleanVersion}`);
}

// 3. 自动同步 scripts/install.ps1 与 scripts/install.sh
if (fs.existsSync('scripts/install.ps1')) {
  let c = fs.readFileSync('scripts/install.ps1', 'utf8');
  c = c.replace(/\$DefaultVersion = ".*"/, `$DefaultVersion = "${tag}"`);
  fs.writeFileSync('scripts/install.ps1', c);
  console.log(`   scripts/install.ps1 DefaultVersion updated to ${tag}`);
}
if (fs.existsSync('scripts/install.sh')) {
  let c = fs.readFileSync('scripts/install.sh', 'utf8');
  c = c.replace(/DEFAULT_VERSION=".*"/, `DEFAULT_VERSION="${tag}"`);
  fs.writeFileSync('scripts/install.sh', c);
  console.log(`   scripts/install.sh DEFAULT_VERSION updated to ${tag}`);
}

// 4. 自动同步 README*.md 徽章
for (const file of ['README.md', 'README.zh-CN.md', 'README.en.md']) {
  if (fs.existsSync(file)) {
    let c = fs.readFileSync(file, 'utf8');
    c = c.replace(/badge\/version-[0-9a-zA-Z.-]+-blue\.svg/g, `badge/version-${cleanVersion}-blue.svg`);
    c = c.replace(/badge\/Releases-v?[0-9a-zA-Z.-]+-00f2fe/g, `badge/Releases-${tag}-00f2fe`);
    fs.writeFileSync(file, c);
    console.log(`   ${file} version badges updated.`);
  }
}

// 5. 安全并发推送到 main 分支（带 rebase 重试，防止多平台并发冲突）
try {
  console.log(`🔄 Committing and pushing ${updatedTargets.join(', ')} metadata to main...`);
  execSync('git config --global user.name "github-actions[bot]"', { stdio: 'ignore' });
  execSync('git config --global user.email "github-actions[bot]@users.noreply.github.com"', { stdio: 'ignore' });

  const commitMsg = `chore(release): 自动同步 ${tag} [${updatedTargets.join(', ')}] 校验清单 [skip ci]`;
  let pushed = false;
  for (let attempt = 1; attempt <= 6; attempt++) {
    try {
      execSync('git pull --rebase origin main', { stdio: 'inherit' });
      execSync('git add latest.json Cargo.toml README*.md scripts/install.*', { stdio: 'inherit' });
      const status = execSync('git status --porcelain', { encoding: 'utf8' });
      if (!status.trim()) {
        console.log('   No changes to commit.');
        pushed = true;
        break;
      }
      execSync(`git commit -m "${commitMsg}"`, { stdio: 'inherit' });
      execSync('git push origin main', { stdio: 'inherit' });
      pushed = true;
      console.log(`✅ Successfully pushed ${updatedTargets.join(', ')} release metadata to main!`);
      break;
    } catch (pushErr) {
      console.warn(`⚠️ Push attempt ${attempt} failed, retrying in 3s...`);
      execSync('sleep 3', { stdio: 'ignore' });
    }
  }
  if (!pushed) {
    console.error('❌ Failed to push metadata to main after 6 attempts.');
  }
} catch (e) {
  console.error('⚠️ Non-fatal error during git push:', e.message);
}
