//! 离线翻译引擎：OPUS-MT 本地模型（ONNX Runtime 推理，完全离线免费）
//! - 模型：Helsinki-NLP OPUS-MT（Xenova ONNX 镜像，int8 量化，每个语言对约30-80MB）
//! - 分发：按需自动下载（hf-mirror.com 优先，huggingface.co 兜底），存 exe 同目录 models/mt/
//! - 路由：中↔英专门模型直达；其余语言对经英语中转（两次解码）
//! - 解码：贪心、无 KV cache（句子短，O(n²) 可接受）；decoder_start=<pad>，终止=</s>

use crate::engines::TranslationEngine;
use ort::session::builder::GraphOptimizationLevel;
use ort::session::Session;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use tauri::Emitter;
use std::str::FromStr;
use tokenizers::Tokenizer;

/// 全局 AppHandle（main.rs setup 注入），用于向前端推送下载/加载进度；
/// 测试环境未注入时静默跳过
pub static APP_HANDLE: OnceLock<tauri::AppHandle> = OnceLock::new();

/// 模型下载源：镜像优先（中国大陆可达），官方站兜底
const HF_BASES: [&str; 2] = ["https://hf-mirror.com", "https://huggingface.co"];
const MAX_SRC_TOKENS: usize = 500; // Marian 位置编码上限 512，留余量
const MAX_NEW_TOKENS: usize = 256;
/// 重复抑制：n=2 禁止一切相邻二元组重复（OPUS-MT 小模型复读倾向强，
/// 紧邻重复几乎无合法场景；代价是牺牲"谢谢/慢慢"类叠词，可接受）+ token概率衰减
const NO_REPEAT_NGRAM: usize = 2;
const REPETITION_PENALTY: f32 = 1.2;
/// 紧邻重复（A A）重罚：小模型"park park/貓貓貓"复读的主因；软禁保留"谢谢"类叠词
const ADJACENT_REPEAT_PENALTY: f32 = 4.0;
/// beam search：束宽（HF 对 Marian 类模型的推荐值；批量化实现，速度损失小）
const NUM_BEAMS: usize = 4;
/// 下载进度事件节流：每 8MB 最多推送一次
const DOWNLOAD_EMIT_STEP: u64 = 8 * 1024 * 1024;

fn emit_status(msg: &str) {
    if let Some(h) = APP_HANDLE.get() {
        let _ = h.emit("offline-mt-status", msg.to_string());
    }
}

// ============ 语言与模型目录 ============

fn lang_name(code: &str) -> &str {
    match code {
        "zh" => "中文",
        "en" => "英语",
        "ja" => "日语",
        "ko" => "韩语",
        "ru" => "俄语",
        "fr" => "法语",
        "de" => "德语",
        "es" => "西语",
        "pt" => "葡语",
        _ => code,
    }
}

/// 脚本启发式语言检测（auto 时的离线替代方案）：
/// 谚文→韩、假名→日、汉字→中、西里尔→俄、其余→英
pub fn detect_lang(text: &str) -> &'static str {
    for c in text.chars() {
        let u = c as u32;
        if (0xAC00..=0xD7AF).contains(&u) || (0x1100..=0x11FF).contains(&u) {
            return "ko";
        }
        if (0x3040..=0x309F).contains(&u) || (0x30A0..=0x30FF).contains(&u) {
            return "ja";
        }
        if (0x4E00..=0x9FFF).contains(&u) || (0x3400..=0x4DBF).contains(&u) {
            return "zh";
        }
        if (0x0400..=0x04FF).contains(&u) {
            return "ru";
        }
    }
    "en"
}

/// 直达模型目录：语言对 → HuggingFace 仓库（Xenova ONNX 镜像，int8 量化）
fn pair_repo(from: &str, to: &str) -> Option<&'static str> {
    match (from, to) {
        ("zh", "en") => Some("Xenova/opus-mt-zh-en"),
        ("en", "zh") => Some("Xenova/opus-mt-en-zh"),
        ("ja", "en") => Some("Xenova/opus-mt-ja-en"),
        ("en", "ja") => Some("Xenova/opus-mt-en-ja"),
        ("ko", "en") => Some("Xenova/opus-mt-ko-en"),
        ("en", "ko") => Some("Xenova/opus-mt-en-ko"),
        ("ru", "en") => Some("Xenova/opus-mt-ru-en"),
        ("en", "ru") => Some("Xenova/opus-mt-en-ru"),
        ("fr", "en") => Some("Xenova/opus-mt-fr-en"),
        ("en", "fr") => Some("Xenova/opus-mt-en-fr"),
        ("de", "en") => Some("Xenova/opus-mt-de-en"),
        ("en", "de") => Some("Xenova/opus-mt-en-de"),
        ("es", "en") => Some("Xenova/opus-mt-es-en"),
        ("en", "es") => Some("Xenova/opus-mt-en-es"),
        ("pt", "en") => Some("Xenova/opus-mt-pt-en"),
        ("en", "pt") => Some("Xenova/opus-mt-en-pt"),
        _ => None,
    }
}

