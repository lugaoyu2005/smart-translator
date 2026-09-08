#!/usr/bin/env node
/**
 * 系统测试：验证整条构建链路与全部自动化测试套件能否作为一个整体工作。
 * 步骤：类型检查 → 前端构建 → Rust编译检查 → Rust全套测试 → 前端全套测试
 * 任何一步失败即失败（非零退出）。
 * 运行：npm run test:system
 */
import { spawnSync } from "node:child_process";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = dirname(dirname(fileURLToPath(import.meta.url)));
const steps = [
  { name: "TypeScript 类型检查 (tsc --noEmit)", cmd: "npx", args: ["tsc", "--noEmit"], cwd: root },
  { name: "前端生产构建 (vite build)", cmd: "npx", args: ["vite", "build"], cwd: root },
  { name: "Rust 编译检查 (cargo check)", cmd: "cargo", args: ["check"], cwd: join(root, "src-tauri") },
  { name: "Rust 全套测试", cmd: "cargo", args: ["test"], cwd: join(root, "src-tauri") },
  { name: "前端全套测试", cmd: "npx", args: ["vitest", "run"], cwd: root },
];

let failed = 0;
for (const s of steps) {
  process.stdout.write(`▶ ${s.name} … `);
  const t0 = Date.now();
  const r = spawnSync(s.cmd, s.args, { cwd: s.cwd, shell: true, stdio: ["pipe", "pipe", "pipe"] });
  const sec = ((Date.now() - t0) / 1000).toFixed(1);
  if (r.status === 0) {
    process.stdout.write(`✅（${sec}s）\n`);
  } else {
    failed++;
    process.stdout.write(`❌（${sec}s）\n`);
    if (r.stdout?.length) console.error(r.stdout.toString().split("\n").slice(-25).join("\n"));
    if (r.stderr?.length) console.error(r.stderr.toString().split("\n").slice(-25).join("\n"));
  }
}

if (failed > 0) {
  console.error(`\n❌ 系统测试：${failed}/${steps.length} 步失败`);
  process.exit(1);
}
console.log(`\n✅ 系统测试：${steps.length}/${steps.length} 步全部通过（构建链路完整可用）`);
