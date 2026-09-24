import assert from 'node:assert/strict';
import { test } from 'node:test';
import { markdownToHtml } from './markdownRender.ts';

test('GitHub alerts become classed blockquotes', () => {
  const note = markdownToHtml('> [!NOTE]\n> hello');
  assert.match(note, /md-alert md-alert-note/);
  assert.match(note, /md-alert-title/);
  assert.match(note, /hello/);

  const warn = markdownToHtml('> [!WARNING] be careful');
  assert.match(warn, /md-alert-warning/);
  assert.match(warn, /be careful/);
});

test('code fence language sentinel is stripped and generic lang is hidden', () => {
  const out = markdownToHtml('```text\ntext\nBase URL: http://127.0.0.1:8045\n```');
  assert.match(out, /Base URL: http:\/\/127\.0\.0\.1:8045/);
  assert.doesNotMatch(out, /<code[^>]*>text\nBase URL/);
  assert.doesNotMatch(out, /<span class="code-block-lang">text<\/span>/);
});

test('named language gets a toolbar label', () => {
  const out = markdownToHtml('```ts\nconst n = 1;\n```');
  assert.match(out, /<span class="code-block-lang">ts<\/span>/);
  assert.match(out, /const n = 1;/);
});

test('two indented ```text fences in one numbered list both render', () => {
  const content = [
    '已通过并发工具调用同时并行执行了两个 Hello World 任务：',
    '',
    '1. **任务 1** 输出：',
    '   ```text',
    '   Hello World 1',
    '   ```',
    '',
    '2. **任务 2** 输出：',
    '   ```text',
    '   Hello World 2',
    '   ```',
    '',
    '两个任务已在单轮中并行派发并成功完成。',
  ].join('\n');
  const out = markdownToHtml(content);
  assert.match(out, /Hello World 1/);
  assert.match(out, /Hello World 2/);
  assert.equal((out.match(/code-block-wrapper/g) ?? []).length, 2);
  assert.doesNotMatch(out, /<p>[^<]*```/);
});

test('code fence inside list ending with backslash closes properly without swallowing following text', () => {
  const content = [
    '- **开发与构建目录**：',
    '  ```text',
    '  E:\\code\\jeikcode\\target\\',
    '  E:\\code\\Antigravity-Manager\\src-tauri\\target\\',
    '  %USERPROFILE%\\.cargo\\bin\\',
    '  ```',
    '- **进程白名单**：',
    '  - `jeikcode.exe`',
    '  - `atomcode.exe`',
    '',
    '#### 2. Linux 服务器环境',
    '',
    '- **二进制目录与文件**：',
    '  ```text',
    '  /usr/local/bin/jeikcode',
    '  /usr/local/bin/atomcode',
    '  ```',
  ].join('\n');
  const out = markdownToHtml(content);
  assert.match(out, /<strong>进程白名单<\/strong>/);
  assert.match(out, /<h4[^>]*>2\. Linux 服务器环境<\/h4>/);
  assert.match(out, /<strong>二进制目录与文件<\/strong>/);
  assert.equal((out.match(/code-block-wrapper/g) ?? []).length, 2);
});

test('structural fence auto-close when closing fence is omitted before heading', () => {
  // 模型输出了 ```rust 代码块，但是漏写了闭合的 ```，直接写了 #### 3. 标题
  const brokenMd = [
    '2. grok-build 分析',
    '* 底层实现：',
    '```rust',
    'cmd.arg("-e").arg(&input.pattern);',
    // 注意：这里漏掉了闭合 ```
    '* 同样没有传 `-F`，没有做任何符号转义保护。',
    '',
    '#### 3. 相比之下，`jeikcode` 的防护其实已经领先了一步：',
    '* `jeikcode` 写的智能回退逻辑：',
    '```rust',
    'let matcher = 1;',
    '```',
  ].join('\n');

  const out = markdownToHtml(brokenMd);
  // 结构性自愈生效：标题必须独立渲染为 h4，而不是被吞进代码块
  assert.match(out, /<h4[^>]*>3\. 相比之下/);
  // 必须有两个独立的代码块，而不是融为一个大块
  assert.equal((out.match(/code-block-wrapper/g) ?? []).length, 2);
});

test('indented code blocks with nested markdown are unpacked and rendered as rich text', () => {
  const md = [
    '    * 同样没有传 `-F`，没有符号保护。',
    '    ',
    '    #### 标题内容',
    '    * 列表条目',
  ].join('\n');
  const out = markdownToHtml(md);
  assert.match(out, /<h4[^>]*>标题内容<\/h4>/);
  assert.match(out, /<li>列表条目<\/li>/);
});
