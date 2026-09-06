//! 截图与OCR模块
//! - GDI截图：捕获屏幕指定区域（BGRA像素）
//! - OCR：Windows内置OCR引擎（Windows.Media.Ocr，支持中英文，无需外部安装）
//! - 结构化返回：逐行文本 + 紧凑边界矩形（物理像素），供前端做段落判定与1:1覆盖
//! - 小区域自动放大 + 词级行重组：修复小框选识别率低、横排误判竖排的问题

use crate::translation::AppState;
use serde::{Deserialize, Serialize};
use std::sync::Mutex;

/// 最近一次截图的暂存区：配合 capture_region_store + ocr_stored_capture
/// 把"截图"与"OCR"拆成两步，让前端在较慢的OCR期间恢复UI显示
static LAST_CAPTURE: Mutex<Option<ScreenshotData>> = Mutex::new(None);

#[derive(Debug, Serialize, Deserialize)]
pub struct ScreenshotRegion {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

/// 截图数据：原始BGRA像素 + 尺寸
#[derive(Debug, Serialize, Deserialize)]
pub struct ScreenshotData {
    pub pixels: Vec<u8>,
    pub width: i32,
    pub height: i32,
}

/// OCR识别出的一行：文本 + 紧凑边界矩形（物理像素，相对截图区域原点）
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OcrLineInfo {
    pub text: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// OCR结构化结果：所有行 + 所用语言
#[derive(Debug, Serialize, Deserialize)]
pub struct OcrResult {
    pub lines: Vec<OcrLineInfo>,
    pub language: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ExtractButton {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub opacity: f64,
    pub visible: bool,
    pub draggable: bool,
}

// ============ 按键组位置计算 ============
// 需求：在框选边框的下方边框居中下方对齐
// 如果超出屏幕最大显示范围则在框选边框的下方边框居中上方对齐（仍锚定底边）
pub fn calculate_extract_button_position(
    selection_rect: &ScreenshotRegion,
    screen_bounds: &(i32, i32, u32, u32),
) -> (f64, f64) {
    let button_width = 120.0;
    let button_height = 40.0;
    let gap = 10.0;

    // 默认位置：底边居中下方对齐
    let mut x = selection_rect.x as f64 + (selection_rect.width as f64 - button_width) / 2.0;
    let mut y = selection_rect.y as f64 + selection_rect.height as f64 + gap;

    let screen_right = screen_bounds.0 as f64 + screen_bounds.2 as f64;
    let screen_bottom = screen_bounds.1 as f64 + screen_bounds.3 as f64;

    // 超出屏幕下方：改为底边居中上方对齐（仍锚定底边）
    if y + button_height > screen_bottom {
        y = selection_rect.y as f64 + selection_rect.height as f64 - gap - button_height;
    }

    if x < 0.0 {
        x = 0.0;
    } else if x + button_width > screen_right {
        x = screen_right - button_width;
    }

    (x, y)
}

// ============ Windows实现 ============

#[cfg(target_os = "windows")]
mod win {
    use super::*;

    use windows::Win32::Graphics::Gdi::{
        BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC,
        GetDIBits, ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, HGDIOBJ,
        SRCCOPY,
    };

    /// GDI截图：捕获屏幕指定区域，返回BGRA像素（自顶向下）
    pub fn capture_region(
        x: i32,
        y: i32,
        width: i32,
        height: i32,
    ) -> Result<ScreenshotData, String> {
        if width <= 0 || height <= 0 {
            return Err("截图区域尺寸无效".to_string());
        }

        unsafe {
            let screen_dc = GetDC(None);
            if screen_dc.is_invalid() {
                return Err("获取屏幕DC失败".to_string());
            }

            let mem_dc = CreateCompatibleDC(Some(screen_dc));
            if mem_dc.is_invalid() {
                let _ = ReleaseDC(None, screen_dc);
                return Err("创建内存DC失败".to_string());
            }

            let bitmap = CreateCompatibleBitmap(screen_dc, width, height);
            if bitmap.is_invalid() {
                let _ = DeleteDC(mem_dc);
                let _ = ReleaseDC(None, screen_dc);
                return Err("创建位图失败".to_string());
            }

            let old_obj = SelectObject(mem_dc, HGDIOBJ(bitmap.0));

            if BitBlt(mem_dc, 0, 0, width, height, Some(screen_dc), x, y, SRCCOPY).is_err() {
                let _ = SelectObject(mem_dc, old_obj);
                let _ = DeleteObject(HGDIOBJ(bitmap.0));
                let _ = DeleteDC(mem_dc);
                let _ = ReleaseDC(None, screen_dc);
                return Err("BitBlt复制失败".to_string());
            }

            let mut bi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: width,
                    biHeight: -height,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: 0,
                    ..Default::default()
                },
                bmiColors: [Default::default(); 1],
            };

            let pixel_count = (width * height * 4) as usize;
            let mut pixels: Vec<u8> = vec![0u8; pixel_count];

            let lines = GetDIBits(
                mem_dc,
                bitmap,
                0,
                height as u32,
                Some(pixels.as_mut_ptr() as *mut _),
                &mut bi,
                DIB_RGB_COLORS,
            );

            let _ = SelectObject(mem_dc, old_obj);
            let _ = DeleteObject(HGDIOBJ(bitmap.0));
            let _ = DeleteDC(mem_dc);
            let _ = ReleaseDC(None, screen_dc);

            if lines == 0 {
                return Err("GetDIBits读取像素失败".to_string());
            }

            Ok(ScreenshotData {
                pixels,
                width,
                height,
            })
        }
    }

    /// OCR词项（内部中间结构）
    #[derive(Debug, Clone)]
    pub(crate) struct OcrWordItem {
        pub text: String,
        pub x: f64,
        pub y: f64,
        pub width: f64,
        pub height: f64,
    }

    /// 小区域放大后再OCR（提升小字号识别率）
    /// 最大边≤120放大3倍，≤300放大2倍；返回(像素, 宽, 高, 倍率)
    fn upscale_pixels(pixels: &[u8], width: i32, height: i32) -> (Vec<u8>, i32, i32, u32) {
        let max_dim = width.max(height);
        let scale: i32 = if max_dim <= 120 {
            3
        } else if max_dim <= 300 {
            2
        } else {
            1
        };
        if scale == 1 || width <= 0 || height <= 0 {
            return (pixels.to_vec(), width, height, 1);
        }

        let new_w = width * scale;
        let new_h = height * scale;
        let mut out = vec![0u8; (new_w * new_h * 4) as usize];
        for y in 0..height {
            for x in 0..width {
                let src = ((y * width + x) * 4) as usize;
                for dy in 0..scale {
                    for dx in 0..scale {
                        let dst = (((y * scale + dy) * new_w + (x * scale + dx)) * 4) as usize;
                        out[dst] = pixels[src];
                        out[dst + 1] = pixels[src + 1];
                        out[dst + 2] = pixels[src + 2];
                        out[dst + 3] = pixels[src + 3];
                    }
                }
            }
        }
        (out, new_w, new_h, scale as u32)
    }

    /// 词级行重组：按y范围重叠率≥50%判定同一视觉行
    /// 修复OCR行分割错误（横排文字被拆成多个"行"导致误判竖排）
    /// 行内按x排序；拉丁词间有明显间隙加空格，CJK直接拼接
    /// 内置符号纠错表（穷举常用误识别，术语管理里可继续追加用户映射）：
    /// OCR输出字符 → 正确字符。只收录"正常文本几乎不会出现"的形近字/变体，
    /// 避免误伤正文（如 一/丁 等常用字绝不收录）
    const BUILTIN_SYMBOL_FIXES: &[(&str, &str)] = &[
        // 箭头：部首/笔画形近字（→ ← ↑ ↓ 的常见误识别产物）
        ("乛", "→"),
        ("⺂", "→"),
        ("⺡", "→"),
        ("乁", "←"),
        // 箭头变体统一为基本箭头
        ("⇒", "→"),
        ("⇨", "→"),
        ("➔", "→"),
        ("➜", "→"),
        ("➤", "→"),
        ("⇐", "←"),
        ("⇑", "↑"),
        ("⇓", "↓"),
        // 竖线形近字/全角
        ("丨", "|"),
        ("︱", "|"),
        ("｜", "|"),
        // 省略号/艾特
        ("⋯", "…"),
        ("＠", "@"),
        // 带圈数字 → 阿拉伯数字（引擎对纯数字的处理与排版更稳）
        ("①", "1"),
        ("②", "2"),
        ("③", "3"),
        ("④", "4"),
        ("⑤", "5"),
        ("⑥", "6"),
        ("⑦", "7"),
        ("⑧", "8"),
        ("⑨", "9"),
        ("⑩", "10"),
        ("⑪", "11"),
        ("⑫", "12"),
        ("⑬", "13"),
        ("⑭", "14"),
        ("⑮", "15"),
        ("⑯", "16"),
        ("⑰", "17"),
        ("⑱", "18"),
        ("⑲", "19"),
        ("⑳", "20"),
        // 带括号数字
        ("⑴", "(1)"),
        ("⑵", "(2)"),
        ("⑶", "(3)"),
        ("⑷", "(4)"),
        ("⑸", "(5)"),
        ("⑹", "(6)"),
        ("⑺", "(7)"),
        ("⑻", "(8)"),
        ("⑼", "(9)"),
        ("⑽", "(10)"),
    ];

    /// 应用符号纠错：先内置穷举表，再用户术语映射（术语管理中 1-2 字非字母数字源词）
    pub(crate) fn apply_symbol_corrections(
        text: &str,
        corrections: &[(String, String)],
    ) -> String {
        let mut result = text.to_string();
        for (from, to) in BUILTIN_SYMBOL_FIXES {
            result = result.replace(from, to);
        }
        for (from, to) in corrections {
            result = result.replace(from.as_str(), to.as_str());
        }
        result
    }

    pub(crate) fn regroup_words_to_lines(
        words: Vec<OcrWordItem>,
        corrections: &[(String, String)],
    ) -> Vec<OcrLineInfo> {
        // 词级纠错：内置穷举表 + 术语管理用户映射（如 乛→→、③→3）。
        // 不再做任何剔除：未命中的符号原样保留，交给翻译引擎处理
        let words: Vec<OcrWordItem> = words
            .into_iter()
            .map(|mut w| {
                w.text = apply_symbol_corrections(&w.text, corrections);
                w
            })
            .collect();
        if words.is_empty() {
            return vec![];
        }

        // 按y中心排序
        let mut sorted = words;
        sorted.sort_by(|a, b| {
            let ac = a.y + a.height / 2.0;
            let bc = b.y + b.height / 2.0;
            ac.partial_cmp(&bc).unwrap_or(std::cmp::Ordering::Equal)
        });

        // 贪心分行：与当前行y范围重叠率≥0.5归入当前行
        let mut line_groups: Vec<Vec<OcrWordItem>> = vec![vec![sorted[0].clone()]];
        for w in sorted.iter().skip(1) {
            let cur = line_groups.last_mut().unwrap();
            let top = cur.iter().map(|x| x.y).fold(f64::MAX, f64::min);
            let bottom = cur.iter().map(|x| x.y + x.height).fold(f64::MIN, f64::max);
            let overlap = (bottom.min(w.y + w.height) - top.max(w.y)).max(0.0);
            let min_h = cur
                .iter()
                .map(|x| x.height)
                .fold(f64::MAX, f64::min)
                .min(w.height)
                .max(0.001);
            if overlap / min_h >= 0.5 {
                cur.push(w.clone());
            } else {
                line_groups.push(vec![w.clone()]);
            }
        }

        // 行内按x排序 + 智能拼接 + 行矩形
        let mut result = Vec::with_capacity(line_groups.len());
        for mut line in line_groups {
            line.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal));

            let max_h = line.iter().map(|x| x.height).fold(0.0f64, f64::max);

            // 行内按水平间隔二次切分：间隔 > 1.5×行高 视为左右并排的独立文本
            // （表格两列、标签+值等），不再硬拼成一行（用户反馈：间隔过大仍识别一起）
            let mut segments: Vec<Vec<&OcrWordItem>> = vec![vec![&line[0]]];
            for w in line.iter().skip(1) {
                let seg = segments.last_mut().unwrap();
                let prev = seg.last().unwrap();
                let gap_x = w.x - (prev.x + prev.width);
                if gap_x > max_h * 1.5 {
                    segments.push(vec![w]);
                } else {
                    seg.push(w);
                }
            }

            for seg in segments {
                let mut text = String::new();
                for (i, w) in seg.iter().enumerate() {
                    if i > 0 {
                        let prev = seg[i - 1];
                        let gap_x = w.x - (prev.x + prev.width);
                        let prev_latin = prev
                            .text
                            .chars()
                            .last()
                            .map(|c| c.is_ascii_alphanumeric())
                            .unwrap_or(false);
                        let cur_latin = w
                            .text
                            .chars()
                            .next()
                            .map(|c| c.is_ascii_alphanumeric())
                            .unwrap_or(false);
                        // 拉丁词之间有明显间隙→补空格（OCR英文词本身不含空格）
                        if prev_latin && cur_latin && gap_x > max_h * 0.1 {
                            text.push(' ');
                        }
                    }
                    text.push_str(&w.text);
                }

                let text = text.trim().to_string();
                if text.is_empty() {
                    continue;
                }

                let min_x = seg.iter().map(|w| w.x).fold(f64::MAX, f64::min);
                let min_y = seg.iter().map(|w| w.y).fold(f64::MAX, f64::min);
                let max_x = seg
                    .iter()
                    .map(|w| w.x + w.width)
                    .fold(f64::MIN, f64::max);
                let max_y = seg
                    .iter()
                    .map(|w| w.y + w.height)
                    .fold(f64::MIN, f64::max);

                result.push(OcrLineInfo {
                    text,
                    x: min_x,
                    y: min_y,
                    width: max_x - min_x,
                    height: max_y - min_y,
                });
            }
        }
        result
    }

    /// Windows内置OCR：从BGRA像素识别文字
    /// 返回结构化行数据（文本+紧凑矩形，物理像素，相对截图区域原点）
    /// 优先中文引擎（zh-Hans-CN），fallback到用户配置文件语言
    pub fn ocr_from_pixels(
        pixels: &[u8],
        width: i32,
        height: i32,
        corrections: &[(String, String)],
    ) -> Result<OcrResult, String> {
        use windows::Graphics::Imaging::{
            BitmapAlphaMode, BitmapDecoder, BitmapEncoder, BitmapPixelFormat,
        };
        use windows::Storage::Streams::InMemoryRandomAccessStream;

        // 小区域放大后再识别（提升小字号识别率，修复小框选误判）
        let (pixels, width, height, scale) = upscale_pixels(pixels, width, height);

        // 通过WinRT编码器将像素写入流（Bgra8）
        let stream =
            InMemoryRandomAccessStream::new().map_err(|e| format!("创建内存流失败: {e}"))?;

        let encoder_id =
            BitmapEncoder::PngEncoderId().map_err(|e| format!("获取PNG编码器ID失败: {e}"))?;
        let encoder = BitmapEncoder::CreateAsync(encoder_id, &stream)
            .map_err(|e| format!("创建编码器失败: {e}"))?
            .get()
            .map_err(|e| format!("初始化编码器失败: {e}"))?;

        encoder
            .SetPixelData(
                BitmapPixelFormat::Bgra8,
                BitmapAlphaMode::Premultiplied,
                width as u32,
                height as u32,
                1.0,
                1.0,
                &pixels,
            )
            .map_err(|e| format!("写入像素数据失败: {e}"))?;
        encoder
            .FlushAsync()
            .map_err(|e| format!("启动编码失败: {e}"))?
            .get()
            .map_err(|e| format!("编码完成失败: {e}"))?;

        // 重置流位置，解码为SoftwareBitmap
        stream.Seek(0).map_err(|e| format!("重置流失败: {e}"))?;
        let decoder = BitmapDecoder::CreateAsync(&stream)
            .map_err(|e| format!("创建解码器失败: {e}"))?
            .get()
            .map_err(|e| format!("初始化解码器失败: {e}"))?;
        let bitmap = decoder
            .GetSoftwareBitmapAsync()
            .map_err(|e| format!("启动位图解码失败: {e}"))?
            .get()
            .map_err(|e| format!("位图解码失败: {e}"))?;

        // 创建OCR引擎：优先中文
        let (engine, lang_name) = create_ocr_engine()?;

        // 执行识别（阻塞等待完成，运行在线程池中）
        let operation = engine
            .RecognizeAsync(&bitmap)
            .map_err(|e| format!("启动OCR识别失败: {e}"))?;
        let result = operation.get().map_err(|e| format!("OCR识别失败: {e}"))?;

        // 展平所有词（坐标除以scale还原到原始尺寸）
        let ocr_lines = result.Lines().map_err(|e| format!("获取OCR行失败: {e}"))?;
        let line_count = ocr_lines
            .Size()
            .map_err(|e| format!("获取OCR行数失败: {e}"))?;

        let mut all_words: Vec<OcrWordItem> = Vec::new();
        for i in 0..line_count {
            let line = ocr_lines
                .GetAt(i)
                .map_err(|e| format!("获取OCR行失败: {e}"))?;
            let words = line.Words().map_err(|e| format!("获取OCR词失败: {e}"))?;
            let word_count = words
                .Size()
                .map_err(|e| format!("获取OCR词数失败: {e}"))?;

            for j in 0..word_count {
                let word = words
                    .GetAt(j)
                    .map_err(|e| format!("获取OCR词失败: {e}"))?;
                let text = word
                    .Text()
                    .map_err(|e| format!("读取词文本失败: {e}"))?
                    .to_string();
                if text.trim().is_empty() {
                    continue;
                }
                let r = word
                    .BoundingRect()
                    .map_err(|e| format!("获取词矩形失败: {e}"))?;
                all_words.push(OcrWordItem {
                    text,
                    x: r.X as f64 / scale as f64,
                    y: r.Y as f64 / scale as f64,
                    width: r.Width as f64 / scale as f64,
                    height: r.Height as f64 / scale as f64,
                });
            }
        }

        // 词级行重组（修复横排被拆成多行的误判）
        let lines = regroup_words_to_lines(all_words, corrections);

        Ok(OcrResult {
            lines,
            language: lang_name,
        })
    }

    /// 创建OCR引擎：中文优先，fallback用户配置语言
    fn create_ocr_engine() -> Result<(windows::Media::Ocr::OcrEngine, String), String> {
        use windows::Globalization::Language;
        use windows::Media::Ocr::OcrEngine;

        let zh_lang = Language::CreateLanguage(&windows::core::HSTRING::from("zh-Hans-CN"))
            .map_err(|e| format!("创建语言对象失败: {e}"))?;
        if let Ok(engine) = OcrEngine::TryCreateFromLanguage(&zh_lang) {
            return Ok((engine, "中文".to_string()));
        }

        if let Ok(engine) = OcrEngine::TryCreateFromUserProfileLanguages() {
            return Ok((engine, "系统语言".to_string()));
        }

        Err("无法创建OCR引擎（系统未安装OCR语言包）".to_string())
    }
}