/// 翻译路由：返回需要依次执行的 (源,目标) 语言跳。
/// 有直达模型用直达；无直达且两端均含英语可达时经英语中转。
pub fn route(from: &str, to: &str) -> Vec<(&'static str, &'static str)> {
    if from == to {
        return vec![];
    }
    if let Some(_) = pair_repo(from, to) {
        return vec![(boxed_pair(from), boxed_pair(to))];
    }
    if from != "en" && to != "en" {
        if pair_repo(from, "en").is_some() && pair_repo("en", to).is_some() {
            return vec![(boxed_pair(from), "en"), ("en", boxed_pair(to))];
        }
    }
    vec![]
}

fn boxed_pair(s: &str) -> &'static str {
    match s {
        "zh" => "zh",
        "en" => "en",
        "ja" => "ja",
        "ko" => "ko",
        "ru" => "ru",
        "fr" => "fr",
        "de" => "de",
        "es" => "es",
        "pt" => "pt",
        _ => "en",
    }
}

// ============ 模型下载 ============

/// 模型根目录：exe 同目录 models/mt/（可用环境变量 SMART_TRANSLATOR_MODELS_DIR 覆盖，测试用）
fn models_root() -> PathBuf {
    if let Ok(dir) = std::env::var("SMART_TRANSLATOR_MODELS_DIR") {
        return PathBuf::from(dir);
    }
    let exe = std::env::current_exe().unwrap_or_default();
    exe.parent()
        .map(|p| p.join("models").join("mt"))
        .unwrap_or_else(|| PathBuf::from("models/mt"))
}

async fn download_file(repo: &str, file: &str, dest: &PathBuf) -> Result<(), String> {
    if dest.exists() && std::fs::metadata(dest).map(|m| m.len() > 0).unwrap_or(false) {
        return Ok(());
    }
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建模型目录失败: {e}"))?;
    }
    let tmp = dest.with_extension("downloading");

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(600))
        .build()
        .map_err(|e| format!("HTTP客户端创建失败: {e}"))?;

    let mut last_err = String::new();
    for base in HF_BASES {
        let url = format!("{base}/{repo}/resolve/main/{file}");
        let resp = match client.get(&url).send().await {
            Ok(r) if r.status().is_success() => r,
            Ok(r) => {
                last_err = format!("{url} -> HTTP {}", r.status());
                continue;
            }
            Err(e) => {
                last_err = format!("{url} -> {e}");
                continue;
            }
        };

        let total = resp.content_length().unwrap_or(0);
        let mut stream = resp;
        let mut out = match std::fs::File::create(&tmp) {
            Ok(f) => f,
            Err(e) => return Err(format!("创建临时文件失败: {e}")),
        };
        let mut received: u64 = 0;
        let mut next_emit = DOWNLOAD_EMIT_STEP;
        loop {
            match stream.chunk().await {
                Ok(Some(chunk)) => {
                    use std::io::Write;
                    if let Err(e) = out.write_all(&chunk) {
                        return Err(format!("写入模型文件失败: {e}"));
                    }
                    received += chunk.len() as u64;
                    if total > 0 && received >= next_emit {
                        next_emit = received + DOWNLOAD_EMIT_STEP;
                        let pct = (received as f64 / total as f64 * 100.0) as u32;
                        emit_status(&format!(
                            "正在下载模型 {}%（{}MB/{}MB）",
                            pct,
                            received / 1024 / 1024,
                            total / 1024 / 1024
                        ));
                    }
                }
                Ok(None) => break,
                Err(e) => {
                    last_err = format!("{url} -> 传输中断: {e}");
                    break;
                }
            }
        }
        // 完整接收才算成功
        if total == 0 || received >= total {
            if let Err(e) = std::fs::rename(&tmp, dest) {
                return Err(format!("模型文件落盘失败: {e}"));
            }
            return Ok(());
        }
    }
    Err(format!("模型下载失败（多源均不可达）: {last_err}"))
}

/// 确保某语言对模型就绪（已存在则跳过；不存在则逐文件下载）
async fn ensure_pair_models(from: &str, to: &str) -> Result<PathBuf, String> {
    let repo = pair_repo(from, to).ok_or_else(|| {
        format!(
            "离线翻译暂不支持 {}→{}（无对应本地模型）",
            lang_name(from),
            lang_name(to)
        )
    })?;
    let dir = models_root().join(repo.replace('/', "_"));
    let files = [
        ("tokenizer.json", dir.join("tokenizer.json")),
        (
            "onnx/encoder_model_quantized.onnx",
            dir.join("encoder_model_quantized.onnx"),
        ),
        (
            "onnx/decoder_model_merged_quantized.onnx",
            dir.join("decoder_model_merged_quantized.onnx"),
        ),
    ];
    for (src, dest) in files {
        let exists = dest.exists() && std::fs::metadata(&dest).map(|m| m.len() > 0).unwrap_or(false);
        if !exists {
            emit_status(&format!(
                "首次使用离线翻译（{}→{}）：准备模型文件…",
                lang_name(from),
                lang_name(to)
            ));
            download_file(repo, src, &dest).await?;
        }
    }
    Ok(dir)
}

// ============ 模型加载与推理 ============

