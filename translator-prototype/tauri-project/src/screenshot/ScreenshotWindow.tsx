import React, {
  useState,
  useEffect,
  useRef,
  useLayoutEffect,
  useCallback,
} from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { currentMonitor } from "@tauri-apps/api/window";
import { PhysicalSize, PhysicalPosition } from "@tauri-apps/api/dpi";
import { emit, listen } from "@tauri-apps/api/event";

// ============ 类型 ============

/** OCR识别的一行（已换算为窗口CSS坐标） */
interface OcrLineInfo {
  text: string;
  x: number;
  y: number;
  width: number;
  height: number;
}

/** 渲染块：段落组（一整块）或独立单行（紧凑块） */
interface Block {
  x: number;
  y: number;
  width: number;
  height: number;
  original: string; // 组内原文（\n连接）
  translation: string;
}

interface EngineInfo {
  name: string;
  engine_type: string;
  available: boolean;
  configured: boolean;
}

interface ShotSettings {
  screenshot_components: string[];
  overlay_bg_color: string;
  overlay_opacity: number;
  overlay_transparent: boolean;
  overlay_bg_fit_text: boolean;
}

const DEFAULT_SETTINGS: ShotSettings = {
  screenshot_components: ["engine", "copy", "close", "settings"],
  overlay_bg_color: "#ffffff",
  overlay_opacity: 0.95,
  overlay_transparent: false,
  overlay_bg_fit_text: false,
};

type Phase = "select" | "processing" | "result";
type CopyMode = "original" | "translated";

// ============ 工具函数 ============

/**
 * 行分组判定：
 * - 行间距 < 中位行高 且 水平方向有重叠 → 同组（一段长文本换行）
 * - 行间距 ≥ 中位行高 → 分组（独立单行）
 * - 同行（y重叠）但水平完全不相交 → 强制分组（后端拆出的左右并排文本，
 *   如表格两列/标签+值；否则垂直间距为负会被误合并）
 * 已知局限：多栏多行交错布局暂不做栏聚类
 */
function groupLines(lines: OcrLineInfo[]): OcrLineInfo[][] {
  const sorted = [...lines].sort((a, b) => a.y - b.y);
  if (sorted.length <= 1) return sorted.length ? [sorted] : [];

  const heights = sorted.map((l) => l.height).sort((a, b) => a - b);
  const medianH = heights[Math.floor(heights.length / 2)] || 1;

  const groups: OcrLineInfo[][] = [[sorted[0]]];
  for (let i = 1; i < sorted.length; i++) {
    const prev = sorted[i - 1];
    const cur = sorted[i];
    const vGap = cur.y - (prev.y + prev.height);
    const xOverlap =
      Math.min(prev.x + prev.width, cur.x + cur.width) - Math.max(prev.x, cur.x);
    if (vGap < medianH && xOverlap > 0) {
      groups[groups.length - 1].push(cur);
    } else {
      groups.push([cur]);
    }
  }
  return groups;
}

/** 多行组的并集矩形 */
function unionRect(lines: OcrLineInfo[]) {
  let minX = Infinity;
  let minY = Infinity;
  let maxX = -Infinity;
  let maxY = -Infinity;
  for (const l of lines) {
    minX = Math.min(minX, l.x);
    minY = Math.min(minY, l.y);
    maxX = Math.max(maxX, l.x + l.width);
    maxY = Math.max(maxY, l.y + l.height);
  }
  return { x: minX, y: minY, width: maxX - minX, height: maxY - minY };
}

/** #RRGGBB → rgba字符串 */
function hexToRgba(hex: string, alpha: number): string {
  try {
    const m = hex.replace("#", "");
    const full =
      m.length === 3
        ? m
            .split("")
            .map((c) => c + c)
            .join("")
        : m;
    const num = parseInt(full, 16);
    if (Number.isNaN(num)) return `rgba(255,255,255,${alpha})`;
    const r = (num >> 16) & 255;
    const g = (num >> 8) & 255;
    const b = num & 255;
    return `rgba(${r},${g},${b},${alpha})`;
  } catch {
    return `rgba(255,255,255,${alpha})`;
  }
}

