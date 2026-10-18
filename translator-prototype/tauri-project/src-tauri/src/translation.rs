//! Tauri命令层：暴露翻译功能给前端
//! 状态管理：TranslationManager 存入 Tauri 状态，跨命令共享

use crate::engines::{
    preprocess_text, AliEngine, BaiduEngine, DeepLEngine, EngineInfo, NiutransEngine,
    OpenAICompatEngine, TencentEngine, TranslationManager, TranslationResult, YoudaoEngine,
};
use serde::{Deserialize, Serialize};

/// Tauri 管理的全局状态
pub struct AppState {
    pub manager: tokio::sync::Mutex<TranslationManager>,
}

/// 从设置中的API密钥构建引擎管理器（术语库从 terms.json 加载）；
/// 只构建「在线翻译引擎」多选框中启用的引擎（优先级=列表顺序），与截图菜单双向同步。
/// 已接入：百度/有道/腾讯云/阿里云/小牛/DeepL/自定义OpenAI兼容
pub fn build_manager(settings: &crate::system::AppSettings) -> TranslationManager {
    let enabled = |id: &str| settings.online_apis.iter().any(|s| s == id);
    let providers = &settings.providers;
    let mut engines: Vec<Box<dyn crate::engines::TranslationEngine>> = Vec::new();

    if enabled("baidu") {
        engines.push(Box::new(BaiduEngine::new(
            &settings.baidu_app_id,
            &settings.baidu_secret,
        )));
    }
    if enabled("youdao") {
        engines.push(Box::new(YoudaoEngine::new(
            &settings.youdao_app_key,
            &settings.youdao_app_secret,
        )));
    }
    if enabled("tencent") {
        engines.push(Box::new(TencentEngine::new(
            &providers.tencent_secret_id,
            &providers.tencent_secret_key,
            &providers.tencent_region,
        )));
    }
    if enabled("ali") {
        engines.push(Box::new(AliEngine::new(
            &providers.ali_access_key_id,
            &providers.ali_access_key_secret,
        )));
    }
    if enabled("niutrans") {
        engines.push(Box::new(NiutransEngine::new(
            &providers.niutrans_api_key,
        )));
    }
    if enabled("deepl") {
        engines.push(Box::new(DeepLEngine::new(&providers.deepl_api_key)));
    }
    // 多自定义 OpenAI 兼容供应商（每个独立引擎；旧单供应商字段向后兼容）
    if settings.custom_providers.is_empty() && !providers.custom_openai_base_url.is_empty() {
        engines.push(Box::new(OpenAICompatEngine::new(
            &providers.custom_openai_base_url,
            &providers.custom_openai_api_key,
            &providers.custom_openai_model,
        )));
    }
    // 引擎显示名只保留供应商名（用户要求：界面上不要"自定义AI-"这类前缀）。
    // 设置页会拒绝重名，这里再做一次兜底去重，避免引擎名冲突导致 current_engine 指向不明。
    let mut used_names: Vec<String> = engines.iter().map(|e| e.name().to_string()).collect();
    for cp in &settings.custom_providers {
        if cp.base_url.trim().is_empty() {
            continue;
        }
        let base = if cp.name.trim().is_empty() {
            "自定义AI".to_string()
        } else {
            cp.name.trim().to_string()
        };
        let mut display = base.clone();
        let mut n = 2usize;
        while used_names.iter().any(|u| u == &display) {
            display = format!("{base}-{n}");
            n += 1;
        }
        used_names.push(display.clone());
        engines.push(Box::new(OpenAICompatEngine::new_named(
            &display, &cp.base_url, &cp.api_key, &cp.model,
        )));
    }
    // 离线翻译（OPUS-MT 本地模型）排在末尾：在线引擎优先，全部失败时自动兜底；
    // 截图菜单/当前翻译源也可手动选中
    if settings.offline_engine != "disabled" {
        engines.push(Box::new(crate::offline_mt::OfflineMtEngine::with_model(
            &settings.offline_model,
        )));
    }

    let term_base = crate::terms::load_term_base();
    TranslationManager::new(engines, term_base)
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Language {
    pub code: String,
    pub name: String,
    pub native_name: String,
}

// ============ 双向自动翻译（原文语种决定目标语种） ============

/// 按字符脚本粗判语种：本地启发式，零网络开销。
/// 只需要区分"这一对语言里的哪一个是原文"，不需要精确的语言识别。
fn detect_lang_by_script(text: &str) -> &'static str {
    let (mut han, mut kana, mut hangul, mut cyr, mut latin) = (0usize, 0usize, 0usize, 0usize, 0usize);
    for c in text.chars() {
        let u = c as u32;
        if (0x4E00..=0x9FFF).contains(&u)
            || (0x3400..=0x4DBF).contains(&u)
            || (0xF900..=0xFAFF).contains(&u)
        {
            han += 1;
        } else if (0x3040..=0x30FF).contains(&u) {
            kana += 1;
        } else if (0xAC00..=0xD7A3).contains(&u) || (0x1100..=0x11FF).contains(&u) {
            hangul += 1;
        } else if (0x0400..=0x04FF).contains(&u) {
            cyr += 1;
        } else if c.is_ascii_alphabetic() || (0xC0..=0x24F).contains(&u) {
            latin += 1;
        }
    }
    // 判定优先级（不是"谁字数多"）：
    // 1. 假名/谚文/西里尔各自的语言专属性强，出现即判定
    // 2. **拉丁字母优先于汉字**：中英混排（如"这是 MiniMap 测试"）按英文原文处理，
    //    目标为中文——即用户要求的"有英有中，默认英翻中"。
    //    纯中文（无任何拉丁字母）才判为中文原文，目标英文。
    if kana > 0 {
        return "ja";
    }
    if hangul > 0 {
        return "ko";
    }
    if cyr > 0 {
        return "ru";
    }
    if latin > 0 {
        return "en";
    }
    if han > 0 {
        return "zh";
    }
    ""
}