/// 单语言对：分词器 + 编码/解码会话（懒加载，进程级缓存）
struct MtPair {
    tokenizer: Tokenizer,
    encoder: Session,
    decoder: Session,
    enc_in_ids: String,
    enc_in_mask: String,
    enc_out: String,
    dec_in_ids: String,
    dec_in_mask: String,
    dec_in_hidden: String,
    dec_out: String,
    /// KV cache（decoder_model_merged）：use_cache_branch 开关输入
    cache_flag_in: String,
    /// (past输入名, present输出名)，按层排序一一对应
    past_io: Vec<(String, String)>,
    pad_id: u32, // Marian decoder 起始 token
    eos_id: u32,
}

static PAIRS: Mutex<Option<HashMap<String, Arc<Mutex<MtPair>>>>> = Mutex::new(None);

fn find_input(names: &[String], contains: &str) -> Result<String, String> {
    names
        .iter()
        .find(|n| n.contains(contains))
        .cloned()
        .ok_or_else(|| format!("模型缺少输入 {contains}（实际输入: {names:?}）"))
}

/// 加载语言对模型（分词器 + 两个 ONNX 会话），IO名按会话元数据 introspect
/// 加载分词器。Xenova 镜像的 tokenizer.json 中 Precompiled normalizer 的
/// charsmap 可能为 null（transformers.js 不需要），Rust tokenizers 无法反序列化——
/// 预处理剥掉该 normalizer（其作用仅是 NFC 规范化，对翻译结果无实质影响）
fn load_tokenizer(path: &PathBuf) -> Result<Tokenizer, String> {
    let raw = std::fs::read_to_string(path).map_err(|e| format!("读取tokenizer失败: {e}"))?;
    let mut v: serde_json::Value =
        serde_json::from_str(&raw).map_err(|e| format!("tokenizer.json解析失败: {e}"))?;
    if v["normalizer"]["type"] == "Precompiled"
        && v["normalizer"]["precompiled_charsmap"].is_null()
    {
        v["normalizer"] = serde_json::Value::Null;
    }
    Tokenizer::from_str(&v.to_string()).map_err(|e| format!("分词器加载失败: {e}"))
}

fn load_pair(dir: &PathBuf) -> Result<Arc<Mutex<MtPair>>, String> {
    let tokenizer = load_tokenizer(&dir.join("tokenizer.json"))?;
    let pad_id = tokenizer
        .token_to_id("<pad>")
        .ok_or("分词器缺少 <pad>（无法确定解码起始符）")?;
    let eos_id = tokenizer
        .token_to_id("</s>")
        .ok_or("分词器缺少 </s>（无法确定终止符）")?;

    let build = |path: &PathBuf| -> Result<Session, String> {
        Session::builder()
            .map_err(|e| format!("ORT会话构建失败: {e}"))?
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .map_err(|e| format!("ORT优化配置失败: {e}"))?
            .with_intra_threads(4)
            .map_err(|e| format!("ORT线程配置失败: {e}"))?
            .commit_from_file(path)
            .map_err(|e| format!("ORT模型加载失败（{}）: {e}", path.display()))
    };
    let encoder = build(&dir.join("encoder_model_quantized.onnx"))?;
    let decoder = build(&dir.join("decoder_model_merged_quantized.onnx"))?;

    let enc_inputs: Vec<String> = encoder.inputs().iter().map(|i| i.name().to_string()).collect();
    let enc_outputs: Vec<String> = encoder.outputs().iter().map(|o| o.name().to_string()).collect();
    let dec_inputs: Vec<String> = decoder.inputs().iter().map(|i| i.name().to_string()).collect();
    let dec_outputs: Vec<String> = decoder.outputs().iter().map(|o| o.name().to_string()).collect();

    // KV cache 必需输入：use_cache_branch 开关 + past_key_values.N.*（与 present.N.* 成对）
    let cache_flag_in = find_input(&dec_inputs, "use_cache_branch")?;
    let mut past_in: Vec<String> = dec_inputs
        .iter()
        .filter(|n| n.starts_with("past_key_values"))
        .cloned()
        .collect();
    past_in.sort();
    let mut past_io: Vec<(String, String)> = Vec::with_capacity(past_in.len());
    for n in past_in {
        let suffix = &n["past_key_values.".len()..];
        let out = format!("present.{suffix}");
        if !dec_outputs.contains(&out) {
            return Err(format!("解码器缺少 past 对应输出 {out}（模型格式异常）"));
        }
        past_io.push((n, out));
    }
    if past_io.is_empty() {
        return Err("解码器没有 past_key_values 输入（非 merged 格式，无法启用 KV cache）".to_string());
    }
    // logits 输出 = 非 present 输出的第一个
    let dec_out_name = dec_outputs
        .iter()
        .find(|o| !o.starts_with("present"))
        .cloned()
        .ok_or("解码器没有 logits 输出（模型文件异常）")?;

    Ok(Arc::new(Mutex::new(MtPair {
        tokenizer,
        encoder,
        decoder,
        enc_in_ids: find_input(&enc_inputs, "input_ids")?,
        enc_in_mask: find_input(&enc_inputs, "mask")?,
        enc_out: enc_outputs
            .first()
            .cloned()
            .ok_or("编码器没有输出（模型文件异常）")?,
        // Xenova 导出的解码器自回归输入名为 input_ids（非 decoder_input_ids）
        dec_in_ids: find_input(&dec_inputs, "input_ids")?,
        dec_in_mask: find_input(&dec_inputs, "mask")?,
        dec_in_hidden: find_input(&dec_inputs, "hidden_states")?,
        dec_out: dec_out_name,
        cache_flag_in,
        past_io,
        pad_id,
        eos_id,
    })))
}