/** 字号测量canvas（模块级复用实例） */
let measureCanvas: HTMLCanvasElement | null = null;

/**
 * 计算文本能完整放入色块的最大字号：
 * 用canvas measureText真实测量字符宽度（中英文/换行位置全部精确），
 * 模拟按块宽换行，二分查找总高度≤块高的最大字号——固定系数估算对
 * 中英混排和小色块必然失准，此法从根源上消除文字溢出。
 */
function fitFontSize(text: string, blockW: number, blockH: number): number {
  const t = (text || "").trim();
  if (!t || blockW <= 14 || blockH <= 10) return 14;

  if (!measureCanvas) measureCanvas = document.createElement("canvas");
  const ctx = measureCanvas.getContext("2d");
  if (!ctx) return 14;

  const family = `"Segoe UI", "Microsoft YaHei", sans-serif`;
  const usableW = Math.max(4, blockW - 12); // 减去左右padding
  const usableH = Math.max(4, blockH - 8); // 减去上下padding
  const paras = t.split("\n");
  const lh = 1.3; // 与CSS line-height保持一致

  /** 模拟按宽度逐字符换行后的总行数 */
  const wrappedLineCount = (fs: number): number => {
    ctx.font = `${fs}px ${family}`;
    let total = 0;
    for (const raw of paras) {
      if (!raw) {
        total += 1;
        continue;
      }
      let cur = "";
      let n = 1;
      for (const ch of raw) {
        const test = cur + ch;
        if (cur && ctx.measureText(test).width > usableW) {
          n += 1;
          cur = ch;
        } else {
          cur = test;
        }
      }
      total += n;
    }
    return total;
  };

  let lo = 10; // 字号下限：过小不可读（用户反馈：下限再大一点）
  let hi = 48;
  let best = 10;
  while (lo <= hi) {
    const mid = Math.floor((lo + hi) / 2);
    if (wrappedLineCount(mid) * mid * lh <= usableH) {
      best = mid;
      lo = mid + 1;
    } else {
      hi = mid - 1;
    }
  }
  return best;
}

// ============ 组件 ============

