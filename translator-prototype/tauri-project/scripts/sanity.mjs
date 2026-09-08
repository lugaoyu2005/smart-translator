#!/usr/bin/env node
/**
 * 健全性测试（Sanity）：环境与项目结构自检。
 * 不测业务逻辑，只回答一个问题："这台机器/这份代码处于可测试、可构建的状态吗？"
 * 运行：npm run test:sanity
 */
import { execSync } from "node:child_process";
import { existsSync, readFileSync, statSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const root = dirname(dirname(fileURLToPath(import.meta.url)));
const results = [];
let failed = 0;

function check(name, fn) {
  try {
    fn();
    results.push(`  ✅ ${name}`);
  } catch (e) {
    failed++;
    results.push(`  ❌ ${name}\n     → ${e.message}`);
  }
}

function requireTool(cmd, minLen = 3) {
  const out = execSync(cmd, { encoding: "utf8", stdio: ["pipe", "pipe", "pipe"] }).trim();
  if (out.length < minLen) throw new Error(`${cmd} 输出异常: ${out}`);
}

console.log(`健全性检查（${root}）\n`);

check("Node 可执行", () => requireTool("node --version"));
check("npm 可执行", () => requireTool("npm --version"));
check("cargo 可执行", () => requireTool("cargo --version"));

check("前端依赖已安装（node_modules）", () => {
  if (!existsSync(join(root, "node_modules", "vitest")))
    throw new Error("缺 vitest，先运行 npm install");
  if (!existsSync(join(root, "node_modules", "@vitejs")))
    throw new Error("缺 @vitejs/plugin-react，先运行 npm install");
});

check("关键源文件存在", () => {
  const files = [
    "src/App.tsx",
    "src/main.tsx",
    "src/components/TranslatePanel.tsx",
    "src/components/SettingsPanel.tsx",
    "src/components/HistoryPanel.tsx",
    "src/components/TermsPanel.tsx",
    "src-tauri/src/main.rs",
    "src-tauri/src/engines.rs",
    "src-tauri/src/translation.rs",
    "src-tauri/src/terms.rs",
    "src-tauri/Cargo.toml",
    "src-tauri/tauri.conf.json",
  ];
  for (const f of files) {
    if (!existsSync(join(root, f))) throw new Error(`缺失: ${f}`);
  }
});

check("tauri.conf.json 是合法JSON且含窗口配置", () => {
  const conf = JSON.parse(readFileSync(join(root, "src-tauri", "tauri.conf.json"), "utf8"));
  if (!conf.app && !conf.build) throw new Error("配置缺少 app/build 段");
});

check("package.json 测试脚本齐备", () => {
  const pkg = JSON.parse(readFileSync(join(root, "package.json"), "utf8"));
  for (const s of ["test", "test:rust", "test:fe", "test:watch", "test:smoke"]) {
    if (!pkg.scripts?.[s]) throw new Error(`缺 script: ${s}`);
  }
});

check("dist 构建产物不是陈旧空目录", () => {
  const d = join(root, "dist");
  if (!existsSync(d)) return; // 从未构建过不算失败
  const s = statSync(join(root, "dist", "index.html"));
  if (s.size < 100) throw new Error("dist/index.html 异常地小");
});

console.log(results.join("\n"));
if (failed > 0) {
  console.log(`\n❌ ${failed} 项不健全`);
  process.exit(1);
}
console.log("\n✅ 健全性检查全部通过");