/// 取（必要时加载）语言对模型。锁内完成加载以避免并发重复加载。
fn get_pair(from: &str, to: &str, dir: &PathBuf) -> Result<Arc<Mutex<MtPair>>, String> {
    let repo = pair_repo(from, to).ok_or("无对应本地模型")?;
    let mut guard = PAIRS
        .lock()
        .map_err(|_| "离线模型缓存锁不可用".to_string())?;
    let map = guard.get_or_insert_with(HashMap::new);
    if let Some(p) = map.get(repo) {
        return Ok(p.clone());
    }
    let pair = load_pair(dir)?;
    map.insert(repo.to_string(), pair.clone());
    Ok(pair)
}

/// 编码器前向：返回 last_hidden_state 扁平 f32（[1, L, D]）
fn run_encoder(
    pair: &mut MtPair,
    ids: &[i64],
    attn: &[i64],
) -> Result<(Vec<f32>, usize, usize), String> {
    let l = ids.len();
    let enc_ids = ort::value::Tensor::from_array((vec![1usize, l], ids.to_vec()))
        .map_err(|e| e.to_string())?;
    let enc_attn = ort::value::Tensor::from_array((vec![1usize, l], attn.to_vec()))
        .map_err(|e| e.to_string())?;
    let outputs = pair
        .encoder
        .run(ort::inputs![
            pair.enc_in_ids.as_str() => enc_ids,
            pair.enc_in_mask.as_str() => enc_attn,
        ])
        .map_err(|e| format!("编码器推理失败: {e}"))?;
    let (shape, data) = outputs[pair.enc_out.as_str()]
        .try_extract_tensor::<f32>()
        .map_err(|e| format!("编码器输出提取失败: {e}"))?;
    let total: usize = shape.iter().map(|&d| d as usize).product();
    let d = *shape.last().unwrap_or(&512) as usize;
    Ok((data[..total].to_vec(), l, d))
}

/// 自注意力 KV cache（跨步携带，免全量重解码）：
/// shapes/data 按 past_io 顺序一一对应，data 为扁平行主序
struct PastTensors {
    shapes: Vec<Vec<usize>>,
    data: Vec<Vec<f32>>,
}

impl PastTensors {
    /// 按 beam 重组：新第 r 束继承父束 parents[r] 的缓存（束被选择/淘汰时调用）
    fn reordered(&self, parents: &[usize]) -> PastTensors {
        let mut shapes = Vec::with_capacity(self.shapes.len());
        let mut data = Vec::with_capacity(self.data.len());
        for (shape, tensor) in self.shapes.iter().zip(&self.data) {
            let b = shape[0].max(1);
            let row: usize = shape.iter().product::<usize>() / b;
            let mut nd = Vec::with_capacity(parents.len() * row);
            for &p in parents {
                nd.extend_from_slice(&tensor[p * row..(p + 1) * row]);
            }
            let mut nshape = shape.clone();
            nshape[0] = parents.len();
            shapes.push(nshape);
            data.push(nd);
        }
        PastTensors { shapes, data }
    }
}

/// 从会话元数据读 past 输入的静态形状构造零长度缓存（首步喂给 merged 模型的空 past）。
/// 动态维（-1）：第0维=批大小，其余（自注意力序列长度）=0；读不到元数据时退回 Marian 标准 8头×64维
fn zero_past(pair: &MtPair, b: usize) -> PastTensors {
    let mut shapes = Vec::with_capacity(pair.past_io.len());
    let mut data = Vec::with_capacity(pair.past_io.len());
    for (in_name, _) in &pair.past_io {
        let meta: Vec<i64> = pair
            .decoder
            .inputs()
            .iter()
            .find(|o| o.name() == in_name)
            .and_then(|o| match o.dtype() {
                ort::value::ValueType::Tensor { shape, .. } => {
                    Some(shape.iter().map(|&d| d).collect::<Vec<i64>>())
                }
                _ => None,
            })
            .unwrap_or_else(|| vec![-1, 8, -1, 64]);
        // optimum/Xenova merged 导出的 self-attn past 固定布局 [batch, heads, seq, head_dim]
        // （实测元数据 [-1, 8, 1, 64]，seq 动态维声明为字面1）：首维=当前批大小，seq 置0，其余保留
        let shape: Vec<usize> = meta
            .iter()
            .enumerate()
            .map(|(i, &d)| {
                if i == 0 {
                    b
                } else if i == 2 {
                    0
                } else {
                    d.max(1) as usize
                }
            })
            .collect();
        let total: usize = shape.iter().product();
        shapes.push(shape);
        data.push(vec![0.0; total]);
    }
    PastTensors { shapes, data }
}

