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
    if enabled("custom") {
        engines.push(Box::new(OpenAICompatEngine::new(
            &providers.custom_openai_base_url,
            &providers.custom_openai_api_key,
            &providers.custom_openai_model,
        )));
    }
    // 离线翻译（OPUS-MT 本地模型）排在末尾：在线引擎优先，全部失败时自动兜底；
    // 截图菜单/当前翻译源也可手动选中
    if settings.offline_engine != "disabled" {
        engines.push(Box::new(crate::offline_mt::OfflineMtEngine::new()));
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

// Tauri命令：翻译文本（先预处理，再走引擎管理器）
#[tauri::command]
pub async fn translate_text(
    state: tauri::State<'_, AppState>,
    text: String,
    from: String,
    to: String,
) -> Result<TranslationResult, String> {
    // 预处理：处理 MiniMap / Mini_Map 等格式
    let processed = preprocess_text(&text);

    // 空文本直接返回
    if processed.is_empty() {
        return Err("请输入要翻译的内容".to_string());
    }

    let mut manager = state.manager.lock().await;

    let result = manager.translate(&processed, &from, &to).await;

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

    let mut manager = state.manager.lock().await;
    let (translations, engine_used) = match engine {
        Some(name) if !name.trim().is_empty() => {
            manager
                .translate_batch_with(name.trim(), &processed, &from, &to)
                .await?
        }
        _ => manager.translate_batch(&processed, &from, &to).await?,
    };

    // 术语库使用计数有自动调整时落盘
    if manager.take_term_dirty() {
        let _ = crate::terms::save_term_base(manager.term_base());
    }

    // 记录翻译历史（整批合并为一条）
    crate::history::record(
        &from,
        &to,
        &lines.join("\n"),
        &translations.join("\n"),
        &engine_used,
    );

    Ok(LinesTranslationResult {
        translations,
        engine_used,
    })
}
