//! 翻译引擎框架：统一管理离线/在线多引擎
//! - 引擎抽象：trait TranslationEngine
//! - 具体引擎：百度翻译、有道智云、Marian离线（后续接入）
//! - 管理器：根据网络状态与可用性自动选择引擎

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

// ============ 基础类型 ============

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum EngineType {
    Offline, // 离线引擎（Marian/NMT）
    Baidu,   // 百度翻译
    Youdao,  // 有道智云
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EngineInfo {
    pub name: String,
    pub engine_type: EngineType,
    pub available: bool,
    pub configured: bool, // 是否已配置密钥/模型
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TranslationResult {
    pub translated_text: String,
    pub engine_used: String,
    pub from: String,
    pub to: String,
}

// ============ 引擎抽象 ============

/// 翻译引擎统一接口
#[async_trait::async_trait]
pub trait TranslationEngine: Send + Sync {
    /// 引擎名称
    fn name(&self) -> &str;
    /// 引擎类型
    fn engine_type(&self) -> EngineType;
    /// 是否已配置（密钥/模型就绪）
    fn is_configured(&self) -> bool;
    /// 执行翻译
    async fn translate(&self, text: &str, from: &str, to: &str) -> Result<String, String>;
}

// ============ 语言代码映射 ============

/// 将内部语言代码转换为百度/有道各自的语言代码
fn lang_code(engine: &EngineType, code: &str) -> String {
    match engine {
        EngineType::Baidu => match code {
            "zh" => "zh".to_string(),
            "en" => "en".to_string(),
            "ja" => "jp".to_string(), // 百度用 jp
            "ko" => "kor".to_string(), // 百度用 kor
            "es" => "spa".to_string(), // 百度用 spa
            "ru" | "fr" | "de" | "pt" => code.to_string(),
            _ => code.to_string(),
        },
        EngineType::Youdao => match code {
            "zh" => "zh-CHS".to_string(),
            "en" => "en".to_string(),
            "ja" => "ja".to_string(),
            "ko" => "ko".to_string(),
            "ru" | "fr" | "de" | "es" | "pt" => code.to_string(),
            _ => code.to_string(),
        },
        EngineType::Offline => code.to_string(),
    }
}

/// 自动检测语言时的目标语言参数
fn auto_lang_param(engine: &EngineType) -> String {
    match engine {
        EngineType::Baidu => "auto".to_string(),
        EngineType::Youdao => "auto".to_string(),
        EngineType::Offline => "auto".to_string(),
    }
}

// ============ 百度翻译引擎 ============

pub struct BaiduEngine {
    app_id: String,
    secret_key: String,
    client: reqwest::Client,
}

impl BaiduEngine {
    pub fn new(app_id: &str, secret_key: &str) -> Self {
        Self {
            app_id: app_id.to_string(),
            secret_key: secret_key.to_string(),
            client: reqwest::Client::new(),
        }
    }

    fn make_sign(&self, query: &str, salt: &str) -> String {
        let raw = format!("{}{}{}{}", self.app_id, query, salt, self.secret_key);
        format!("{:x}", md5::compute(raw.as_bytes()))
    }
}

#[async_trait::async_trait]
impl TranslationEngine for BaiduEngine {
    fn name(&self) -> &str {
        "百度翻译"
    }

    fn engine_type(&self) -> EngineType {
        EngineType::Baidu
    }

    fn is_configured(&self) -> bool {
        !self.app_id.is_empty() && !self.secret_key.is_empty()
    }

    async fn translate(&self, text: &str, from: &str, to: &str) -> Result<String, String> {
        let salt = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis().to_string())
            .unwrap_or_else(|_| "0".to_string());

        let from_code = if from == "auto" {
            auto_lang_param(&EngineType::Baidu)
        } else {
            lang_code(&EngineType::Baidu, from)
        };
        let to_code = lang_code(&EngineType::Baidu, to);
        let sign = self.make_sign(text, &salt);

        let mut params = HashMap::new();
        params.insert("q", text);
        params.insert("from", &from_code);
        params.insert("to", &to_code);
        params.insert("appid", &self.app_id);
        params.insert("salt", &salt);
        params.insert("sign", &sign);

        let resp = self
            .client
            .get("https://fanyi-api.baidu.com/api/trans/vip/translate")
            .query(&params)
            .send()
            .await
            .map_err(|e| format!("百度翻译请求失败: {e}"))?;

        let status = resp.status();
        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("百度翻译响应解析失败: {e}"))?;

        // 错误检查
        if let Some(err_code) = body.get("error_code").and_then(|v| v.as_str()) {
            if err_code != "52000" {
                let msg = body
                    .get("error_msg")
                    .and_then(|v| v.as_str())
                    .unwrap_or("未知错误");
                return Err(format!("百度翻译错误 [{err_code}]: {msg}"));
            }
        }

        if !status.is_success() {
            return Err(format!("百度翻译HTTP错误: {status}"));
        }

        // 拼接所有翻译结果（按输入行结构保留换行，支持多行批量的按行拆分）
        let texts: Vec<&str> = body
            .get("trans_result")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|item| item.get("dst").and_then(|d| d.as_str()))
                    .collect()
            })
            .ok_or_else(|| "百度翻译响应格式异常".to_string())?;

        Ok(texts.join("\n"))
    }
}