fn parse_bilingual_pair(pair: &str) -> (String, String) {
    let mut it = pair
        .split(['-', ',', '>'])
        .map(|s| s.trim())
        .filter(|s| !s.is_empty());
    let a = it.next().unwrap_or("zh").to_string();
    let b = it.next().unwrap_or("en").to_string();
    (a, b)
}


/// 该文本是否"已经是目标语言"（用于跳过无需翻译的块）。
/// 判据是"纯目标语言"：目标语种的字符必须出现，且不含其它语种的字符——
/// 混排文本（如"这是 MiniMap 测试"）仍需翻译，不算已是目标语言。
/// 只对能靠字符判定的语种生效（zh/ja/ko/ru/en）；fr/es/de/pt 等一律视为需要翻译。
fn is_pure_lang(text: &str, lang: &str) -> bool {
    let (mut han, mut kana, mut hangul, mut cyr, mut latin) = (0usize, 0usize, 0usize, 0usize, 0usize);
    for c in text.chars() {
        let u = c as u32;
        if (0x4E00..=0x9FFF).contains(&u)
            || (0x3400..=0x4DBF).contains(&u)
            || (0xF900..=0xFAFF).contains(&u)
        {
            han += 1;
        } else if (0x3040..=0x30FF).contains(&u) {
            kana += 1;
        } else if (0xAC00..=0xD7A3).contains(&u) || (0x1100..=0x11FF).contains(&u) {
            hangul += 1;
        } else if (0x0400..=0x04FF).contains(&u) {
            cyr += 1;
        } else if c.is_ascii_alphabetic() || (0xC0..=0x24F).contains(&u) {
            latin += 1;
        }
    }
    match lang {
        "zh" => han > 0 && latin == 0 && kana == 0 && hangul == 0 && cyr == 0,
        "ja" => kana > 0 && latin == 0 && hangul == 0 && cyr == 0,
        "ko" => hangul > 0 && latin == 0 && kana == 0 && cyr == 0,
        "ru" => cyr > 0 && latin == 0 && han == 0 && kana == 0 && hangul == 0,
        "en" => latin > 0 && han == 0 && kana == 0 && hangul == 0 && cyr == 0,
        _ => false,
    }
}

