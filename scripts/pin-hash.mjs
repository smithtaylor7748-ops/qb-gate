#!/usr/bin/env node
/**
 * 把一个已核对过的安装包的 SHA-256 写回 installers.lock.json。
 *
 *   node scripts/pin-hash.mjs <id> <本地文件路径>
 *
 * 用法前提：**你已经手动从官方地址下载并确认过这个文件**。
 * 这个脚本只负责算哈希并写回，它不能替你判断文件本身可不可信。
 *
 * 提交时请连同来源说明一起提交（从哪个官方页面、什么日期下载的），
 * 否则后来的人无从判断这个哈希凭什么可信。
 */

import { createHash } from 'node:crypto';
import { readFile, writeFile } from 'node:fs/promises';
import { resolve } from 'node:path';

const LOCKFILE = resolve(import.meta.dirname, '..', 'installers.lock.json');

const [id, filePath] = process.argv.slice(2);

if (!id || !filePath) {
  console.error('用法: node scripts/pin-hash.mjs <id> <本地文件路径>');
  console.error('例:   node scripts/pin-hash.mjs claude-code ./Claude-Setup.exe');
  process.exit(1);
}

const bytes = await readFile(filePath).catch((e) => {
  console.error(`读不到文件 ${filePath}: ${e.message}`);
  process.exit(1);
});

const sha256 = createHash('sha256').update(bytes).digest('hex');

const lock = JSON.parse(await readFile(LOCKFILE, 'utf8'));
const entry = lock.installers.find((i) => i.id === id);

if (!entry) {
  console.error(`installers.lock.json 里没有 id 为 "${id}" 的条目。`);
  console.error(`现有条目: ${lock.installers.map((i) => i.id).join(', ')}`);
  process.exit(1);
}

const previous = entry.sha256;
entry.sha256 = sha256;

await writeFile(LOCKFILE, JSON.stringify(lock, null, 2) + '\n');

console.log(`${entry.name} (${id})`);
console.log(`  文件   ${filePath}`);
console.log(`  大小   ${(bytes.length / 1024 / 1024).toFixed(1)} MB`);
console.log(`  SHA256 ${sha256}`);
if (previous && previous !== sha256) {
  console.log(`  注意   原值 ${previous} 已被覆盖 —— 确认这是有意的版本更新。`);
}
console.log('\n已写回 installers.lock.json。请在提交信息里注明下载来源与日期。');