// ============ 有道智云引擎 ============

pub struct YoudaoEngine {
    app_key: String,
    app_secret: String,
    client: reqwest::Client,
}

impl YoudaoEngine {
    pub fn new(app_key: &str, app_secret: &str) -> Self {
        Self {
            app_key: app_key.to_string(),
            app_secret: app_secret.to_string(),
            client: reqwest::Client::new(),
        }
    }

    /// 有道签名：SHA256(appKey + input + salt + curtime + appSecret)
    fn make_sign(&self, input: &str, salt: &str, curtime: &str) -> String {
        use sha2::{Digest, Sha256};
        let raw = format!(
            "{}{}{}{}{}",
            self.app_key, input, salt, curtime, self.app_secret
        );
        let mut hasher = Sha256::new();
        hasher.update(raw.as_bytes());
        format!("{:x}", hasher.finalize())
    }
}

#[async_trait::async_trait]
impl TranslationEngine for YoudaoEngine {
    fn name(&self) -> &str {
        "有道智云"
    }

    fn engine_type(&self) -> EngineType {
        EngineType::Youdao
    }

    fn is_configured(&self) -> bool {
        !self.app_key.is_empty() && !self.app_secret.is_empty()
    }

    async fn translate(&self, text: &str, from: &str, to: &str) -> Result<String, String> {
        // salt必须每次请求唯一（纳秒级）：秒级时间戳在同一秒的多段翻译请求中
        // 会重复，有道判定为重放攻击返回207（签名错误）
        let salt = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos().to_string())
            .unwrap_or_else(|_| "0".to_string());
        let curtime = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs().to_string())
            .unwrap_or_else(|_| "0".to_string());

        let from_code = if from == "auto" {
            auto_lang_param(&EngineType::Youdao)
        } else {
            lang_code(&EngineType::Youdao, from)
        };
        let to_code = lang_code(&EngineType::Youdao, to);

        // 有道v3签名input规则：字符数≤20时 input=q；
        // >20时 input=前10字符 + 总字符数 + 后10字符（必须截断，否则202签名失败）
        let input: String = {
            let chars: Vec<char> = text.chars().collect();
            let n = chars.len();
            if n <= 20 {
                text.to_string()
            } else {
                let first: String = chars[..10].iter().collect();
                let last: String = chars[n - 10..].iter().collect();
                format!("{first}{n}{last}")
            }
        };
        let sign = self.make_sign(&input, &salt, &curtime);

        let mut params = HashMap::new();
        params.insert("q", text);
        params.insert("from", &from_code);
        params.insert("to", &to_code);
        params.insert("appKey", &self.app_key);
        params.insert("salt", &salt);
        params.insert("sign", &sign);
        params.insert("signType", "v3");
        params.insert("curtime", &curtime);

        let resp = self
            .client
            .post("https://openapi.youdao.com/api")
            .form(&params)
            .send()
            .await
            .map_err(|e| format!("有道翻译请求失败: {e}"))?;

        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("有道翻译响应解析失败: {e}"))?;

        if let Some(code) = body.get("errorCode").and_then(|v| v.as_str()) {
            if code != "0" {
                eprintln!(
                    "[youdao-debug] code={code} q={text:?} input={input:?} salt={salt:?} curtime={curtime:?} sign={sign}"
                );
                let msg = match code {
                    "101" => "缺少必填参数",
                    "102" => "不支持的语言类型",
                    "103" => "翻译文本过长",
                    "108" => "应用ID无效（检查有道appKey）",
                    "110" => "无可用次数/余额",
                    "111" => "开发者账号异常",
                    "202" => "签名校验失败（检查有道密钥）",
                    "207" => "签名错误（salt重复或密钥不匹配）",
                    "301" => "词典查询失败",
                    "302" => "翻译查询失败",
                    "303" => "服务异常，请稍后重试",
                    _ => "未知错误",
                };
                return Err(format!("有道翻译失败: {msg}（错误码{code}）"));
            }
        }

        let translations: Vec<&str> = body
            .get("translation")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|t| t.as_str()).collect())
            .ok_or_else(|| "有道翻译响应格式异常".to_string())?;

        Ok(translations.join(""))
    }
}

