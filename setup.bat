@echo off
echo ========================================
echo 智能翻译软件 - 开发环境设置脚本
echo ========================================
echo.

echo 1. 检查Node.js和npm...
node --version
if %errorlevel% neq 0 (
    echo 错误: 未找到Node.js，请先安装Node.js
    echo 下载地址: https://nodejs.org/
    pause
    exit /b 1
)

npm --version
if %errorlevel% neq 0 (
    echo 错误: 未找到npm，请重新安装Node.js
    pause
    exit /b 1
)

echo 2. 检查Rust...
rustc --version
if %errorlevel% neq 0 (
    echo 警告: 未找到Rust，将尝试安装...
    echo 请从 https://rustup.rs/ 下载并安装Rust
    echo 安装完成后重新运行此脚本
    pause
    exit /b 1
)

cargo --version
if %errorlevel% neq 0 (
    echo 错误: 未找到cargo，请检查Rust安装
    pause
    exit /b 1
)

echo 3. 安装Tauri CLI...
npm install -g @tauri-apps/cli
if %errorlevel% neq 0 (
    echo 警告: Tauri CLI安装失败，尝试使用本地安装
    npm install @tauri-apps/cli
)

echo 4. 创建项目目录...
if not exist "smart-translator" mkdir smart-translator
cd smart-translator

echo 5. 初始化Tauri项目...
echo 正在创建Tauri项目结构...
mkdir src-tauri
mkdir src
mkdir src\components
mkdir src\styles
mkdir public

echo 6. 创建package.json...
echo { > package.json
echo   "name": "smart-translator", >> package.json
echo   "version": "0.1.0", >> package.json
echo   "scripts": { >> package.json
echo     "dev": "vite", >> package.json
echo     "build": "tsc && vite build", >> package.json
echo     "tauri": "tauri" >> package.json
echo   }, >> package.json
echo   "dependencies": { >> package.json
echo     "@tauri-apps/api": "^2.0.0", >> package.json
echo     "react": "^18.2.0", >> package.json
echo     "react-dom": "^18.2.0" >> package.json
echo   }, >> package.json
echo   "devDependencies": { >> package.json
echo     "@tauri-apps/cli": "^2.0.0", >> package.json
echo     "@types/react": "^18.2.0", >> package.json
echo     "@types/react-dom": "^18.2.0", >> package.json
echo     "@vitejs/plugin-react": "^4.0.0", >> package.json
echo     "typescript": "^5.0.0", >> package.json
echo     "vite": "^4.4.0" >> package.json
echo   } >> package.json
echo } >> package.json

echo 7. 安装前端依赖...
call npm install

echo 8. 创建Tauri配置...
echo { > src-tauri\tauri.conf.json
echo   "build": { >> src-tauri\tauri.conf.json
echo     "beforeDevCommand": "npm run dev", >> src-tauri\tauri.conf.json
echo     "beforeBuildCommand": "npm run build", >> src-tauri\tauri.conf.json
echo     "devPath": "http://localhost:5173", >> src-tauri\tauri.conf.json
echo     "distDir": "../dist" >> src-tauri\tauri.conf.json
echo   }, >> src-tauri\tauri.conf.json
echo   "package": { >> src-tauri\tauri.conf.json
echo     "productName": "Smart Translator", >> src-tauri\tauri.conf.json
echo     "version": "0.1.0" >> src-tauri\tauri.conf.json
echo   }, >> src-tauri\tauri.conf.json
echo   "tauri": { >> src-tauri\tauri.conf.json
echo     "windows": [ >> src-tauri\tauri.conf.json
echo       { >> src-tauri\tauri.conf.json
echo         "title": "智能翻译软件", >> src-tauri\tauri.conf.json
echo         "width": 1200, >> src-tauri\tauri.conf.json
echo         "height": 800 >> src-tauri\tauri.conf.json
echo       } >> src-tauri\tauri.conf.json
echo     ] >> src-tauri\tauri.conf.json
echo   } >> src-tauri\tauri.conf.json
echo } >> src-tauri\tauri.conf.json

echo 9. 创建Cargo.toml...
echo [package] > src-tauri\Cargo.toml
echo name = "smart-translator" >> src-tauri\Cargo.toml
echo version = "0.1.0" >> src-tauri\Cargo.toml
echo edition = "2021" >> src-tauri\Cargo.toml
echo. >> src-tauri\Cargo.toml
echo [dependencies] >> src-tauri\Cargo.toml
echo tauri = { version = "2.0", features = ["shell-open"] } >> src-tauri\Cargo.toml
echo serde = { version = "1.0", features = ["derive"] } >> src-tauri\Cargo.toml
echo serde_json = "1.0" >> src-tauri\Cargo.toml

echo 10. 完成设置！
echo.
echo 项目已创建在: %cd%
echo.
echo 下一步操作:
echo 1. 启动前端开发服务器: npm run dev
echo 2. 启动Tauri开发模式: tauri dev
echo.
echo 按任意键退出...
pause > nul