// ============ Tauri命令 ============

/// 截图+OCR一体命令：捕获屏幕指定区域并识别，返回结构化行数据
/// （避免像素数组跨IPC传输，单次调用完成）
#[tauri::command]
pub async fn capture_region_ocr(
    x: i32,
    y: i32,
    width: i32,
    height: i32,
) -> Result<OcrResult, String> {
    #[cfg(target_os = "windows")]
    {
        tauri::async_runtime::spawn_blocking(move || {
            let data = win::capture_region(x, y, width, height)?;
            win::ocr_from_pixels(&data.pixels, data.width, data.height, &[])
        })
        .await
        .map_err(|e| format!("OCR任务执行失败: {e}"))?
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (x, y, width, height);
        Err("当前平台暂不支持截图OCR".to_string())
    }
}

/// 捕获屏幕指定区域并暂存于后端内存（不跨IPC传像素）
/// 配合 ocr_stored_capture 使用：截图（百毫秒级）完成后前端立即恢复UI，
/// 较慢的OCR在窗口可见状态下执行，消除"松手后纯空白"的等待期
#[tauri::command]
pub fn capture_region_store(x: i32, y: i32, width: i32, height: i32) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        let data = win::capture_region(x, y, width, height)?;
        *LAST_CAPTURE
            .lock()
            .map_err(|_| "截图缓存锁不可用".to_string())? = Some(data);
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (x, y, width, height);
        Err("当前平台暂不支持截图".to_string())
    }
}