// ============ 术语库（一词多义优先级） ============

/// 术语条目：一词多义，按优先级排序
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TermEntry {
    pub source: String,        // 源词
    pub translations: Vec<String>, // 多个译法
    pub priority: Vec<u32>,    // 每个译法的优先级（数字越大越优先）
    pub usage_count: u32,      // 使用次数（自动调整依据）
}

impl TermEntry {
    /// 获取当前最优译法（优先级最高的）
    pub fn best_translation(&self) -> Option<&str> {
        if self.translations.is_empty() {
            return None;
        }
        let mut best_idx = 0;
        let mut best_priority = 0u32;
        for (i, &p) in self.priority.iter().enumerate() {
            if p > best_priority {
                best_priority = p;
                best_idx = i;
            }
        }
        self.translations.get(best_idx).map(|x| x.as_str())
    }
}

/// 术语库：支持手动设置优先级 + 使用频率自动调整
#[derive(Debug, Serialize, Deserialize, Default)]
pub struct TermBase {
    pub entries: HashMap<String, TermEntry>,
    /// 是否有未持久化的变更（自动调整计数等），由命令层检查后落盘
    #[serde(skip)]
    pub dirty: bool,
}

impl TermBase {
    /// 手动设置某词某译法的优先级
    pub fn set_priority(&mut self, source: &str, translation: &str, priority: u32) {
        let entry = self.entries.entry(source.to_string()).or_insert(TermEntry {
            source: source.to_string(),
            translations: vec![],
            priority: vec![],
            usage_count: 0,
        });

        if let Some(idx) = entry.translations.iter().position(|t| t == translation) {
            if idx < entry.priority.len() {
                entry.priority[idx] = priority;
            }
        } else {
            entry.translations.push(translation.to_string());
            entry.priority.push(priority);
        }
        self.dirty = true;
    }

    /// 新增译法（不覆盖已有译法的优先级）；返回是否确实新增
    pub fn add_translation(&mut self, source: &str, translation: &str) -> bool {
        let entry = self.entries.entry(source.to_string()).or_insert(TermEntry {
            source: source.to_string(),
            translations: vec![],
            priority: vec![],
            usage_count: 0,
        });
        let existed = entry.translations.iter().any(|t| t == translation);
        if existed {
            return false;
        }
        entry.translations.push(translation.to_string());
        entry.priority.push(0);
        self.dirty = true;
        true
    }

    /// 删除术语的某个译法；若删完译法为空则整条移除。返回是否有变更
    pub fn remove_translation(&mut self, source: &str, translation: &str) -> bool {
        let changed = if let Some(entry) = self.entries.get_mut(source) {
            if let Some(idx) = entry.translations.iter().position(|t| t == translation) {
                entry.translations.remove(idx);
                if idx < entry.priority.len() {
                    entry.priority.remove(idx);
                }
                true
            } else {
                false
            }
        } else {
            false
        };
        if changed {
            if self.entries.get(source).map_or(true, |e| e.translations.is_empty()) {
                self.entries.remove(source);
            }
            self.dirty = true;
        }
        changed
    }

    /// 删除整条术语。返回是否有变更
    pub fn remove_entry(&mut self, source: &str) -> bool {
        if self.entries.remove(source).is_some() {
            self.dirty = true;
            true
        } else {
            false
        }
    }