/// 解码器前向（缓存模式）：只喂各束最新 token + 上一步 past，返回各束 logits [V] 与新 past
fn run_decoder_cached(
    pair: &mut MtPair,
    dec_last: &[i64],
    dec_past: PastTensors,
    enc_past: &PastTensors,
    enc_hidden: &[f32],
    enc_len: usize,
    enc_dim: usize,
    attn: &[i64],
) -> Result<(Vec<Vec<f32>>, PastTensors, PastTensors), String> {
    let b = dec_last.len();
    let dec_in = ort::value::Tensor::from_array((vec![b, 1usize], dec_last.to_vec()))
        .map_err(|e| e.to_string())?;
    let mut hidden = Vec::with_capacity(b * enc_len * enc_dim);
    let mut attn_b = Vec::with_capacity(b * enc_len);
    for _ in 0..b {
        hidden.extend_from_slice(enc_hidden);
        attn_b.extend_from_slice(attn);
    }
    let dec_hidden = ort::value::Tensor::from_array((vec![b, enc_len, enc_dim], hidden))
        .map_err(|e| e.to_string())?;
    let dec_attn = ort::value::Tensor::from_array((vec![b, enc_len], attn_b))
        .map_err(|e| e.to_string())?;
    let flag = ort::value::Tensor::from_array((vec![1usize], vec![true])).map_err(|e| e.to_string())?;

    let mut inputs: Vec<(&str, ort::session::SessionInputValue)> = Vec::new();
    inputs.push((pair.dec_in_ids.as_str(), dec_in.into()));
    inputs.push((pair.dec_in_mask.as_str(), dec_attn.into()));
    inputs.push((pair.dec_in_hidden.as_str(), dec_hidden.into()));
    inputs.push((pair.cache_flag_in.as_str(), flag.into()));
    // past_io 顺序含 decoder/encoder 混排；按名字路由到对应组取数
    let mut dec_i = 0usize;
    let mut enc_i = 0usize;
    for (in_name, _) in pair.past_io.iter() {
        let is_enc = in_name.contains(".encoder.");
        let src_past = if is_enc { &enc_past } else { &dec_past };
        let i = if is_enc { enc_i } else { dec_i };
        let t = ort::value::Tensor::from_array((src_past.shapes[i].clone(), src_past.data[i].clone()))
            .map_err(|e| e.to_string())?;
        inputs.push((in_name.as_str(), t.into()));
        if is_enc { enc_i += 1; } else { dec_i += 1; }
    }
    // outputs 借用 session：先把只读元数据取出，避免与 &mut pair 冲突
    let dec_out = pair.dec_out.clone();
    let past_io = pair.past_io.clone();
    let outputs = pair
        .decoder
        .run(inputs)
        .map_err(|e| format!("解码器推理失败: {e}"))?;
    read_logits_and_past(&dec_out, &past_io, &outputs, b)
}

/// 解码器前向（首步）：use_cache_branch=false 走全量分支（仅1个起始token），拿到首份past
fn run_decoder_first(
    pair: &mut MtPair,
    dec_ids: &[Vec<i64>],
    enc_hidden: &[f32],
    enc_len: usize,
    enc_dim: usize,
    attn: &[i64],
) -> Result<(Vec<Vec<f32>>, PastTensors, PastTensors), String> {
    let b = dec_ids.len();
    let flat: Vec<i64> = dec_ids.concat();
    let mut hidden = Vec::with_capacity(b * enc_len * enc_dim);
    let mut attn_b = Vec::with_capacity(b * enc_len);
    for _ in 0..b {
        hidden.extend_from_slice(enc_hidden);
        attn_b.extend_from_slice(attn);
    }
    let dec_in =
        ort::value::Tensor::from_array((vec![b, dec_ids[0].len()], flat)).map_err(|e| e.to_string())?;
    let dec_hidden = ort::value::Tensor::from_array((vec![b, enc_len, enc_dim], hidden))
        .map_err(|e| e.to_string())?;
    let dec_attn = ort::value::Tensor::from_array((vec![b, enc_len], attn_b))
        .map_err(|e| e.to_string())?;
    let flag = ort::value::Tensor::from_array((vec![1usize], vec![false])).map_err(|e| e.to_string())?;

    let mut inputs: Vec<(&str, ort::session::SessionInputValue)> = Vec::new();
    inputs.push((pair.dec_in_ids.as_str(), dec_in.into()));
    inputs.push((pair.dec_in_mask.as_str(), dec_attn.into()));
    inputs.push((pair.dec_in_hidden.as_str(), dec_hidden.into()));
    inputs.push((pair.cache_flag_in.as_str(), flag.into()));
    // outputs 借用 session：先把只读元数据取出，避免与 &mut pair 冲突
    let dec_out = pair.dec_out.clone();
    let past_io = pair.past_io.clone();
    let zero = zero_past(pair, b);
    for (i, (in_name, _)) in past_io.iter().enumerate() {
        let t = ort::value::Tensor::from_array((zero.shapes[i].clone(), zero.data[i].clone()))
            .map_err(|e| e.to_string())?;
        inputs.push((in_name.as_str(), t.into()));
    }
    let outputs = pair
        .decoder
        .run(inputs)
        .map_err(|e| format!("解码器推理失败: {e}"))?;
    read_logits_and_past(&dec_out, &past_io, &outputs, b)
}

