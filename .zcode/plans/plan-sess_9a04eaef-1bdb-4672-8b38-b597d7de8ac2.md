# 截图翻译 v2 批次（OCR 选型已按搜索结果更新）

## 1. 翻译引擎/OCR 设置改版
**设置页「翻译引擎」分区：**
- 新增「在线翻译引擎」多选框组：百度✓、有道✓、小牛、DeepL、腾讯云、阿里云、自定义-OpenAI兼容（后五者标"未接入"，仅存配置不实现翻译）→ 绑定 `settings.online_apis`，`build_manager` 只构建启用引擎 → 截图菜单自动双向同步
- API 密钥区扩展：小牛/DeepL/腾讯云/阿里云各自动+密钥输入、**自定义-OpenAI兼容（Base URL/API Key/模型名）**，全部持久化到 `settings.providers`
**OCR 列表（单选，同款样式）：**
- Windows内置OCR（已接入，默认）
- **RapidOCR 本地引擎（本轮接入）**：PaddleOCR 模型 ONNX 版，免费离线无限量、中文质量高；用 crates.io 现成 crate（`rapidocr-core`，备选 `paddle-ocr-rs`），det/rec/cls 模型约15MB 随应用打包；捕获区域→推理→行框映射为同构 OcrLineInfo
- **有道OCR（本轮接入）**：区域PNG→Base64→ocrapi 云接口（体验金计费），签名按官方规范
- Tesseract/PaddleOCR原生：标"未接入"禁用
- 迁移：默认 ocr_engine 由无效的 "tesseract" 改为 "windows"
- 风险：RapidOCR 的 Rust crate 较新，若集成受阻降级为仅有道OCR，RapidOCR 挪下轮

## 2. 报错显示在框选区域内
- processSelection 失败：保留 selection、清空 blocks、进入 result 阶段带错误（不再回退框选模式）
- 错误显示：选区水平垂直居中、红色字体（深色半透明底）；选区过小（宽<180或高<70）→ 红色圆圈感叹号
- 菜单栏报错状态正常显示；切换引擎 = 用新引擎重新执行截图翻译（重试）；删除原底部 toast

## 3. 保存/恢复按钮固定底部
.settings-panel 改 flex 纵向：内容区滚动、.settings-actions 固定面板底部

## 4. 截图菜单新增语言对组件
- 菜单组新增 "lang" 组件（默认启用）：点击弹垂直菜单 = 原文区（自动检测✓）+ ⇄反转 + 译文区（中文✓）
- 语言：自动检测、中文、英语、日语、韩语、俄语、法语、德语、西班牙语、葡萄牙语（百度/有道共同支持）
- lang_code 扩展 ru/fr/de/es/pt 映射；翻译 from/to 改用所选语言对（原写死 auto→zh）；持久化到 settings.default_translation_direction

## 5. 界面杂项 + 启动延迟优化
- 关闭按钮改正方形（30×30 居中✕）
- 主窗口尺寸记忆：window_size_mode（"上次大小"默认/"固定大小"+宽高输入）；启动按模式应用；点×隐藏时保存当前尺寸；基本设置页加选项
- 截图触发卡顿：启动后对截图窗口 show→hide 预热一次（强制 WebView2 合成管线初始化）；若仍卡，下轮考虑"常驻透明+点击穿透"架构

## 收尾
- cargo test 全量 + npm run build；git 提交；更新 当前开发进度.txt