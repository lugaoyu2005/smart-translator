//! 截图与OCR模块
//! - GDI截图：捕获屏幕指定区域（BGRA像素）
//! - OCR：Windows内置OCR引擎（Windows.Media.Ocr，支持中英文，无需外部安装）
//! - 结构化返回：逐行文本 + 紧凑边界矩形（物理像素），供前端做段落判定与1:1覆盖
//! - 小区域自动放大 + 词级行重组：修复小框选识别率低、横排误判竖排的问题

use serde::{Deserialize, Serialize};

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
    pub(crate) fn regroup_words_to_lines(words: Vec<OcrWordItem>) -> Vec<OcrLineInfo> {
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
            let mut text = String::new();
            for (i, w) in line.iter().enumerate() {
                if i > 0 {
                    let prev = &line[i - 1];
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

            let min_x = line.iter().map(|w| w.x).fold(f64::MAX, f64::min);
            let min_y = line.iter().map(|w| w.y).fold(f64::MAX, f64::min);
            let max_x = line.iter().map(|w| w.x + w.width).fold(f64::MIN, f64::max);
            let max_y = line
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
        result
    }

    /// Windows内置OCR：从BGRA像素识别文字
    /// 返回结构化行数据（文本+紧凑矩形，物理像素，相对截图区域原点）
    /// 优先中文引擎（zh-Hans-CN），fallback到用户配置文件语言
    pub fn ocr_from_pixels(pixels: &[u8], width: i32, height: i32) -> Result<OcrResult, String> {
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
        let lines = regroup_words_to_lines(all_words);

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
            win::ocr_from_pixels(&data.pixels, data.width, data.height)
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
        win::ocr_from_pixels(&pixels, width, height)
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
    fn test_regroup_horizontal_words_split_into_lines() {
        // 模拟OCR把一行横排文字拆成3个"行"（同y范围、不同x）
        // 词级重组应合并为1行，按x排序拼接
        let words = vec![
            win::OcrWordItem { text: "hello".into(), x: 0.0, y: 10.0, width: 40.0, height: 12.0 },
            win::OcrWordItem { text: "beautiful".into(), x: 44.0, y: 10.5, width: 60.0, height: 12.0 },
            win::OcrWordItem { text: "world".into(), x: 106.0, y: 10.2, width: 35.0, height: 12.0 },
        ];
        let lines = win::regroup_words_to_lines(words);
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
        let lines = win::regroup_words_to_lines(words);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].text, "line one");
        assert_eq!(lines[1].text, "line two");
    }
}