/// 从输出提取各束最后一步 logits 与 present KV cache
fn read_logits_and_past(
    dec_out: &str,
    past_io: &[(String, String)],
    outputs: &ort::session::SessionOutputs,
    b: usize,
) -> Result<(Vec<Vec<f32>>, PastTensors, PastTensors), String> {
    let (shape, data) = outputs[dec_out]
        .try_extract_tensor::<f32>()
        .map_err(|e| format!("解码器输出提取失败: {e}"))?;
    let v = *shape.last().ok_or("logits维度异常")? as usize;
    // [B, t, V]：各束最后一步
    let steps = shape.iter().map(|&d| d as usize).product::<usize>() / v / b.max(1);
    let mut logits = Vec::with_capacity(b);
    for bi in 0..b {
        let start = (bi * steps + (steps - 1)) * v;
        logits.push(data[start..start + v].to_vec());
    }
    // 拆两组：decoder.*（自注意力，每步增长）与 encoder.*（cross-attention KV，
    // 仅首步输出有效；true分支的encoder present是空张量，需沿用首步值恒定回喂）
    let mut dec = (Vec::new(), Vec::new());
    let mut enc = (Vec::new(), Vec::new());
    for (_, out_name) in past_io {
        let (pshape, pdata) = outputs[out_name.as_str()]
            .try_extract_tensor::<f32>()
            .map_err(|e| format!("past输出提取失败({out_name}): {e}"))?;
        let shape: Vec<usize> = pshape.iter().map(|&d| d as usize).collect();
        let data = pdata.to_vec();
        let target = if out_name.contains(".encoder.") { &mut enc } else { &mut dec };
        target.0.push(shape);
        target.1.push(data);
    }
    Ok((
        logits,
        PastTensors { shapes: dec.0, data: dec.1 },
        PastTensors { shapes: enc.0, data: enc.1 },
    ))
}

/// 繁→简转换：OPUS-MT en→zh 语料偏繁体，目标语言为中文时统一转简体。
/// 用 Windows 系统自带 LCMapString（全字表、零维护）；非 Windows 平台原样返回
#[cfg(target_os = "windows")]
fn to_simplified(text: &str) -> String {
    use windows::Win32::Globalization::{LCMapStringW, LCMAP_SIMPLIFIED_CHINESE};
    if text.is_empty() {
        return text.to_string();
    }
    let src_wide: Vec<u16> = text.encode_utf16().collect();
    const ZH_CN: u32 = 0x0804; // zh-CN
    unsafe {
        // 第一次调用取目标长度
        let len = LCMapStringW(ZH_CN, LCMAP_SIMPLIFIED_CHINESE, &src_wide, None, 0);
        if len <= 0 {
            return text.to_string();
        }
        let mut dest = vec![0u16; len as usize];
        let written = LCMapStringW(
            ZH_CN,
            LCMAP_SIMPLIFIED_CHINESE,
            &src_wide,
            Some(windows::core::PWSTR(dest.as_mut_ptr())),
            len,
        );
        if written <= 0 {
            return text.to_string();
        }
        dest.truncate(written as usize);
        String::from_utf16_lossy(&dest)
    }
}

#[cfg(not(target_os = "windows"))]
fn to_simplified(text: &str) -> String {
    text.to_string()
}

/// 重复抑制（transformers 标准手段）：
/// 1) no_repeat_ngram_size：历史中出现过同前缀 n-gram 时禁选其尾token（掐断死循环）
/// 2) repetition_penalty：全部已生成token的logit衰减（CTRL论文式，压轻度复读）
fn suppress_repetition(logits: &mut [f32], generated: &[i64]) {
    if let Some(&last) = generated.last() {
        if last >= 0 {
            let u = last as usize;
            if u < logits.len() {
                logits[u] -= ADJACENT_REPEAT_PENALTY;
            }
        }
    }
    let n = NO_REPEAT_NGRAM;
    if generated.len() >= n {
        let m = n - 1;
        let prefix = &generated[generated.len() - m..];
        for i in 0..=(generated.len() - n) {
            if &generated[i..i + m] == prefix {
                let ban = generated[i + m] as usize;
                if ban < logits.len() {
                    logits[ban] = f32::NEG_INFINITY;
                }
            }
        }
    }
    for &t in generated {
        let u = if t >= 0 { t as usize } else { continue };
        if u < logits.len() {
            if logits[u] > 0.0 {
                logits[u] -= REPETITION_PENALTY;
            } else {
                logits[u] *= REPETITION_PENALTY;
            }
        }
    }
}