const ScreenshotWindow: React.FC = () => {
  const win = getCurrentWebviewWindow();

  const [phase, setPhase] = useState<Phase>("select");
  // 处理阶段反馈：当前子阶段 + 已耗时（秒）
  const [procStage, setProcStage] = useState<"ocr" | "translate">("ocr");
  const [procSeconds, setProcSeconds] = useState(0);
  const [dragging, setDragging] = useState(false);
  const [selection, setSelection] = useState({ x: 0, y: 0, width: 0, height: 0 });
  const [blocks, setBlocks] = useState<Block[]>([]);
  const [overlayVisible, setOverlayVisible] = useState(true);
  const [noText, setNoText] = useState(false);
  const [error, setError] = useState("");
  const [retranslating, setRetranslating] = useState(false);

  const [engines, setEngines] = useState<EngineInfo[]>([]);
  const [engineIdx, setEngineIdx] = useState(0);
  const [engineMenuOpen, setEngineMenuOpen] = useState(false);
  const [settings, setSettings] = useState<ShotSettings>(DEFAULT_SETTINGS);
  const [copyMode, setCopyMode] = useState<CopyMode>("original");
  // Ctrl+拖动 / Ctrl+双击累积的文字片段（多段，复制时按操作顺序合并）；
  // block=来源块索引（null=从块外起拖），whole=双击整段拾取（可再次Ctrl+双击取消），
  // 已选块描边标记提供可见反馈
  const [picked, setPicked] = useState<
    { text: string; block: number | null; whole: boolean }[]
  >([]);
  const [ctrlHeld, setCtrlHeld] = useState(false);
  // 本次按下的拖拽是否带Ctrl（mousedown时快照，避免先松Ctrl再松鼠标导致误清空）
  const dragWasCtrl = useRef(false);
  // 拖选起始块索引（null=块外起拖）
  const dragStartBlock = useRef<number | null>(null);

  // 按键组（菜单组）
  const [groupPos, setGroupPos] = useState({ x: -9999, y: -9999 });
  const [groupDragging, setGroupDragging] = useState(false);
  const groupDragged = useRef(false);
  const dragOffset = useRef({ x: 0, y: 0 });
  const groupRef = useRef<HTMLDivElement>(null);

  const startPoint = useRef({ x: 0, y: 0 });
  const scaleFactor = useRef(1);
  const monitorPos = useRef({ x: 0, y: 0 });

  // 启动：全屏化窗口 + 加载设置与引擎
  useEffect(() => {
    (async () => {
      try {
        const monitor = await currentMonitor();
        if (monitor) {
          scaleFactor.current = monitor.scaleFactor || 1;
          monitorPos.current = { x: monitor.position.x, y: monitor.position.y };
          await win.setSize(
            new PhysicalSize(monitor.size.width, monitor.size.height)
          );
          await win.setPosition(
            new PhysicalPosition(monitor.position.x, monitor.position.y)
          );
        }
      } catch (e) {
        console.error("初始化截图窗口失败:", e);
      }

      try {
        const s = await invoke<any>("get_app_settings");
        setSettings({
          screenshot_components:
            s?.screenshot_components ?? DEFAULT_SETTINGS.screenshot_components,
          overlay_bg_color: s?.overlay_bg_color ?? DEFAULT_SETTINGS.overlay_bg_color,
          overlay_opacity: s?.overlay_opacity ?? DEFAULT_SETTINGS.overlay_opacity,
          overlay_transparent:
            s?.overlay_transparent ?? DEFAULT_SETTINGS.overlay_transparent,
          overlay_bg_fit_text:
            s?.overlay_bg_fit_text ?? DEFAULT_SETTINGS.overlay_bg_fit_text,
        });
      } catch {}

      try {
        const es = await invoke<EngineInfo[]>("list_engines");
        setEngines(es.filter((e) => e.configured));
      } catch {}
    })();
  }, [win]);

  // 设置保存后实时生效
  useEffect(() => {
    const un = listen("screenshot-settings-updated", async () => {
      try {
        const s = await invoke<any>("get_app_settings");
        setSettings({
          screenshot_components:
            s?.screenshot_components ?? DEFAULT_SETTINGS.screenshot_components,
          overlay_bg_color: s?.overlay_bg_color ?? DEFAULT_SETTINGS.overlay_bg_color,
          overlay_opacity: s?.overlay_opacity ?? DEFAULT_SETTINGS.overlay_opacity,
          overlay_transparent:
            s?.overlay_transparent ?? DEFAULT_SETTINGS.overlay_transparent,
          overlay_bg_fit_text:
            s?.overlay_bg_fit_text ?? DEFAULT_SETTINGS.overlay_bg_fit_text,
        });
      } catch {}
      try {
        const es = await invoke<EngineInfo[]>("list_engines");
        const configured = es.filter((e) => e.configured);
        setEngines(configured);
        setEngineIdx((i) =>
          configured.length ? Math.min(i, configured.length - 1) : 0
        );
      } catch {}
    });
    return () => {
      un.then((f) => f());
    };
  }, []);

  // 从设置页返回截图窗口时恢复置顶
  useEffect(() => {
    const un = win.onFocusChanged(({ payload: focused }) => {
      if (focused) {
        win.setAlwaysOnTop(true).catch(() => {});
      }
    });
    return () => {
      un.then((f) => f());
    };
  }, [win]);

  // Ctrl按住状态（多选拖选模式）
  useEffect(() => {
    const down = (e: KeyboardEvent) => {
      if (e.key === "Control") setCtrlHeld(true);
    };
    const up = (e: KeyboardEvent) => {
      if (e.key === "Control") setCtrlHeld(false);
    };
    const blur = () => setCtrlHeld(false);
    window.addEventListener("keydown", down);
    window.addEventListener("keyup", up);
    window.addEventListener("blur", blur);
    return () => {
      window.removeEventListener("keydown", down);
      window.removeEventListener("keyup", up);
      window.removeEventListener("blur", blur);
    };
  }, []);

  // 处理中计时器：每秒刷新耗时显示
  useEffect(() => {
    if (phase !== "processing") return;
    setProcSeconds(0);
    const t = setInterval(() => setProcSeconds((s) => s + 1), 1000);
    return () => clearInterval(t);
  }, [phase]);

  /** 重置到框选模式（退出/重新触发共用） */
  const resetState = useCallback(() => {
    setPhase("select");
    setDragging(false);
    setBlocks([]);
    setOverlayVisible(true);
    setNoText(false);
    setError("");
    setCopyMode("original");
    setPicked([]);
    setGroupDragging(false);
    setEngineMenuOpen(false);
    groupDragged.current = false;
    setSelection({ x: 0, y: 0, width: 0, height: 0 });
  }, []);

  // 托盘菜单/全局快捷键触发截图：Rust侧已显示窗口，这里重置到框选模式
  useEffect(() => {
    const un = listen("trigger-screenshot", () => resetState());
    return () => {
      un.then((f) => f());
    };
  }, [resetState]);

  /** 退出截图翻译：重置状态并隐藏窗口 */
  const exit = useCallback(async () => {
    resetState();
    try {
      await win.hide();
    } catch {}
  }, [win, resetState]);

  // ESC退出
  useEffect(() => {
    const h = (e: KeyboardEvent) => {
      if (e.key === "Escape") exit();
    };
    window.addEventListener("keydown", h);
    return () => window.removeEventListener("keydown", h);
  }, [exit]);

  // ============ 选择与处理 ============

  const handleRootMouseDown = (e: React.MouseEvent) => {
    if (e.button !== 0) return; // 仅左键
    // 双击防闪：抑制原生"双击选词"高亮（双击逻辑在onDoubleClick中处理）
    if (e.detail >= 2) e.preventDefault();
    if (phase === "select") {
      setDragging(true);
      setError("");
      startPoint.current = { x: e.clientX, y: e.clientY };
      setSelection({ x: e.clientX, y: e.clientY, width: 0, height: 0 });
    } else if (phase === "result") {
      // 快照本次按下的修饰键与起始块（mouseup阶段使用）
      dragWasCtrl.current = ctrlHeld;
      dragStartBlock.current = null;
      // Ctrl多选拖选模式：不切换覆盖层，允许原生拖选开始
      if (ctrlHeld) return;
      // 引擎菜单打开时：点击菜单外仅关闭菜单，不切换覆盖层
      if (engineMenuOpen) {
        setEngineMenuOpen(false);
        return;
      }
      // 左键点击块外：隐藏/显示翻译结果
      setOverlayVisible((v) => !v);
    }
  };

  const handleRootMouseMove = (e: React.MouseEvent) => {
    if (!dragging) return;
    const s = startPoint.current;
    setSelection({
      x: Math.min(s.x, e.clientX),
      y: Math.min(s.y, e.clientY),
      width: Math.abs(e.clientX - s.x),
      height: Math.abs(e.clientY - s.y),
    });
  };

  const handleRootMouseUp = async () => {
    // 结果阶段：Ctrl+拖动累积片段 / 普通拖动覆盖式单选
    if (phase === "result") {
      const sel = window.getSelection()?.toString().trim() ?? "";
      if (dragWasCtrl.current) {
        if (sel) {
          setPicked((prev) => [
            ...prev,
            { text: sel, block: dragStartBlock.current, whole: false },
          ]);
        }
        // 原生选区保留可见；已选块以描边持续标记
      } else if (sel) {
        // 普通拖动=覆盖式单选，清空之前累积
        setPicked([]);
      }
      return;
    }
    if (!dragging) return;
    setDragging(false);
    if (selection.width < 8 || selection.height < 8) {
      setSelection({ x: 0, y: 0, width: 0, height: 0 });
      return;
    }
    await processSelection(selection);
  };

  /** 截图 → OCR → 分组判定 → 逐组翻译 → 显示结果 */
  const processSelection = async (sel: {
    x: number;
    y: number;
    width: number;
    height: number;
  }) => {
    setPhase("processing");
    setProcStage("ocr");
    setProcSeconds(0);
    setPicked([]);
    const sf = scaleFactor.current || 1;
    const physX = Math.round(monitorPos.current.x + sel.x * sf);
    const physY = Math.round(monitorPos.current.y + sel.y * sf);
    const physW = Math.round(sel.width * sf);
    const physH = Math.round(sel.height * sf);

    try {
      // 隐藏窗口，避免截到自身
      await win.hide();
      await new Promise((r) => setTimeout(r, 250));

      // 第一步：纯截图（百毫秒级，后端暂存像素）
      await invoke("capture_region_store", {
        x: physX,
        y: physY,
        width: physW,
        height: physH,
      });

      // 立即恢复UI：选区边框+加载面板在OCR/翻译期间全程可见
      // （OCR与翻译才是耗时大头，此前窗口全程隐藏导致纯空白等待）
      await win.show();
      await win.setFocus();

      // 第二步：OCR（较慢，但面板已可见）
      const ocr = await invoke<{
        lines: OcrLineInfo[];
        language: string;
      }>("ocr_stored_capture");

      // 物理像素（相对截图区域）→ 窗口CSS坐标
      const cssLines: OcrLineInfo[] = ocr.lines.map((l) => ({
        text: l.text,
        x: sel.x + l.x / sf,
        y: sel.y + l.y / sf,
        width: l.width / sf,
        height: l.height / sf,
      }));

      if (cssLines.length === 0) {
        setNoText(true);
        setBlocks([]);
        setPhase("result");
        await win.show();
        await win.setFocus();
        return;
      }

      // 分组判定
      const groups = groupLines(cssLines);
      const groupTexts = groups.map((g) => g.map((l) => l.text).join("\n"));

      // 渲染块：单组=用户框选框整块；多组=各组并集矩形（独立单行为紧凑块）
      const newBlocks: Block[] = groups.map((g, gi) => {
        if (groups.length === 1) {
          return {
            x: sel.x,
            y: sel.y,
            width: sel.width,
            height: sel.height,
            original: groupTexts[0],
            translation: "",
          };
        }
        const r = unionRect(g);
        return { ...r, original: groupTexts[gi], translation: "" };
      });
      setBlocks(newBlocks);

      // 窗口已恢复可见（截图完成后），只需切换到翻译阶段文案
      setProcStage("translate");

      // 逐组翻译（百度合并\n一次请求，按行拆分重组）
      const engineName = engines[engineIdx]?.name;
      const res = await invoke<{
        translations: string[];
        engine_used: string;
      }>("translate_lines", {
        lines: groupTexts,
        from: "auto",
        to: "zh",
        engine: engineName ?? null,
      });

      setBlocks(
        newBlocks.map((b, i) => ({ ...b, translation: res.translations[i] ?? "" }))
      );
      setNoText(false);
      setOverlayVisible(true);
      groupDragged.current = false;
      setPhase("result");
      await win.show();
      await win.setFocus();
    } catch (e: any) {
      setError(typeof e === "string" ? e : "截图处理失败");
      setPhase("select");
      setSelection({ x: 0, y: 0, width: 0, height: 0 });
      try {
        await win.show();
      } catch {}
    }
  };

  // ============ 按键组（菜单组） ============

  // 位置：底边居中下方对齐；超出屏幕下方 → 底边居中上方对齐（仍锚定底边）
  useLayoutEffect(() => {
    if (phase !== "result" || groupDragged.current) return;
    const el = groupRef.current;
    if (!el) return;
    const r = el.getBoundingClientRect();
    const w = r.width || 220;
    const h = r.height || 40;
    const centerX = selection.x + selection.width / 2;
    const bottom = selection.y + selection.height;
    let y = bottom + 10;
    if (y + h > window.innerHeight) {
      y = bottom - 10 - h;
    }
    let x = centerX - w / 2;
    if (x < 8) x = 8;
    if (x + w > window.innerWidth - 8) x = window.innerWidth - 8 - w;
    setGroupPos({ x, y });
  }, [phase, selection]);

  const handleGroupMouseDown = (e: React.MouseEvent) => {
    e.stopPropagation();
    setGroupDragging(true);
    groupDragged.current = true;
    dragOffset.current = { x: e.clientX - groupPos.x, y: e.clientY - groupPos.y };
  };

  useEffect(() => {
    if (!groupDragging) return;
    const onMove = (e: MouseEvent) => {
      setGroupPos({
        x: e.clientX - dragOffset.current.x,
        y: e.clientY - dragOffset.current.y,
      });
    };
    const onUp = () => setGroupDragging(false);
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
    return () => {
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
    };
  }, [groupDragging]);

  /** 用指定引擎重译并刷新覆盖层 */
  const retranslate = async (engineName: string | null) => {
    if (blocks.length === 0 || retranslating) return;
    setRetranslating(true);
    try {
      const res = await invoke<{
        translations: string[];
        engine_used: string;
      }>("translate_lines", {
        lines: blocks.map((b) => b.original),
        from: "auto",
        to: "zh",
        engine: engineName,
      });
      setBlocks((prev) =>
        prev.map((b, i) => ({
          ...b,
          translation: res.translations[i] ?? b.translation,
        }))
      );
    } catch (e: any) {
      setError(typeof e === "string" ? e : "重新翻译失败");
    } finally {
      setRetranslating(false);
    }
  };

  /** 点击引擎按钮：向上展开垂直引擎菜单（每次打开实时拉取引擎列表） */
  const toggleEngineMenu = async () => {
    if (engineMenuOpen) {
      setEngineMenuOpen(false);
      return;
    }
    try {
      const es = await invoke<EngineInfo[]>("list_engines");
      const configured = es.filter((e) => e.configured);
      setEngines(configured);
      setEngineIdx((i) =>
        configured.length ? Math.min(i, configured.length - 1) : 0
      );
    } catch {}
    setEngineMenuOpen(true);
  };

  /** 点选菜单中的引擎：关闭菜单 + 重译刷新 */
  const selectEngine = async (idx: number) => {
    setEngineMenuOpen(false);
    if (blocks.length === 0 || retranslating) return;
    setEngineIdx(idx);
    await retranslate(engines[idx]?.name ?? null);
  };

  /** 复制到剪贴板并退出。内容优先级：
   *  1) 当前原生选区文字（拖动单选 / 双击整段，所见即所得）
   *  2) Ctrl累积的多个片段（Ctrl+拖动、Ctrl+双击；按操作顺序合并）
   *  3) 全部段落（按当前原/译模式） */
  const handleCopy = async () => {
    let text = window.getSelection()?.toString().trim() ?? "";
    if (!text && picked.length > 0) {
      text = picked.map((p) => p.text).join("\n");
    }
    if (!text) {
      text = blocks
        .map((b) => (copyMode === "original" ? b.original : b.translation))
        .join("\n");
    }
    try {
      try {
        await navigator.clipboard.writeText(text);
      } catch {
        // 剪贴板API不可用时的降级方案
        const ta = document.createElement("textarea");
        ta.value = text;
        document.body.appendChild(ta);
        ta.select();
        document.execCommand("copy");
        document.body.removeChild(ta);
      }
      exit();
    } catch {
      setError("复制到剪贴板失败");
    }
  };

  /** 打开主窗口设置页（截图会话保留，不打断） */
  const openSettings = async () => {
    try {
      // 临时取消置顶，让主窗口设置页置顶显示（z2设置页 > z1覆盖层 > z0原屏幕）
      await win.setAlwaysOnTop(false);
      await emit("open-screenshot-settings");
    } catch (e) {
      console.error("打开设置失败:", e);
    }
  };

  const handleContextMenu = (e: React.MouseEvent) => {
    e.preventDefault();
    exit();
  };

  // ============ 渲染 ============

  return (
    <div
      className={`screenshot-root ${
        phase === "result" && ctrlHeld ? "ctrl-mode" : ""
      }`}
      onMouseDown={handleRootMouseDown}
      onMouseMove={handleRootMouseMove}
      onMouseUp={handleRootMouseUp}
      onContextMenu={handleContextMenu}
    >
      {/* 选择/处理阶段：暗幕 + 提示 */}
      {phase !== "result" && (
        <div className="dim-mask">
          {!dragging && phase === "select" && (
            <div className="hint">
              <div className="hint-main">拖动鼠标框选要翻译的区域</div>
              <div className="hint-sub">
                左键隐藏/显示翻译 · 右键退出 · ESC退出
              </div>
            </div>
          )}
          {phase === "processing" && (
            <div
              className="processing-panel"
              style={{
                left:
                  selection.width > 0
                    ? Math.min(
                        Math.max(selection.x + selection.width / 2, 130),
                        window.innerWidth - 130
                      )
                    : window.innerWidth / 2,
                top:
                  selection.height > 0
                    ? Math.min(
                        Math.max(selection.y + selection.height / 2, 70),
                        window.innerHeight - 70
                      )
                    : window.innerHeight / 2,
              }}
            >
              <span className="processing-spinner" />
              <span className="processing-text">
                {procStage === "ocr" ? "正在识别文字…" : "正在翻译…"}
              </span>
              {procSeconds > 0 && (
                <span className="processing-elapsed">{procSeconds}s</span>
              )}
              <span className="processing-sub">ESC 取消</span>
            </div>
          )}
        </div>
      )}

      {/* 拖动中的选区框 */}
      {dragging && selection.width > 0 && (
        <div
          className="drag-selection"
          style={{
            left: selection.x,
            top: selection.y,
            width: selection.width,
            height: selection.height,
          }}
        />
      )}

      {/* 结果/处理阶段：选区描边常驻（处理中给用户视觉锚点） */}
      {(phase === "result" || phase === "processing") && selection.width > 0 && (
        <div
          className="selection-border"
          style={{
            left: selection.x,
            top: selection.y,
            width: selection.width,
            height: selection.height,
          }}
        />
      )}

      {/* 翻译色块（1:1覆盖原文区域）
          交互：块外左键单击=隐藏/显示覆盖层；块内拖选=选中文字（原生选区）；
                双击=原生选中整段文字（与拖动同款选字）；Ctrl+双击=该段文字加入
                片段累积；复制按钮按优先级取内容 */}
      {phase === "result" &&
        overlayVisible &&
        blocks.map((b, i) => (
          <div
            key={i}
            className="block"
            onMouseDown={(e) => {
              // 记录起始块与修饰键快照（供mouseup累积片段用）
              dragStartBlock.current = i;
              dragWasCtrl.current = ctrlHeld;
              e.stopPropagation();
              // 双击防闪：抑制原生选词高亮（双击整段选中在onDoubleClick中处理）
              if (e.detail >= 2) e.preventDefault();
            }}
            onDoubleClick={(e) => {
              e.stopPropagation();
              const el = e.currentTarget;
              // 双击（含Ctrl+双击）：原生选中整段文字——
              // 所有选中态统一用原生贴字高亮，无描边/徽标等额外样式
              const sel = window.getSelection();
              if (!sel) return;
              sel.removeAllRanges();
              const range = document.createRange();
              range.selectNodeContents(el);
              sel.addRange(range);
              if (e.ctrlKey || e.shiftKey) {
                // Ctrl+双击：整段加入片段累积（再次同块操作=取消）
                const t = el.textContent?.trim() ?? "";
                if (t) {
                  setPicked((prev) => {
                    const idx = prev.findIndex((p) => p.whole && p.block === i);
                    if (idx >= 0) return prev.filter((_, k) => k !== idx);
                    return [...prev, { text: t, block: i, whole: true }];
                  });
                }
              }
            }}
            style={{
              left: b.x,
              top: b.y,
              // 背景自适应模式：色块贴合翻译文字（宽度自动、超过原文宽度换行）；
              // 默认模式/无背景模式：固定覆盖原文矩形
              width:
                settings.overlay_bg_fit_text && !settings.overlay_transparent
                  ? "fit-content"
                  : b.width,
              maxWidth:
                settings.overlay_bg_fit_text && !settings.overlay_transparent
                  ? b.width
                  : undefined,
              height:
                settings.overlay_bg_fit_text && !settings.overlay_transparent
                  ? "auto"
                  : b.height,
              background: settings.overlay_transparent
                ? "transparent"
                : hexToRgba(
                    settings.overlay_bg_color,
                    settings.overlay_opacity
                  ),
              boxShadow: settings.overlay_transparent
                ? "none"
                : undefined,
              textShadow: settings.overlay_transparent
                ? "0 0 3px rgba(255,255,255,0.95), 0 0 6px rgba(255,255,255,0.7)"
                : undefined,
              fontSize: fitFontSize(b.translation, b.width, b.height),
              alignItems: b.translation.includes("\n")
                ? "flex-start"
                : "center",
              userSelect: "text",
              cursor: "text",
            }}
          >
            {b.translation || "…"}
          </div>
        ))}

      {/* 按键组（菜单组）：底边居中下方/上方，10%→悬停100%，可拖动 */}
      {phase === "result" && !noText && (
        <div
          ref={groupRef}
          className={`menu-group ${groupDragging ? "dragging" : ""} ${
            engineMenuOpen ? "menu-open" : ""
          }`}
          style={{ left: groupPos.x, top: groupPos.y }}
          onMouseDown={handleGroupMouseDown}
        >
          {settings.screenshot_components.includes("engine") && (
            <div className="engine-wrap">
              <button
                className="group-item engine"
                onMouseDown={(e) => e.stopPropagation()}
                onClick={toggleEngineMenu}
                title="选择翻译引擎并刷新翻译"
              >
                {retranslating ? "…" : engines[engineIdx]?.name || "选择引擎"}
              </button>
              {engineMenuOpen && (
                <div
                  className="engine-menu"
                  onMouseDown={(e) => e.stopPropagation()}
                >
                  {engines.length === 0 && (
                    <div className="engine-menu-empty">
                      无可用引擎（请到设置填写API密钥）
                    </div>
                  )}
                  {engines.map((e, i) => (
                    <button
                      key={e.name}
                      className={`engine-menu-item ${
                        i === engineIdx ? "active" : ""
                      }`}
                      onMouseDown={(e) => e.stopPropagation()}
                      onClick={() => selectEngine(i)}
                    >
                      <span className="engine-name">{e.name}</span>
                      {i === engineIdx && <span className="engine-check">✓</span>}
                    </button>
                  ))}
                </div>
              )}
            </div>
          )}
          {settings.screenshot_components.includes("copy") && (
            <>
              <button
                className="group-item"
                onMouseDown={(e) => {
                  // 阻止点击按钮时浏览器清除原生文字选区
                  e.stopPropagation();
                  e.preventDefault();
                }}
                onClick={handleCopy}
                title={`复制（拖选文字/选中段落/全部，当前${copyMode === "original" ? "原文" : "译文"}）`}
              >
                ⧉
              </button>
              <button
                className="group-item toggle"
                onMouseDown={(e) => e.stopPropagation()}
                onClick={() =>
                  setCopyMode((m) =>
                    m === "original" ? "translated" : "original"
                  )
                }
                title="切换复制内容：原文/译文"
              >
                {copyMode === "original" ? "原" : "译"}
              </button>
            </>
          )}
          {settings.screenshot_components.includes("close") && (
            <button
              className="group-item close"
              onMouseDown={(e) => e.stopPropagation()}
              onClick={exit}
              title="退出截图翻译"
            >
              ×
            </button>
          )}
          {settings.screenshot_components.includes("settings") && (
            <button
              className="group-item"
              onMouseDown={(e) => e.stopPropagation()}
              onClick={openSettings}
              title="打开设置（当前会话保留）"
            >
              ⚙
            </button>
          )}
        </div>
      )}

      {noText && phase === "result" && (
        <div className="toast">未识别到文字（右键退出后重试）</div>
      )}
      {error && (
        <div className="toast error" onClick={() => setError("")}>
          {error}（点击关闭）
        </div>
      )}
    </div>
  );
};

export default ScreenshotWindow;