/// 把目标语种 "auto" 解析成具体语种：原文是 A → 目标 B，原文是 B → 目标 A。
/// 检测不出（或不在配置的语言对里）时翻成语言对的第一个（默认中文，符合中文用户）。
pub fn resolve_target_lang(text: &str, to: &str, pair: &str) -> String {
    if !to.eq_ignore_ascii_case("auto") {
        return to.to_string();
    }
    let (a, b) = parse_bilingual_pair(pair);
    let detected = detect_lang_by_script(text);
    if detected == a {
        b
    } else if detected == b {
        a
    } else {
        a
    }
}

// Tauri命令：翻译文本（先预处理，再走引擎管理器）
#[tauri::command]
pub async fn translate_text(
    state: tauri::State<'_, AppState>,
    text: String,
    from: String,
    to: String,
    engine: Option<String>,
) -> Result<TranslationResult, String> {
    // 预处理：处理 MiniMap / Mini_Map 等格式
    let processed = preprocess_text(&text);

    // 空文本直接返回
    if processed.is_empty() {
        return Err("请输入要翻译的内容".to_string());
    }

    // 双向自动：目标语种为 auto 时按原文语种反向决定（中文→英文 / 英文→中文）
    let settings = crate::system::load_settings();
    let to = resolve_target_lang(&processed, &to, &settings.bilingual_pair);

    let mut manager = state.manager.lock().await;

    // 指定引擎（翻译测试页手动选引擎）；None=按优先级自动
    let result = if let Some(name) = engine.as_deref().filter(|s| !s.trim().is_empty()) {
        manager
            .translate_with(name.trim(), &processed, &from, &to)
            .await
    } else {
        manager.translate(&processed, &from, &to).await
    };

    // 术语库使用计数有自动调整时落盘
    if manager.take_term_dirty() {
        let _ = crate::terms::save_term_base(manager.term_base());
    }

    // 记录翻译历史
    if let Ok(r) = &result {
        crate::history::record(
            &from,
            &to,
            &processed,
            &r.translated_text,
            &r.engine_used,
        );
    }

    result.map(|mut r| {
        // 返回的from/to记录实际请求的语言
        r.from = from.clone();
        r.to = to.clone();
        r
    })
}

// Tauri命令：获取支持的语言列表
#[tauri::command]
pub async fn get_supported_languages() -> Result<Vec<Language>, String> {
    Ok(vec![
        Language {
            code: "auto".to_string(),
            name: "Auto Detect".to_string(),
            native_name: "自动检测".to_string(),
        },
        Language {
            code: "zh".to_string(),
            name: "Chinese".to_string(),
            native_name: "中文".to_string(),
        },
        Language {
            code: "en".to_string(),
            name: "English".to_string(),
            native_name: "English".to_string(),
        },
        Language {
            code: "ja".to_string(),
            name: "Japanese".to_string(),
            native_name: "日本語".to_string(),
        },
        Language {
            code: "ko".to_string(),
            name: "Korean".to_string(),
            native_name: "한국어".to_string(),
        },
        Language {
            code: "ru".to_string(),
            name: "Russian".to_string(),
            native_name: "Русский".to_string(),
        },
        Language {
            code: "fr".to_string(),
            name: "French".to_string(),
            native_name: "Français".to_string(),
        },
        Language {
            code: "de".to_string(),
            name: "German".to_string(),
            native_name: "Deutsch".to_string(),
        },
        Language {
            code: "es".to_string(),
            name: "Spanish".to_string(),
            native_name: "Español".to_string(),
        },
        Language {
            code: "pt".to_string(),
            name: "Portuguese".to_string(),
            native_name: "Português".to_string(),
        },
    ])
}

// Tauri命令：列出可用引擎
#[tauri::command]
pub async fn list_engines(state: tauri::State<'_, AppState>) -> Result<Vec<EngineInfo>, String> {
    let manager = state.manager.lock().await;
    Ok(manager.list_engines())
}

// Tauri命令：重新加载引擎（保存API密钥设置后调用）
#[tauri::command]
pub async fn reload_engines(state: tauri::State<'_, AppState>) -> Result<(), String> {
    let settings = crate::system::load_settings();
    let manager = build_manager(&settings);
    let mut guard = state.manager.lock().await;
    *guard = manager;
    Ok(())
}

