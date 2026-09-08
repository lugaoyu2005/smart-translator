#!/usr/bin/env node
// 监视器：文件夹更新即自动测试（watch mode）。
//
// - src-tauri 下的 .rs 文件或 Cargo.toml 变更 → cargo test（Rust 全套）
// - src 下的 .ts/.tsx 文件变更              → vitest run（前端全套）
// - 两者都变 → 依次都跑
// - 自动跳过 target/ dist/ node_modules/ 的变更
// - 防抖：连续保存合并为一次触发；测试进行中的变更排队，跑完立即补跑
//
// 用法：
//   npm run test:watch            # 按变更类型跑对应套件
//   npm run test:watch -- --all   # 任何变更都跑全套（Rust+前端+健全性）
//   npm run test:watch -- --smoke # 任何变更只跑冒烟子集（最快反馈）
import { spawn } from "node:child_process";
import { watch } from "node:fs";
import { dirname, join, relative, sep } from "node:path";
import { fileURLToPath } from "node:url";

const root = dirname(dirname(fileURLToPath(import.meta.url)));
const mode = process.argv[2] ?? ""; // "" | --all | --smoke
const DEBOUNCE_MS = 600;

const WATCH_TARGETS = [
  { path: join(root, "src"), kind: "fe" },
  { path: join(root, "src-tauri", "src"), kind: "rust" },
  { path: join(root, "src-tauri", "Cargo.toml"), kind: "rust" },
];

const SKIP = ["node_modules", `${sep}target${sep}`, `${sep}dist${sep}`, "mt-models", ".test-tmp"];

let running = null; // 当前执行的子进程
let pending = new Set(); // 待补跑的测试类型
let debounceTimer = null;
let dirty = new Set(); // 本次防抖窗口内累积的变更类型

const cyan = (s) => `\x1b[36m${s}\x1b[0m`;
const green = (s) => `\x1b[32m${s}\x1b[0m`;
const red = (s) => `\x1b[31m${s}\x1b[0m`;
const dim = (s) => `\x1b[2m${s}\x1b[0m`;

function now() {
  return new Date().toLocaleTimeString("zh-CN", { hour12: false });
}

function run(name, cmd, args, cwd) {
  return new Promise((resolve) => {
    const t0 = Date.now();
    console.log(cyan(`\n▶ [${now()}] ${name}`) + dim(`  (${cmd} ${args.join(" ")})`));
    const child = spawn(cmd, args, { cwd, shell: true, stdio: "inherit" });
    child.on("close", (code) => {
      const sec = ((Date.now() - t0) / 1000).toFixed(1);
      console.log(
        code === 0
          ? green(`✔ ${name} 通过（${sec}s）`)
          : red(`✘ ${name} 失败（${sec}s，退出码 ${code}）`)
      );
      resolve(code === 0);
    });
  });
}

const RUNNERS = {
  rust: () =>
    mode === "--smoke"
      ? run("Rust 冒烟", "cargo", ["test", "smoke_", "acceptance_"], join(root, "src-tauri"))
      : run("Rust 测试", "cargo", ["test"], join(root, "src-tauri")),
  fe: () =>
    mode === "--smoke"
      ? run("前端冒烟", "npx", ["vitest", "run", "--testNamePattern=Smoke"], root)
      : run("前端测试", "npx", ["vitest", "run"], root),
  sanity: () => run("健全性检查", "node", ["scripts/sanity.mjs"], root),
};

async function execute(kinds) {
  const order = ["fe", "rust", "sanity"];
  const todo = order.filter((k) => kinds.has(k));
  let allOk = true;
  for (const k of todo) {
    allOk = (await RUNNERS[k]()) && allOk;
  }
  const stamp = allOk ? green("全部通过 ✅") : red("存在失败 ❌");
  console.log(cyan(`\n══ ${stamp} ══ ${dim(`等待文件变更… (--all=${mode === "--all"})`)}`));
}

function schedule(kinds) {
  for (const k of kinds) dirty.add(k);
  clearTimeout(debounceTimer);
  debounceTimer = setTimeout(() => {
    if (running) {
      for (const k of dirty) pending.add(k);
      dirty = new Set();
      return;
    }
    const batch = dirty;
    dirty = new Set();
    running = (async () => {
      await execute(batch);
      running = null;
      if (pending.size > 0) {
        const next = pending;
        pending = new Set();
        schedule(next);
      }
    })();
  }, DEBOUNCE_MS);
}

function classify(fullPath) {
  if (SKIP.some((s) => fullPath.includes(s))) return new Set();
  if (mode === "--all") return new Set(["fe", "rust"]);
  if (fullPath.includes(`${sep}src-tauri${sep}`)) return new Set(["rust"]);
  return new Set(["fe"]);
}

console.log(cyan("🔍 监视器已启动（文件夹更新即测试）"));
console.log(dim(`   监视: ${WATCH_TARGETS.map((t) => relative(root, t.path)).join(", ")}`));
console.log(dim(`   模式: ${mode || "按变更类型"} | 防抖 ${DEBOUNCE_MS}ms | Ctrl+C 退出`));

// 启动时先跑一轮健全性检查，确认环境可用
schedule(new Set(["sanity"]));

for (const target of WATCH_TARGETS) {
  try {
    watch(target.path, { recursive: true }, (_event, filename) => {
      if (!filename) return;
      const full = join(target.path, filename);
      const kinds = mode === "--all" ? new Set(["fe", "rust"]) : classify(full);
      if (kinds.size === 0) return;
      console.log(dim(`  · 变更: ${relative(root, full)}`));
      schedule(kinds);
    });
  } catch (e) {
    console.error(red(`无法监视 ${target.path}: ${e.message}`));
    process.exit(1);
  }
}
