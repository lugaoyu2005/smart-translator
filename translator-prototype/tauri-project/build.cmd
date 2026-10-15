@echo off
chcp 65001 >nul
setlocal
title Smart Translator - 一键打包更新

rem ============================================================
rem  Smart Translator 一键打包 / 部署脚本
rem  用法：双击本文件；或 build.cmd -SkipTests / -NoDeploy
rem  流程：环境自检 -> 版本号校验 -> 前端检查 -> Rust 编译
rem        -> 打包 exe/MSI/NSIS -> 备份旧 exe -> 覆盖部署目录 -> 启动
rem ============================================================

set "ROOT=%~dp0"
if "%ROOT:~-1%"=="\" set "ROOT=%ROOT:~0,-1%"
set "PROJ=%ROOT%\src\translator-prototype\tauri-project"
set "CARGO_BIN=%USERPROFILE%\.cargo\bin"
set "ORT_LIB=%USERPROFILE%\.ort\onnxruntime-win-x64-1.28.0\lib"
set "VCVARS=C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat"
rem 用不含括号的短路径，避免在 if(...) 块内被 cmd 当作块结束符
for %%I in ("%VCVARS%") do set "VCVARS_SHORT=%%~sI"

set "SKIP_TESTS=0"
set "NO_DEPLOY=0"
for %%A in (%*) do (
  if /I "%%A"=="-SkipTests" set "SKIP_TESTS=1"
  if /I "%%A"=="-NoDeploy"  set "NO_DEPLOY=1"
)

echo.
echo ============================================================
echo   Smart Translator 一键打包
echo   项目：%PROJ%
echo   部署：%ROOT%
echo ============================================================
echo.

rem ---------- [1/7] 环境自检 ----------
echo [1/7] 环境自检...
if not exist "%VCVARS%" (
  echo   [X] 找不到 MSVC: %VCVARS_SHORT%
  echo       请安装 Visual Studio 2022 Build Tools^(含 VCTools 工作负载^)
  goto :fail
)
if not exist "%CARGO_BIN%\cargo.exe" (
  echo   [X] 找不到 Rust: %CARGO_BIN%\cargo.exe
  echo       请安装 rustup ^(stable-x86_64-pc-windows-msvc^)
  goto :fail
)
if not exist "%ORT_LIB%\onnxruntime.lib" (
  echo   [X] 找不到 ONNX Runtime: %ORT_LIB%
  echo       ort 官方 CDN 在部分网络不可达，必须本地放一份：
  echo       下载 onnxruntime-win-x64-1.28.0.zip 解压到 %USERPROFILE%\.ort\onnxruntime-win-x64-1.28.0
  goto :fail
)
if not exist "%PROJ%\package.json" (
  echo   [X] 找不到前端工程: %PROJ%
  goto :fail
)
echo   OK  MSVC / Rust / ONNX Runtime / 工程目录 均就绪

rem ---------- [2/7] 版本号一致性 ----------
echo.
echo [2/7] 校验版本号一致性...
for /f "tokens=2 delims== " %%v in ('findstr /R /C:"^version" "%PROJ%\src-tauri\Cargo.toml"') do set "V_CARGO=%%~v"
for /f "tokens=2 delims=:," %%v in ('findstr /R /C:"\"version\"" "%PROJ%\src-tauri\tauri.conf.json"') do (
  if not defined V_CONF set "V_CONF=%%~v"
)
set "V_CONF=%V_CONF: =%"
set "V_CONF=%V_CONF:"=%"
findstr /C:">版本: %V_CARGO%<" "%PROJ%\src\components\SettingsPanel.tsx" >nul 2>&1
rem findstr 在中文代码页下匹配 UTF-8 中文会失败，所以这里只匹配版本号附近的 ASCII 片段
findstr /C:"%V_CARGO%<" "%PROJ%\src\components\SettingsPanel.tsx" >nul 2>&1
if errorlevel 1 (
  echo   [!] 关于页版本号与 Cargo.toml^(%V_CARGO%^) 不一致
  echo       请把 src\components\SettingsPanel.tsx 关于页的 <p>版本: ...</p> 改为 %V_CARGO%
  echo       同时改 src\App.test.tsx 里对应的 findByText 断言，否则测试会红
  goto :fail
)
if not "%V_CARGO%"=="%V_CONF%" (
  echo   [!] Cargo.toml^(%V_CARGO%^) 与 tauri.conf.json^(%V_CONF%^) 不一致
  goto :fail
)
echo   OK  版本号 %V_CARGO% ^(Cargo.toml / tauri.conf.json / 关于页 三处一致^)

