# 离线翻译引擎（OPUS-MT 本地模型 + ONNX Runtime）

## 技术方案
沿用项目既有栈：直接依赖 `ort = "2.0.0-rc.12"`（与 rapidocr-core 完全同版本，复用同一 ONNX Runtime，Windows 构建零新增工具链）+ Hugging Face 官方 `tokenizers` 纯 Rust crate。模型用 Xenova 镜像的 Helsinki-NLP OPUS-MT int8 量化版（已验证 en-zh/zh-en/ja-en/ko-en 仓库文件齐全：tokenizer.json + encoder_model_quantized.onnx + decoder_model_quantized.onnx）。decoder 用无缓存版全量解码（句子短，O(n²) 可接受，实现简单可靠）。

## 用户已确认的三项决策
- 模型按需自动下载（与 RapidOCR 同模式，每语言对约 30-80MB）
- 默认启用 + 在线引擎失败时自动兜底；截图菜单也可手动选中
- 中↔英专门模型直达，日/韩/俄/法/德/西/葡 ↔ 中经英语中转（两次解码）

## 实施步骤

### 1. 依赖（Cargo.toml）
- `ort = "2.0.0-rc.12"`（与 rapidocr-core 同版本）
- `tokenizers = { default-features = false, features = ["onig"] }`

### 2. 新模块 src-tauri/src/offline_mt.rs
- **模型目录 catalog**：zh↔en 直达 2 对 + 其余 7 语言 ↔ en 各 1 对（共 16 个小模型 repo 映射）
- **下载器 ensure_pair_models**：hf-mirror.com 优先（本环境已验证官方站直连不通、镜像可达）→ huggingface.co 兜底；存 exe 同目录 models/mt/<repo>/；进度通过 `app_handle.emit_to("main", "offline-mt-status", 百分比消息)` 推送
- **MtPair 会话缓存**：static Mutex<HashMap<对, 会话>>，懒加载；输入输出按 session 元数据名 introspect（encoder_hidden_states/encoder_attention_mask/decoder_input_ids）
- **贪心解码**：decoder_start=<pad>（token_to_id 取）、终止 </s>、上限 256 token、logits 末位 argmax
- **auto 语言检测**：谚文→ko、假名→ja、汉字→zh、西里尔→ru、拉丁→en
- **pivot 路由**：X→Y 无直达模型时拆成 X→en→Y 两次解码
- **OfflineMtEngine**：实现 TranslationEngine（name=“离线翻译”，EngineType::Offline 已有占位），translate 内部 spawn_blocking
- 命令层事件句柄传递：通过 tauri::AppHandle 全局 static 或参数注入，供下载进度推送

### 3. 引擎接入（translation.rs）
build_manager 中 `offline_engine != "disabled"` 时把 OfflineMtEngine **append 到列表末尾**（在线优先、离线兜底；手动切换截图菜单/当前翻译源也能选中）。online_apis 过滤逻辑不变。

### 4. 设置（system.rs + SettingsPanel.tsx）
- 复用现有 `offline_engine` 字段（"marian"=启用 / "disabled"=禁用）；migrate 增加 argos→marian
- 设置页现有“离线翻译引擎”下拉改为：**离线翻译 OPUS-MT（本地免费无网络）/ 禁用** + 说明文字（首次使用按语言对自动下载约30-80MB，模型存本地）

### 5. 前端状态显示（ScreenshotWindow.tsx）
processing 面板新增状态行监听 `offline-mt-status` 事件（“正在下载模型 35%…"/“首次加载模型…”），消除长下载期间的静默等待；翻译测试页无需改动（engines 列表自动含“离线翻译”）。

### 6. 测试（真实网络执行）
- 单测：catalog 路由/pivot 拆分/脚本检测
- #[ignore] 端到端：真实下载 en-zh 模型 → "Hello world"→中文、中文→English、"私は猫です"→zh（pivot）——实现后立即真实运行验证
- 全量 cargo test + npm run build

### 7. 收尾
更新 当前开发进度.txt + 快速启动指南.md（引擎表加离线翻译）；git 提交（不含模型文件/密钥）。

## 风险与兜底
- 个别语言对仓库缺失（如 en→ja 未验证成功）：catalog 只收录确认存在的 repo，下载 404 时报“该语言对暂无离线模型”，测试阶段逐一验证 16 个仓库
- 首次使用下载耗时：进度事件 + processing 面板状态行兜底体验
- 解码无 KV cache 首版性能：典型短句 1-2s；后续可升级 decoder_model_merged+cache