    /// 查询术语并增加使用次数（自动调整：常用译法优先级+1）
    pub fn lookup(&mut self, source: &str) -> Option<String> {
        let entry = self.entries.get_mut(source)?;
        entry.usage_count += 1;

        // 自动调整：使用次数每累计50次，最佳译法优先级+1
        if entry.usage_count % 50 == 0 {
            if let Some(best) = entry.best_translation().map(|s| s.to_string()) {
                if let Some(idx) = entry.translations.iter().position(|t| *t == best) {
                    if idx < entry.priority.len() {
                        entry.priority[idx] += 1;
                        self.dirty = true;
                    }
                }
            }
        }

        entry.best_translation().map(|s| s.to_string())
    }

    /// 应用术语替换：将文本中的术语替换为最优译法
    pub fn apply_to_text(&mut self, text: &str) -> String {
        let mut result = text.to_string();
        // 收集所有术语（按长度降序，优先匹配长词）；克隆避免借用冲突
        let mut terms: Vec<String> = self.entries.keys().cloned().collect();
        terms.sort_by_key(|t| std::cmp::Reverse(t.chars().count()));

        for term in terms {
            if let Some(translation) = self.lookup(&term) {
                result = result.replace(&term, &translation);
            }
        }
        result
    }
}

// ============ 引擎管理器 ============

/// 翻译引擎管理器：维护所有引擎，根据网络与配置自动选择
pub struct TranslationManager {
    engines: Vec<Box<dyn TranslationEngine>>,
    term_base: TermBase,
}

impl TranslationManager {
    pub fn new(
        baidu: Option<BaiduEngine>,
        youdao: Option<YoudaoEngine>,
        term_base: TermBase,
    ) -> Self {
        let mut engines: Vec<Box<dyn TranslationEngine>> = vec![];
        if let Some(e) = baidu {
            if e.is_configured() {
                engines.push(Box::new(e));
            }
        }
        if let Some(e) = youdao {
            if e.is_configured() {
                engines.push(Box::new(e));
            }
        }
        Self {
            engines,
            term_base,
        }
    }

    /// 列出所有可用引擎信息
    pub fn list_engines(&self) -> Vec<EngineInfo> {
        self.engines
            .iter()
            .map(|e| EngineInfo {
                name: e.name().to_string(),
                engine_type: e.engine_type(),
                available: true,
                configured: e.is_configured(),
            })
            .collect()
    }

    /// 设置术语优先级（手动微调）
    pub fn set_term_priority(&mut self, source: &str, translation: &str, priority: u32) {
        self.term_base.set_priority(source, translation, priority);
    }

    /// 获取所有术语条目
    pub fn list_terms(&self) -> Vec<TermEntry> {
        self.term_base.entries.values().cloned().collect()
    }

    /// 术语库只读访问（持久化用）
    pub fn term_base(&self) -> &TermBase {
        &self.term_base
    }

    /// 术语库可变访问（命令层增删改用）
    pub fn term_base_mut(&mut self) -> &mut TermBase {
        &mut self.term_base
    }

    /// 取出并清除自动调整脏标记（用于翻译后按需落盘）
    pub fn take_term_dirty(&mut self) -> bool {
        let dirty = self.term_base.dirty;
        self.term_base.dirty = false;
        dirty
    }

    /// 翻译：先应用术语库，再选择第一个可用引擎
    pub async fn translate(&mut self, text: &str, from: &str, to: &str) -> Result<TranslationResult, String> {
        if self.engines.is_empty() {
            return Err("没有可用的翻译引擎（请检查网络或配置API密钥）".to_string());
        }

        // 应用术语库替换
        let processed = self.term_base.apply_to_text(text);

        // 依次尝试引擎（第一个成功即返回）
        let mut last_err = String::new();
        for engine in &self.engines {
            match engine.translate(&processed, from, to).await {
                Ok(result) => {
                    return Ok(TranslationResult {
                        translated_text: result,
                        engine_used: engine.name().to_string(),
                        from: from.to_string(),
                        to: to.to_string(),
                    });
                }
                Err(e) => last_err = e,
            }
        }
        Err(last_err)
    }

    /// 批量翻译：每个元素可含多行（\n分隔）。
    /// 百度：展平所有行合并为一次请求（规避免费版1QPS限速），按行拆分后重组；
    /// 其他引擎：逐行翻译后按行数重组。
    /// 返回（与输入对齐的结果数组, 使用的引擎名）
    pub async fn translate_batch(
        &mut self,
        texts: &[String],
        from: &str,
        to: &str,
    ) -> Result<(Vec<String>, String), String> {
        if self.engines.is_empty() {
            return Err("没有可用的翻译引擎（请检查网络或配置API密钥）".to_string());
        }

        // 应用术语库替换
        let processed: Vec<String> = texts
            .iter()
            .map(|t| self.term_base.apply_to_text(t))
            .collect();

        let mut last_err = String::new();
        for engine in &self.engines {
            match Self::batch_with_engine(engine.as_ref(), &processed, from, to).await {
                Ok(results) => return Ok((results, engine.name().to_string())),
                Err(e) => last_err = e,
            }
        }
        Err(last_err)
    }