// Tauri命令：添加/更新术语优先级（并持久化到 terms.json）
#[tauri::command]
pub async fn set_term_priority(
    state: tauri::State<'_, AppState>,
    source: String,
    translation: String,
    priority: u32,
) -> Result<(), String> {
    let mut manager = state.manager.lock().await;
    manager.set_term_priority(&source, &translation, priority);
    crate::terms::save_term_base(manager.term_base())
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LinesTranslationResult {
    pub translations: Vec<String>,
    pub engine_used: String,
    /// 与 lines 等长：true = 该组本来就是目标语言，未翻译（前端不铺覆盖块）
    #[serde(default)]
    pub skipped: Vec<bool>,
}

// Tauri命令：批量翻译（截图翻译用）
// lines：每个元素是一组文本（可含\n多行）；engine：可选指定引擎名（手动切换翻译源）
// 百度引擎会合并为一次请求按行拆分（规避免费版1QPS限速），其他引擎逐行
#[tauri::command]
pub async fn translate_lines(
    state: tauri::State<'_, AppState>,
    lines: Vec<String>,
    from: String,
    to: String,
    engine: Option<String>,
) -> Result<LinesTranslationResult, String> {
    // 预处理：MiniMap / Mini_Map 等
    let processed: Vec<String> = lines
        .iter()
        .map(|l| {
            let p = preprocess_text(l);
            if p.is_empty() {
                l.trim().to_string()
            } else {
                p
            }
        })
        .collect();

    // ===== 语言决策 =====
    // 原文语言不再由用户指定（菜单栏已去掉"原文对象"），一律交给引擎自动检测；
    // 译文语言由用户手选。目标为 "auto" 时按每块原文反向（双默认模式）。
    let settings = crate::system::load_settings();
    let fixed_target = if to.eq_ignore_ascii_case("auto") {
        None
    } else {
        Some(to.clone())
    };

    // 逐组决定"翻不翻"以及"翻成什么"：
    //  - 目标语言固定时：已经是该语言的块直接跳过（只翻译原文部分）
    //  - 目标为 auto 时：每块按自身语言的相反语种翻（中→英 / 英→中）
    let mut targets: Vec<String> = Vec::with_capacity(processed.len());
    let mut skipped: Vec<bool> = vec![false; processed.len()];
    let mut pending: Vec<usize> = Vec::new();
    for (i, line) in processed.iter().enumerate() {
        let tgt = match &fixed_target {
            Some(t) => t.clone(),
            None => resolve_target_lang(line, "auto", &settings.bilingual_pair),
        };
        // 已是目标语言 → 不翻译（混排块因为不是"纯目标语言"，仍会翻译）
        if fixed_target.is_some() && is_pure_lang(line, &tgt) {
            skipped[i] = true;
            targets.push(tgt);
            continue;
        }
        targets.push(tgt);
        pending.push(i);
    }

    let mut translations: Vec<String> = processed.clone();
    let mut engine_used = String::new();
    let mut manager = state.manager.lock().await;
    if !pending.is_empty() {
        // 同一批里目标语言必须一致才能合并请求；auto 模式下可能不一致，按目标分组翻译
        let mut batches: Vec<(String, Vec<usize>)> = Vec::new();
        for &i in &pending {
            match batches.iter_mut().find(|(t, _)| *t == targets[i]) {
                Some((_, idxs)) => idxs.push(i),
                None => batches.push((targets[i].clone(), vec![i])),
            }
        }
        for (tgt, idxs) in batches {
            let texts: Vec<String> = idxs.iter().map(|&i| processed[i].clone()).collect();
            let (res, used) = match engine {
                Some(ref name) if !name.trim().is_empty() => {
                    manager
                        .translate_batch_with(name.trim(), &texts, &from, &tgt)
                        .await?
                }
                _ => manager.translate_batch(&texts, &from, &tgt).await?,
            };
            if engine_used.is_empty() {
                engine_used = used;
            }
            for (k, &i) in idxs.iter().enumerate() {
                translations[i] = res.get(k).cloned().unwrap_or_default();
            }
        }
    }

    // 术语库使用计数有自动调整时落盘
    if manager.take_term_dirty() {
        let _ = crate::terms::save_term_base(manager.term_base());
    }

    // 记录翻译历史（整批合并为一条；跳过翻译时用实际目标语言）
    let history_to = targets.first().cloned().unwrap_or_else(|| to.clone());
    if !pending.is_empty() {
        crate::history::record(
            &from,
            &history_to,
            &lines.join("\n"),
            &translations.join("\n"),
            &engine_used,
        );
    }

    Ok(LinesTranslationResult {
        translations,
        engine_used,
        skipped,
    })
}

#[cfg(test)]
mod tests {
    // ==================================================================
    // 集成/验收测试：build_manager 组装逻辑（设置→引擎列表→优先级）
    // 前缀过滤：cargo test <前缀>
    //   integration_  集成测试
    //   acceptance_   验收测试
    //   sanity_       健全性测试
    // ==================================================================
    use super::*;
    use crate::system::AppSettings;

    #[test]
    fn is_pure_lang_skips_only_unmixed_target_text() {
        // 纯中文：已是中文目标 → 跳过
        assert!(is_pure_lang("游戏崩溃了", "zh"));
        // 中英混排：不算已是中文 → 仍需翻译
        assert!(!is_pure_lang("这是 MiniMap 测试", "zh"));
        // 纯英文：目标英文 → 跳过
        assert!(is_pure_lang("Hello world", "en"));
        // 英文里夹中文：不算纯英文
        assert!(!is_pure_lang("Hello 世界", "en"));
        // 标点数字：两种语言都不算"纯"
        assert!(!is_pure_lang("12345!!", "zh"));
        assert!(!is_pure_lang("12345!!", "en"));
        // 无法靠字符判定的语种（法语）一律需要翻译
        assert!(!is_pure_lang("Bonjour le monde", "fr"));
    }

    #[test]
    fn bilingual_auto_resolves_target_by_source_script() {
        // 双向自动：中文原文 → 目标英文
        assert_eq!(resolve_target_lang("你好世界", "auto", "zh-en"), "en");
        // 英文原文 → 目标中文
        assert_eq!(resolve_target_lang("Hello world", "auto", "zh-en"), "zh");
        // 中英混排 → 按英文原文处理 → 目标中文（用户明确要求：有英有中，默认英翻中）
        assert_eq!(resolve_target_lang("这是 MiniMap 测试", "auto", "zh-en"), "zh");
        assert_eq!(resolve_target_lang("游戏崩溃了 please restart", "auto", "zh-en"), "zh");
        // 纯中文（无拉丁字母） → 目标英文
        assert_eq!(resolve_target_lang("游戏崩溃了", "auto", "zh-en"), "en");
        // 假名 → 日语，不在 zh-en 对里 → 回落中文
        assert_eq!(resolve_target_lang("こんにちは", "auto", "zh-en"), "zh");
        // 非 auto 原样返回
        assert_eq!(resolve_target_lang("你好", "ja", "zh-en"), "ja");
        // 其他语言对
        assert_eq!(resolve_target_lang("中文", "auto", "zh-ja"), "ja");
        assert_eq!(resolve_target_lang("こんにちは", "auto", "zh-ja"), "zh");
        assert_eq!(resolve_target_lang("Привет", "auto", "zh-ru"), "zh");
        // 纯符号/数字：检测不出 → 回落语言对第一个
        assert_eq!(resolve_target_lang("12345", "auto", "zh-en"), "zh");
    }

    #[test]
    fn integration_default_settings_build_offline_only() {
        // 默认设置：百度/有道启用但无密钥（构建时被过滤）→ 仅剩离线引擎兜底
        let settings = AppSettings::default();
        let m = build_manager(&settings);
        let names: Vec<String> = m.list_engines().iter().map(|e| e.name.clone()).collect();
        assert_eq!(names, vec!["离线翻译".to_string()]);
    }

    #[test]
    fn regression_engine_manager_rebuilt_after_key_save() {
        // 回归（v0.4.15）：保存 API 密钥后必须立即重建引擎管理器。
        // save_app_settings 现在会 build_manager 重建，否则填好密钥的在线引擎
        // 要重启应用才会出现在截图菜单/翻译测试的引擎列表里。
        let mut settings = AppSettings::default();
        // 保存前：百度/有道已启用但密钥为空 → 构建时被 is_configured 过滤，只剩离线兜底
        let before: Vec<String> = build_manager(&settings)
            .list_engines()
            .iter()
            .map(|e| e.name.clone())
            .collect();
        assert_eq!(before, vec!["离线翻译".to_string()]);

        // 用户填好密钥保存后：用同一份设置重建 → 在线引擎必须立即出现
        settings.baidu_app_id = "app-id".to_string();
        settings.baidu_secret = "secret".to_string();
        settings.youdao_app_key = "key".to_string();
        settings.youdao_app_secret = "secret".to_string();
        assert_eq!(
            build_manager(&settings)
                .list_engines()
                .iter()
                .map(|e| e.name.clone())
                .collect::<Vec<String>>(),
            vec![
                "百度翻译".to_string(),
                "有道智云".to_string(),
                "离线翻译".to_string()
            ],
            "重建后在线引擎必须立即可用"
        );
        // configured 过滤后仍必须非空（截图菜单/翻译测试都按 configured 过滤）
        let configured = build_manager(&settings)
            .list_engines()
            .into_iter()
            .filter(|e| e.configured)
            .count();
        assert_eq!(configured, 3);
    }

    #[test]
    fn integration_disabled_offline_builds_empty() {
        let mut settings = AppSettings::default();
        settings.offline_engine = "disabled".to_string();
        let m = build_manager(&settings);
        assert!(m.list_engines().is_empty(), "全部禁用+无密钥时引擎列表应为空");
    }

    #[test]
    fn integration_legacy_custom_provider_becomes_engine() {
        // 旧版单供应商字段向后兼容 → "自定义AI"
        let mut settings = AppSettings::default();
        settings.offline_engine = "disabled".to_string();
        settings.providers.custom_openai_base_url = "https://api.example.com/v1".to_string();
        settings.providers.custom_openai_api_key = "sk-test".to_string();
        settings.providers.custom_openai_model = "gpt-x".to_string();
        let m = build_manager(&settings);
        let names: Vec<String> = m.list_engines().iter().map(|e| e.name.clone()).collect();
        assert_eq!(names, vec!["自定义AI".to_string()]);
    }

    #[test]
    fn integration_named_custom_providers() {
        // 多自定义供应商：每个生成独立引擎；空 base_url 跳过
        let mut settings = AppSettings::default();
        settings.offline_engine = "disabled".to_string();
        settings.custom_providers = vec![
            crate::system::CustomProvider {
                id: "p1".into(),
                name: "DeepSeek".into(),
                base_url: "https://a.example.com/v1".into(),
                api_key: "k".into(),
                model: "m".into(),
            },
            crate::system::CustomProvider {
                id: "p2".into(),
                name: String::new(),
                base_url: "https://b.example.com/v1".into(),
                api_key: "k".into(),
                model: "m".into(),
            },
            crate::system::CustomProvider {
                id: "p3".into(),
                name: "无地址".into(),
                base_url: String::new(),
                api_key: "k".into(),
                model: "m".into(),
            },
        ];
        let m = build_manager(&settings);
        let names: Vec<String> = m.list_engines().iter().map(|e| e.name.clone()).collect();
        assert_eq!(
            names,
            vec!["DeepSeek".to_string(), "自定义AI".to_string()],
            "空名回落'自定义AI'，空base_url被跳过"
        );
    }

    #[tokio::test]
    async fn acceptance_language_list_complete() {
        // 验收：语言列表含自动检测+9种语言，代码唯一、本地化名非空
        let langs = get_supported_languages().await.unwrap();
        assert_eq!(langs.len(), 10);
        let mut codes: Vec<String> = langs.iter().map(|l| l.code.clone()).collect();
        codes.sort();
        let n = codes.len();
        codes.dedup();
        assert_eq!(codes.len(), n, "语言代码必须唯一");
        assert!(langs.iter().all(|l| !l.native_name.is_empty() && !l.name.is_empty()));
        assert_eq!(langs[0].code, "auto");
    }

    #[test]
    fn sanity_preprocess_lines_fallback() {
        // translate_lines 的预处理回退：MiniMap分词；空结果回退trim原文
        let lines = vec!["MiniMap".to_string(), "   ".to_string()];
        let processed: Vec<String> = lines
            .iter()
            .map(|l| {
                let p = preprocess_text(l);
                if p.is_empty() {
                    l.trim().to_string()
                } else {
                    p
                }
            })
            .collect();
        assert_eq!(processed, vec!["Mini Map", ""]);
    }
}