/// beam search 解码（束宽 NUM_BEAMS，批量化前向）：
/// 得分=对数概率之和/长度（平均对数概率），EOS 即入完成池，活跃束凑不满/达步数上限时结束
fn beam_decode(pair: &mut MtPair, text: &str) -> Result<String, String> {
    if text.trim().is_empty() {
        return Ok(String::new());
    }
    let enc = pair
        .tokenizer
        .encode(text, false)
        .map_err(|e| format!("分词失败: {e}"))?;
    let mut ids: Vec<i64> = enc.get_ids().iter().map(|&i| i as i64).collect();
    ids.truncate(MAX_SRC_TOKENS);
    let attn: Vec<i64> = vec![1; ids.len()];

    let (hidden, enc_len, enc_dim) = run_encoder(pair, &ids, &attn)?;

    struct Hypo {
        tokens: Vec<i64>, // 含起始 <pad>
        logprob: f32,
    }
    let start = pair.pad_id as i64;
    let mut beams = vec![Hypo { tokens: vec![start], logprob: 0.0 }];
    let mut completed: Vec<Hypo> = Vec::new();
    // 自注意力缓存（每步更新）+ cross-attention缓存（首步取值后恒定，仅随束复制）
    let mut dec_past: Option<PastTensors> = None;
    let mut enc_past: Option<PastTensors> = None;

    for _step in 0..MAX_NEW_TOKENS {
        if beams.is_empty() {
            break;
        }
        // 首步走全量分支（仅起始token），并取回有效的 cross-attention KV；
        // 之后只喂各束最新token + 上步缓存（encoder 输出为空，忽略）
        let is_first = dec_past.is_none();
        let (logits_batch, new_dec) = if is_first {
            let dec_ids: Vec<Vec<i64>> = beams.iter().map(|b| b.tokens.clone()).collect();
            let (l, nd, ne) = run_decoder_first(pair, &dec_ids, &hidden, enc_len, enc_dim, &attn)?;
            enc_past = Some(ne);
            (l, nd)
        } else {
            let dp = dec_past
                .take()
                .ok_or_else(|| "解码缓存状态异常".to_string())?;
            let ep = enc_past
                .as_ref()
                .ok_or_else(|| "解码缓存状态异常".to_string())?;
            let last: Vec<i64> = beams.iter().map(|b| *b.tokens.last().unwrap()).collect();
            let (l, nd, _ne_empty) =
                run_decoder_cached(pair, &last, dp, ep, &hidden, enc_len, enc_dim, &attn)?;
            (l, nd)
        };

        // 每束 log_softmax 后取局部 top-K，再合并全局排序
        let k = (NUM_BEAMS * 2).min(logits_batch[0].len());
        struct Cand {
            parent: usize,
            token: u32,
            score: f32,
        }
        let mut cands: Vec<Cand> = Vec::new();
        for (bi, (beam, logits)) in beams.iter().zip(&logits_batch).enumerate() {
            let mut logits = logits.clone();
            suppress_repetition(&mut logits, &beam.tokens);
            let maxl = logits.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
            let sum: f32 = logits.iter().map(|&l| (l - maxl).exp()).sum();
            let log_norm = maxl + sum.ln();
            let mut idx: Vec<u32> = (0..logits.len() as u32).collect();
            idx.sort_unstable_by(|&a, &b| {
                logits[b as usize]
                    .partial_cmp(&logits[a as usize])
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            for t in idx.into_iter().take(k) {
                cands.push(Cand {
                    parent: bi,
                    token: t,
                    score: beam.logprob + (logits[t as usize] - log_norm),
                });
            }
        }
        cands.sort_unstable_by(|a, b| {
            b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal)
        });

        let mut next_beams: Vec<Hypo> = Vec::new();
        let mut parents: Vec<usize> = Vec::new();
        for c in cands {
            if next_beams.len() >= NUM_BEAMS {
                break;
            }
            let mut tokens = beams[c.parent].tokens.clone();
            tokens.push(c.token as i64);
            if c.token == pair.eos_id {
                completed.push(Hypo { tokens, logprob: c.score });
            } else {
                parents.push(c.parent);
                next_beams.push(Hypo { tokens, logprob: c.score });
            }
        }
        // 缓存按"谁被保留"重排；encoder 组仅按父束复制（内容恒定），供下一步使用
        dec_past = Some(new_dec.reordered(&parents));
        enc_past = Some(enc_past.take().ok_or("解码缓存状态异常")?.reordered(&parents));
        beams = next_beams;
        if completed.len() >= NUM_BEAMS {
            break;
        }
    }
    completed.extend(beams); // 未完成的也参与评分
    if completed.is_empty() {
        return Err("解码失败：无候选".to_string());
    }
    // GNMT 长度惩罚（(5+len)/6）：兼顾防短句偏好与防长输出偏好
    let score = |h: &Hypo| {
        let len = (h.tokens.len() - 1).max(1) as f32;
        h.logprob / ((5.0 + len) / 6.0)
    };
    completed.sort_unstable_by(|a, b| {
        score(b).partial_cmp(&score(a)).unwrap_or(std::cmp::Ordering::Equal)
    });
    let best = &completed[0];
    let out_ids: Vec<u32> = best.tokens[1..].iter().map(|&t| t as u32).collect();
    pair.tokenizer
        .decode(&out_ids, true)
        .map_err(|e| format!("反分词失败: {e}"))
}

/// 按语言跳依次翻译（模型必须已就绪；纯CPU推理在阻塞线程执行）
fn translate_blocking(hops: &[(&'static str, &'static str)], text: &str) -> Result<String, String> {
    let mut cur = text.to_string();
    for (f, t) in hops {
        let dir = {
            let repo = pair_repo(f, t).ok_or("无对应本地模型")?;
            models_root().join(repo.replace('/', "_"))
        };
        let pair = get_pair(f, t, &dir)?;
        let mut p = pair.lock().map_err(|_| "模型会话锁不可用".to_string())?;
        cur = beam_decode(&mut p, &cur)?;
    }
    // OPUS-MT en→zh 语料偏繁体：目标为中文时统一转简体（Windows 系统级转换）
    if hops.last().map(|(_, t)| *t) == Some("zh") {
        cur = to_simplified(&cur);
    }
    Ok(cur)
}

// ============ 引擎实现 ============

pub struct OfflineMtEngine;

impl OfflineMtEngine {
    pub fn new() -> Self {
        Self
    }
}

impl Default for OfflineMtEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl TranslationEngine for OfflineMtEngine {
    fn name(&self) -> &str {
        "离线翻译"
    }

    fn engine_type(&self) -> crate::engines::EngineType {
        crate::engines::EngineType::Offline
    }

    /// 无需密钥；启用与否由 settings.offline_engine 在 build_manager 层控制
    fn is_configured(&self) -> bool {
        true
    }

    async fn translate(&self, text: &str, from: &str, to: &str) -> Result<String, String> {
        if text.trim().is_empty() {
            return Ok(String::new());
        }
        let from = if from == "auto" {
            detect_lang(text)
        } else {
            from
        };
        let hops = route(from, to);
        if hops.is_empty() {
            return Err(format!(
                "离线翻译暂不支持 {}→{}（无对应本地模型）",
                lang_name(from),
                lang_name(to)
            ));
        }

        // 下载是异步IO：在阻塞推理前完成
        for (f, t) in &hops {
            ensure_pair_models(f, t).await?;
        }

        let text = text.to_string();
        tauri::async_runtime::spawn_blocking(move || translate_blocking(&hops, &text))
            .await
            .map_err(|e| format!("离线翻译任务执行失败: {e}"))?
    }
}

// ============ 测试 ============

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_lang() {
        assert_eq!(detect_lang("Hello, world!"), "en");
        assert_eq!(detect_lang("你好，世界"), "zh");
        assert_eq!(detect_lang("こんにちは世界"), "ja");
        assert_eq!(detect_lang("안녕하세요"), "ko");
        assert_eq!(detect_lang("Привет мир"), "ru");
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn test_to_simplified() {
        assert_eq!(to_simplified("這是一個測試"), "这是一个测试");
        assert_eq!(to_simplified("我喜歡貓"), "我喜欢猫");
        // 简体/标点原样保留
        assert_eq!(to_simplified("你好，世界！hello"), "你好，世界！hello");
    }

    #[test]
    fn test_route() {
        // 直达
        assert_eq!(route("en", "zh"), vec![("en", "zh")]);
        assert_eq!(route("zh", "en"), vec![("zh", "en")]);
        // 经英语中转
        assert_eq!(route("ja", "zh"), vec![("ja", "en"), ("en", "zh")]);
        assert_eq!(route("zh", "ja"), vec![("zh", "en"), ("en", "ja")]);
        assert_eq!(route("fr", "de"), vec![("fr", "en"), ("en", "de")]);
        // 同语言/不支持
        assert!(route("zh", "zh").is_empty());
        assert!(route("xx", "yy").is_empty());
    }

    /// 端到端：真实下载 en→zh 模型（约100MB，首次运行耗时几分钟）并翻译
    #[tokio::test]
    #[ignore]
    async fn test_offline_mt_en2zh() {
        std::env::set_var("SMART_TRANSLATOR_MODELS_DIR", "target/mt-models");
        let engine = OfflineMtEngine::new();
        let t0 = std::time::Instant::now();
        let out = engine.translate("Hello world, this is a test.", "en", "zh").await.unwrap();
        let t1 = std::time::Instant::now();
        println!("[offline-mt] en→zh 首次(含加载) {:?}: {out}", t0.elapsed());
        let out2 = engine.translate("The weather is nice today.", "en", "zh").await.unwrap();
        println!("[offline-mt] en→zh 纯推理 {:?}: {out2}", t1.elapsed());
        assert!(out2.chars().any(|c| ('\u{4E00}'..='\u{9FFF}').contains(&c)));
        assert!(out.chars().any(|c| ('\u{4E00}'..='\u{9FFF}').contains(&c)));
    }

    /// 端到端：zh→en（复用已下载缓存）
    #[tokio::test]
    #[ignore]
    async fn test_offline_mt_zh2en() {
        std::env::set_var("SMART_TRANSLATOR_MODELS_DIR", "target/mt-models");
        let engine = OfflineMtEngine::new();
        let out = engine.translate("今天天气真好，我们去公园散步吧。", "zh", "en").await.unwrap();
        println!("[offline-mt] zh→en: {out}");
        assert!(out.to_lowercase().contains("weather") || out.to_lowercase().contains("park"));
    }

    /// 端到端：ja→zh 经英语中转（验证 pivot 与 ja-en 模型下载）
    #[tokio::test]
    #[ignore]
    async fn test_offline_mt_ja2zh_pivot() {
        std::env::set_var("SMART_TRANSLATOR_MODELS_DIR", "target/mt-models");
        let engine = OfflineMtEngine::new();
        let out = engine.translate("私は猫が好きです。", "ja", "zh").await.unwrap();
        println!("[offline-mt] ja→zh (pivot): {out}");
        assert!(out.chars().any(|c| ('\u{4E00}'..='\u{9FFF}').contains(&c)));
    }
}