rem ---------- [3/7] 前端检查 ----------
echo.
if "%SKIP_TESTS%"=="1" (
  echo [3/7] 已跳过前端检查 ^(-SkipTests^)
) else (
  echo [3/7] 前端检查 ^(tsc + vitest^)...
  pushd "%PROJ%"
  call npx tsc --noEmit
  if errorlevel 1 ( popd & echo   [X] 类型检查失败 & goto :fail )
  call npx vitest run
  if errorlevel 1 ( popd & echo   [X] 前端测试失败 & goto :fail )
  popd
  echo   OK  类型检查与前端测试通过
)

rem ---------- [4/7] 进入 MSVC 环境 ----------
echo.
echo [4/7] 加载 MSVC 构建环境...
call "%VCVARS%" >nul
if errorlevel 1 ( echo   [X] vcvars64.bat 执行失败 & goto :fail )
set "PATH=%CARGO_BIN%;%PATH%"
set "ORT_LIB_LOCATION=%ORT_LIB%"
echo   OK  link.exe / ORT_LIB_LOCATION 已就位

rem ---------- [5/7] 构建 ----------
echo.
echo [5/7] 编译并打包^(首次约 10-25 分钟，增量约 1-3 分钟^)...
pushd "%PROJ%"
call npm run tauri build
set "RC=%ERRORLEVEL%"
popd
if not "%RC%"=="0" ( echo   [X] 构建失败^(exit %RC%^) & goto :fail )
set "REL=%PROJ%\src-tauri\target\release"
if not exist "%REL%\smart-translator.exe" ( echo   [X] 未找到产物 exe & goto :fail )
echo   OK  产物: %REL%\smart-translator.exe

rem ---------- [6/7] 部署 ----------
echo.
if "%NO_DEPLOY%"=="1" (
  echo [6/7] 已跳过部署 ^(-NoDeploy^)
  goto :done
)
echo [6/7] 部署到 %ROOT% ...
tasklist /FI "IMAGENAME eq smart-translator.exe" 2>nul | findstr /I "smart-translator.exe" >nul
if not errorlevel 1 (
  echo   检测到应用正在运行，先结束进程...
  taskkill /IM smart-translator.exe /F >nul 2>&1
  timeout /t 2 /nobreak >nul
)
if exist "%ROOT%\smart-translator.exe" (
  copy /Y "%ROOT%\smart-translator.exe" "%ROOT%\smart-translator.exe.v%V_CARGO%.prev.bak" >nul
  echo   已备份旧版本为 smart-translator.exe.v%V_CARGO%.prev.bak
)
copy /Y "%REL%\smart-translator.exe" "%ROOT%\smart-translator.exe" >nul
if errorlevel 1 ( echo   [X] 复制 exe 失败 & goto :fail )
if not exist "%ROOT%\onnxruntime.dll" (
  copy /Y "%ORT_LIB%\onnxruntime.dll" "%ROOT%\onnxruntime.dll" >nul
  echo   已补装 onnxruntime.dll
)
echo   OK  已更新 %ROOT%\smart-translator.exe

rem ---------- [7/7] 启动 ----------
echo.
echo [7/7] 启动新版本...
start "" "%ROOT%\smart-translator.exe"
timeout /t 3 /nobreak >nul
tasklist /FI "IMAGENAME eq smart-translator.exe" 2>nul | findstr /I "smart-translator.exe" >nul
if errorlevel 1 (
  echo   [!] 进程未检测到，请手动双击 smart-translator.exe 确认
) else (
  echo   OK  应用已启动
)

:done
echo.
echo ============================================================
echo   完成  版本 v%V_CARGO%
echo   绿色版：%ROOT%\smart-translator.exe
echo   安装包：%REL%\bundle\msi\  ^|  %REL%\bundle\nsis\
echo ============================================================
echo.
pause
exit /b 0

:fail
echo.
echo ============================================================
echo   失败，未完成部署。请按上面的提示处理后重试。
echo ============================================================
echo.
pause
exit /b 1