/// 对暂存的截图执行OCR（OCR较慢，异步执行，取走即清空缓存）
/// 引擎按 settings.ocr_engine 分发：windows=内置引擎；youdao=有道OCR（云）。
/// 符号纠错映射来自术语管理（1-2字非字母数字源词的术语），如 乛→→
#[tauri::command]
pub async fn ocr_stored_capture(
    state: tauri::State<'_, AppState>,
) -> Result<OcrResult, String> {
    #[cfg(target_os = "windows")]
    {
        // 从术语库提取符号映射（短源词且不含字母数字，避免把普通术语误当符号）
        let corrections: Vec<(String, String)> = {
            let manager = state.manager.lock().await;
            manager
                .list_terms()
                .into_iter()
                .filter(|t| {
                    t.source.chars().count() <= 2
                        && !t.source.chars().any(|c| c.is_ascii_alphanumeric())
                })
                .filter_map(|t| {
                    t.best_translation().map(|b| (t.source.clone(), b.to_string()))
                })
                .collect()
        };

        let data = LAST_CAPTURE
            .lock()
            .map_err(|_| "截图缓存锁不可用".to_string())?
            .take()
            .ok_or_else(|| "没有已暂存的截图".to_string())?;

        let settings = crate::system::load_settings();
        match settings.ocr_engine.as_str() {
            "youdao" => {
                youdao_ocr_pixels(
                    &data,
                    &settings.youdao_app_key,
                    &settings.youdao_app_secret,
                    &corrections,
                )
                .await
            }
            "rapidocr" => {
                tauri::async_runtime::spawn_blocking(move || {
                    rapidocr_ocr_pixels(&data, &corrections)
                })
                .await
                .map_err(|e| format!("OCR任务执行失败: {e}"))?
            }
            _ => {
                tauri::async_runtime::spawn_blocking(move || {
                    win::ocr_from_pixels(&data.pixels, data.width, data.height, &corrections)
                })
                .await
                .map_err(|e| format!("OCR任务执行失败: {e}"))?
            }
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = state;
        Err("当前平台暂不支持截图OCR".to_string())
    }
}

/// 有道OCR（云）：区域截图 BGRA → PNG → Base64 → ocrapi
/// 签名按有道OCR规范：input = sha256(q的Base64字符串)，sha256(appKey+input+salt+curtime+secret)
async fn youdao_ocr_pixels(
    data: &ScreenshotData,
    app_key: &str,
    app_secret: &str,
    corrections: &[(String, String)],
) -> Result<OcrResult, String> {
    use base64::Engine as _;
    use sha2::{Digest, Sha256};

    if app_key.is_empty() || app_secret.is_empty() {
        return Err("有道OCR未配置：请先在设置中填写有道应用ID与密钥".to_string());
    }

    // BGRA → RGBA → PNG → Base64
    let mut rgba = data.pixels.clone();
    for px in rgba.chunks_exact_mut(4) {
        px.swap(0, 2);
    }
    let img = image::RgbaImage::from_raw(data.width as u32, data.height as u32, rgba)
        .ok_or_else(|| "图像数据无效".to_string())?;
    let mut png = Vec::new();
    image::DynamicImage::ImageRgba8(img)
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .map_err(|e| format!("PNG编码失败: {e}"))?;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&png);

    let salt = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos().to_string())
        .unwrap_or_else(|_| "0".to_string());
    let curtime = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_else(|_| "0".to_string());

    let q_hash = format!("{:x}", Sha256::digest(b64.as_bytes()));
    let sign_raw = format!("{}{}{}{}{}", app_key, q_hash, salt, curtime, app_secret);
    let sign = format!("{:x}", Sha256::digest(sign_raw.as_bytes()));

    let client = reqwest::Client::new();
    let resp: serde_json::Value = client
        .post("https://openapi.youdao.com/ocrapi")
        .form(&[
            ("type", "1".to_string()),
            ("q", b64),
            ("langType", "auto".to_string()),
            ("appKey", app_key.to_string()),
            ("salt", salt),
            ("curtime", curtime),
            ("signType", "v3".to_string()),
            ("sign", sign),
        ])
        .send()
        .await
        .map_err(|e| format!("有道OCR请求失败: {e}"))?
        .json()
        .await
        .map_err(|e| format!("有道OCR响应解析失败: {e}"))?;

    if let Some(code) = resp.get("errorCode").and_then(|v| v.as_str()) {
        if code != "0" {
            return Err(format!("有道OCR失败（错误码{code}）"));
        }
    }

    // 解析 lines：boundingBox 内递归收集所有 (x,y) 点，取包围盒
    let empty = Vec::new();
    let raw_lines = resp
        .get("lines")
        .and_then(|v| v.as_array())
        .unwrap_or(&empty);
    let mut lines = Vec::new();
    for l in raw_lines {
        let text = win::apply_symbol_corrections(
            l.get("text").and_then(|v| v.as_str()).unwrap_or(""),
            corrections,
        );
        if text.trim().is_empty() {
            continue;
        }
        let mut pts: Vec<(f64, f64)> = Vec::new();
        collect_points(l.get("boundingBox").unwrap_or(&serde_json::Value::Null), &mut pts);
        if pts.is_empty() {
            continue;
        }
        let min_x = pts.iter().map(|p| p.0).fold(f64::MAX, f64::min);
        let min_y = pts.iter().map(|p| p.1).fold(f64::MAX, f64::min);
        let max_x = pts.iter().map(|p| p.0).fold(f64::MIN, f64::max);
        let max_y = pts.iter().map(|p| p.1).fold(f64::MIN, f64::max);
        lines.push(OcrLineInfo {
            text,
            x: min_x,
            y: min_y,
            width: max_x - min_x,
            height: max_y - min_y,
        });
    }
    Ok(OcrResult { lines, language: "auto".to_string() })
}