    /// 批量翻译（指定引擎名称，用于手动切换翻译源）
    pub async fn translate_batch_with(
        &mut self,
        engine_name: &str,
        texts: &[String],
        from: &str,
        to: &str,
    ) -> Result<(Vec<String>, String), String> {
        let processed: Vec<String> = texts
            .iter()
            .map(|t| self.term_base.apply_to_text(t))
            .collect();

        for engine in &self.engines {
            if engine.name() == engine_name {
                let results = Self::batch_with_engine(engine.as_ref(), &processed, from, to).await?;
                return Ok((results, engine.name().to_string()));
            }
        }
        Err(format!("未找到引擎: {engine_name}"))
    }

    /// 单引擎批量执行：百度合并请求，其他逐行；结果按各元素的行数重组对齐
    async fn batch_with_engine(
        engine: &dyn TranslationEngine,
        texts: &[String],
        from: &str,
        to: &str,
    ) -> Result<Vec<String>, String> {
        // 展平所有行
        let mut line_counts: Vec<usize> = Vec::with_capacity(texts.len());
        let mut all_lines: Vec<String> = Vec::new();
        for t in texts {
            let segs: Vec<&str> = t.split('\n').collect();
            line_counts.push(segs.len());
            for s in segs {
                all_lines.push(s.to_string());
            }
        }

        if matches!(engine.engine_type(), EngineType::Baidu) {
            // 百度：q支持\n多行，一次请求返回按行结果
            let joined = all_lines.join("\n");
            if let Ok(result) = engine.translate(&joined, from, to).await {
                let outs: Vec<String> =
                    result.split('\n').map(|s| s.trim().to_string()).collect();
                if outs.len() == all_lines.len() {
                    return Ok(reassemble(&outs, &line_counts));
                }
                // 行数不匹配：落到底部逐行翻译
            }
            // 请求失败也落到底部逐行重试
        }

        // 逐行翻译
        let mut outs: Vec<String> = Vec::with_capacity(all_lines.len());
        for l in &all_lines {
            outs.push(engine.translate(l, from, to).await?);
        }
        Ok(reassemble(&outs, &line_counts))
    }
}

/// 按 line_counts 将展平的行结果重组为与 texts 对齐的数组（\n连接）
fn reassemble(flat: &[String], line_counts: &[usize]) -> Vec<String> {
    let mut results = Vec::with_capacity(line_counts.len());
    let mut idx = 0;
    for c in line_counts {
        let end = (idx + *c).min(flat.len());
        let seg: Vec<String> = flat[idx..end].to_vec();
        idx = end;
        results.push(seg.join("\n"));
    }
    results
}

// ============ 文本预处理 ============