/// RapidOCR 本地引擎（PaddleOCR PP-OCRv6 模型 + ONNX Runtime）：
/// 完全离线免费无限量；首次使用自动下载模型（约15MB）到 exe 同目录 models/
fn rapidocr_ocr_pixels(
    data: &ScreenshotData,
    corrections: &[(String, String)],
) -> Result<OcrResult, String> {
    use rapidocr_core::config::{PipelineConfig, RapidOcrConfig};
    use std::sync::Mutex;
    static RAPIDOCR: Mutex<Option<rapidocr_core::RapidOcr>> = Mutex::new(None);

    let mut guard = RAPIDOCR.lock().map_err(|_| "RapidOCR锁不可用".to_string())?;
    if guard.is_none() {
        // 模型缓存目录：与 settings.json 同目录的 models/
        let exe_dir = std::env::current_exe().unwrap_or_default();
        let models_dir = exe_dir
            .parent()
            .map(|p| p.join("models"))
            .unwrap_or_else(|| std::path::PathBuf::from("models"));
        let model_path = rapidocr_core::model::ensure_ppocrv6_small_models(&models_dir)
            .map_err(|e| {
                format!("RapidOCR模型准备失败（首次使用需联网下载约15MB）: {e}")
            })?;
        let cfg = RapidOcrConfig::ppocr_v6_small(&model_path)
            .with_pipeline(PipelineConfig {
                use_det: true,
                use_cls: false, // 截图场景无旋转文本，关闭方向分类提速
                use_rec: true,
            });
        *guard = Some(
            rapidocr_core::RapidOcr::new(cfg)
                .map_err(|e| format!("RapidOCR初始化失败: {e}"))?,
        );
    }
    let ocr = guard.as_mut().unwrap();

    // BGRA → RGB（丢弃alpha）
    let mut rgb: Vec<u8> = Vec::with_capacity(data.pixels.len() / 4 * 3);
    for px in data.pixels.chunks_exact(4) {
        rgb.push(px[2]);
        rgb.push(px[1]);
        rgb.push(px[0]);
    }
    let img = image::RgbImage::from_raw(data.width as u32, data.height as u32, rgb)
        .ok_or_else(|| "图像数据无效".to_string())?;
    let out = ocr.run_image(&img).map_err(|e| format!("RapidOCR识别失败: {e}"))?;

    let mut lines = Vec::new();
    for l in out.lines {
        let text = win::apply_symbol_corrections(&l.text, corrections);
        if text.trim().is_empty() {
            continue;
        }
        let (mut min_x, mut min_y, mut max_x, mut max_y) =
            (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
        for p in l.bbox.points.iter() {
            min_x = min_x.min(p[0] as f64);
            min_y = min_y.min(p[1] as f64);
            max_x = max_x.max(p[0] as f64);
            max_y = max_y.max(p[1] as f64);
        }
        lines.push(OcrLineInfo {
            text,
            x: min_x,
            y: min_y,
            width: max_x - min_x,
            height: max_y - min_y,
        });
    }
    Ok(OcrResult {
        lines,
        language: "auto".to_string(),
    })
}

/// 递归收集 JSON 值树中所有同时含 x/y 键的对象作为坐标点（有道OCR boundingBox 解析用）
fn collect_points(v: &serde_json::Value, pts: &mut Vec<(f64, f64)>) {
    match v {
        serde_json::Value::Object(m) => {
            if let (Some(x), Some(y)) = (
                m.get("x").and_then(|v| v.as_f64()),
                m.get("y").and_then(|v| v.as_f64()),
            ) {
                pts.push((x, y));
            }
            for (_, sub) in m {
                collect_points(sub, pts);
            }
        }
        serde_json::Value::Array(a) => {
            for sub in a {
                collect_points(sub, pts);
            }
        }
        _ => {}
    }
}

/// 捕获屏幕指定区域（BGRA像素）
#[tauri::command]
pub fn capture_region(x: i32, y: i32, width: i32, height: i32) -> Result<ScreenshotData, String> {
    #[cfg(target_os = "windows")]
    {
        win::capture_region(x, y, width, height)
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (x, y, width, height);
        Err("当前平台暂不支持截图".to_string())
    }
}

/// 从BGRA像素执行OCR，返回结构化行数据（文本+紧凑矩形，物理像素）
#[tauri::command]
pub fn perform_ocr(pixels: Vec<u8>, width: i32, height: i32) -> Result<OcrResult, String> {
    #[cfg(target_os = "windows")]
    {
        win::ocr_from_pixels(&pixels, width, height, &[])
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (pixels, width, height);
        Err("当前平台暂不支持OCR".to_string())
    }
}

/// 截图（兼容旧命令）：占位
#[tauri::command]
pub async fn capture_screenshot() -> Result<Vec<u8>, String> {
    Ok(Vec::new())
}

/// 获取提取按钮信息（兼容旧命令）
#[tauri::command]
pub async fn get_extract_button_info() -> Result<ExtractButton, String> {
    Ok(ExtractButton {
        x: 100.0,
        y: 200.0,
        width: 120.0,
        height: 40.0,
        opacity: 0.1,
        visible: true,
        draggable: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_button_below_selection() {
        let selection = ScreenshotRegion {
            x: 100,
            y: 100,
            width: 300,
            height: 200,
        };
        let screen = (0, 0, 1920, 1080);
        let (x, y) = calculate_extract_button_position(&selection, &screen);
        assert_eq!(x, 190.0);
        assert_eq!(y, 310.0);
    }

    #[test]
    fn test_button_above_bottom_edge_when_overflow() {
        // 选区底边接近屏幕底部：按钮改为底边上方对齐（仍锚定底边）
        let selection = ScreenshotRegion {
            x: 100,
            y: 1000,
            width: 300,
            height: 100,
        };
        let screen = (0, 0, 1920, 1080);
        let (_, y) = calculate_extract_button_position(&selection, &screen);
        // 底边y=1100，上方对齐：1100 - 10 - 40 = 1050
        assert_eq!(y, 1050.0);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn test_regroup_strips_ocr_noise_symbols() {
        // 内置纠错：⺂（→的误识别）还原为→；③ 映射为 3；
        // 未映射符号（♠）原样保留不再剔除，交给翻译引擎处理
        let words = vec![
            win::OcrWordItem { text: "Click".into(), x: 0.0, y: 0.0, width: 40.0, height: 12.0 },
            win::OcrWordItem { text: "\u{2E82}\u{2E82}".into(), x: 44.0, y: 0.0, width: 20.0, height: 12.0 }, // ⺂⺂ → →→
            win::OcrWordItem { text: "here\u{2462}".into(), x: 68.0, y: 0.0, width: 60.0, height: 12.0 }, // here③ → here3
            win::OcrWordItem { text: "\u{2660}".into(), x: 132.0, y: 0.0, width: 10.0, height: 12.0 }, // ♠ 未映射，透传
        ];
        let lines = win::regroup_words_to_lines(words, &[]);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].text, "Click→→here3♠");
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn test_regroup_term_symbol_correction() {
        // 术语管理符号映射：乛→→（用户在术语库中穷举维护）
        let words = vec![
            win::OcrWordItem { text: "Open".into(), x: 0.0, y: 0.0, width: 40.0, height: 12.0 },
            win::OcrWordItem { text: "乛".into(), x: 44.0, y: 0.0, width: 10.0, height: 12.0 },
            win::OcrWordItem { text: "Save".into(), x: 58.0, y: 0.0, width: 40.0, height: 12.0 },
        ];
        let corrections = vec![("乛".to_string(), "→".to_string())];
        let lines = win::regroup_words_to_lines(words, &corrections);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].text, "Open→Save");
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn test_regroup_spacer_bridges_gap() {
        // 行中的符号词不再被剔除（符号透传）：连续文本保持一行不拆
        let words = vec![
            win::OcrWordItem { text: "第一部分".into(), x: 0.0, y: 0.0, width: 60.0, height: 20.0 },
            win::OcrWordItem { text: "@".into(), x: 70.0, y: 0.0, width: 30.0, height: 20.0 },
            win::OcrWordItem { text: "第二部分".into(), x: 110.0, y: 0.0, width: 60.0, height: 20.0 },
        ];
        let lines = win::regroup_words_to_lines(words, &[]);
        assert_eq!(lines.len(), 1, "符号词透传，不应拆分");
        assert_eq!(lines[0].text, "第一部分@第二部分");
        assert!((lines[0].width - 170.0).abs() < 0.01);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn test_regroup_splits_wide_horizontal_gaps() {
        // 同一行左右间隔过大（>1.5×行高）：拆成两条独立行（表格两列/标签+值场景）
        let words = vec![
            win::OcrWordItem { text: "Name".into(), x: 0.0, y: 0.0, width: 40.0, height: 12.0 },
            win::OcrWordItem { text: "Value".into(), x: 200.0, y: 0.0, width: 40.0, height: 12.0 },
        ];
        let lines = win::regroup_words_to_lines(words, &[]);
        assert_eq!(lines.len(), 2, "水平间隔160 > 1.5×12，应拆为两行");
        assert_eq!(lines[0].text, "Name");
        assert_eq!(lines[1].text, "Value");

        // 正常句子内的小间隔（词间空隙）不拆分
        let words = vec![
            win::OcrWordItem { text: "hello".into(), x: 0.0, y: 0.0, width: 40.0, height: 12.0 },
            win::OcrWordItem { text: "world".into(), x: 48.0, y: 0.0, width: 40.0, height: 12.0 },
        ];
        let lines = win::regroup_words_to_lines(words, &[]);
        assert_eq!(lines.len(), 1, "间隔8 < 1.5×12，应保持一行");
        assert_eq!(lines[0].text, "hello world");
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn test_regroup_horizontal_words_split_into_lines() {
        // 模拟OCR把一行横排文字拆成3个"行"（同y范围、不同x）
        // 词级重组应合并为1行，按x排序拼接
        let words = vec![
            win::OcrWordItem { text: "hello".into(), x: 0.0, y: 10.0, width: 40.0, height: 12.0 },
            win::OcrWordItem { text: "beautiful".into(), x: 44.0, y: 10.5, width: 60.0, height: 12.0 },
            win::OcrWordItem { text: "world".into(), x: 106.0, y: 10.2, width: 35.0, height: 12.0 },
        ];
        let lines = win::regroup_words_to_lines(words, &[]);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].text, "hello beautiful world");
        assert!((lines[0].width - 141.0).abs() < 0.01);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn test_regroup_two_separate_lines() {
        // 两行垂直排列的文字（y不重叠）应保持2行
        let words = vec![
            win::OcrWordItem { text: "line".into(), x: 0.0, y: 0.0, width: 30.0, height: 12.0 },
            win::OcrWordItem { text: "one".into(), x: 32.0, y: 0.0, width: 24.0, height: 12.0 },
            win::OcrWordItem { text: "line".into(), x: 0.0, y: 30.0, width: 30.0, height: 12.0 },
            win::OcrWordItem { text: "two".into(), x: 32.0, y: 30.0, width: 24.0, height: 12.0 },
        ];
        let lines = win::regroup_words_to_lines(words, &[]);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].text, "line one");
        assert_eq!(lines[1].text, "line two");
    }
}