/// 处理首字母大写无间隔（MiniMap -> Mini Map）与下划线（Mini_Map -> Mini Map）
pub fn preprocess_text(text: &str) -> String {
    let mut result = String::with_capacity(text.len() + 8);

    let chars: Vec<char> = text.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        if c == '_' {
            result.push(' ');
            continue;
        }

        // 小写字母后跟大写字母时插入空格（处理驼峰）
        if i > 0
            && c.is_uppercase()
            && chars[i - 1].is_lowercase()
            && chars[i - 1].is_alphabetic()
        {
            result.push(' ');
        }

        result.push(c);
    }

    // 清理多余空格
    let mut cleaned = String::with_capacity(result.len());
    let mut prev_space = false;
    for c in result.chars() {
        if c == ' ' {
            if !prev_space {
                cleaned.push(c);
            }
            prev_space = true;
        } else {
            cleaned.push(c);
            prev_space = false;
        }
    }

    cleaned.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_camel_case() {
        assert_eq!(preprocess_text("MiniMap"), "Mini Map");
        assert_eq!(preprocess_text("HelloWorld"), "Hello World");
    }

    #[test]
    fn test_underscore() {
        assert_eq!(preprocess_text("Mini_Map"), "Mini Map");
    }

    #[test]
    fn test_normal_text() {
        assert_eq!(preprocess_text("hello world"), "hello world");
    }

    #[test]
    fn test_term_base_priority() {
        let mut tb = TermBase::default();
        tb.set_priority("server", "服务器", 5);
        tb.set_priority("server", "服务员", 10);
        assert_eq!(tb.lookup("server"), Some("服务员".to_string()));
    }

    #[test]
    fn test_term_auto_adjust() {
        let mut tb = TermBase::default();
        tb.set_priority("bug", "漏洞", 5);
        tb.set_priority("bug", "缺陷", 3);
        // 使用49次，最优仍是"漏洞"
        for _ in 0..49 {
            tb.lookup("bug");
        }
        assert_eq!(tb.lookup("bug"), Some("漏洞".to_string()));
        // 第50次时自动+1，最优变为"漏洞"（优先级6）
        let entry = &tb.entries["bug"];
        assert_eq!(entry.best_translation(), Some("漏洞"));
    }

    /// 真实API验证引擎切换路径（需网络+密钥）：
    /// 密钥从环境变量读取：BAIDU_APP_ID / BAIDU_SECRET / YOUDAO_APP_KEY / YOUDAO_APP_SECRET
    /// 显式运行：cargo test -- --ignored
    #[ignore]
    #[tokio::test]
    async fn test_batch_switch_engine_real_api() {
        let baidu_id = std::env::var("BAIDU_APP_ID").unwrap_or_default();
        let baidu_secret = std::env::var("BAIDU_SECRET").unwrap_or_default();
        let youdao_key = std::env::var("YOUDAO_APP_KEY").unwrap_or_default();
        let youdao_secret = std::env::var("YOUDAO_APP_SECRET").unwrap_or_default();
        if baidu_id.is_empty()
            || baidu_secret.is_empty()
            || youdao_key.is_empty()
            || youdao_secret.is_empty()
        {
            println!("跳过真实API测试：请先设置环境变量 BAIDU_APP_ID / BAIDU_SECRET / YOUDAO_APP_KEY / YOUDAO_APP_SECRET");
            return;
        }
        let baidu = BaiduEngine::new(&baidu_id, &baidu_secret);
        let youdao = YoudaoEngine::new(&youdao_key, &youdao_secret);
        let mut m = TranslationManager::new(Some(baidu), Some(youdao), TermBase::default());

        let texts = vec![
            "hello world".to_string(),
            "The quick brown fox jumps over the lazy dog".to_string(),
        ];

        // 默认批量（百度：合并\n一次请求）
        let (r1, e1) = m.translate_batch(&texts, "auto", "zh").await.unwrap();
        assert_eq!(r1.len(), 2, "默认批量结果数应与输入一致");
        println!("[默认] {e1} => {r1:?}");

        // 切换到有道（逐行）
        let (r2, e2) = m
            .translate_batch_with("有道智云", &texts, "auto", "zh")
            .await
            .unwrap();
        assert_eq!(r2.len(), 2);
        assert_eq!(e2, "有道智云");
        println!("[有道] {e2} => {r2:?}");

        // 切换回百度（合并请求）
        let (r3, e3) = m
            .translate_batch_with("百度翻译", &texts, "auto", "zh")
            .await
            .unwrap();
        assert_eq!(r3.len(), 2);
        assert_eq!(e3, "百度翻译");
        println!("[百度] {e3} => {r3:?}");

        // 两组结果都非空（内容是否相同取决于引擎，不强制）
        assert!(r1.iter().all(|s| !s.is_empty()));
        assert!(r2.iter().all(|s| !s.is_empty()));
    }

    /// 临时诊断：逐行定位有道失败原因（需 YOUDAO_APP_KEY/YOUDAO_APP_SECRET 环境变量）
    #[ignore]
    #[tokio::test]
    async fn test_youdao_debug() {
        let key = std::env::var("YOUDAO_APP_KEY").unwrap_or_default();
        let secret = std::env::var("YOUDAO_APP_SECRET").unwrap_or_default();
        let e = YoudaoEngine::new(&key, &secret);
        for q in [
            "hello world",
            "The quick brown fox jumps over the lazy dog",
        ] {
            match e.translate(q, "auto", "zh").await {
                Ok(r) => println!("[OK] {q} => {r}"),
                Err(err) => println!("[ERR] {q} => {err}"),
            }
        }
    }
}