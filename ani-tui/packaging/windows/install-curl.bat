@echo off
setlocal

powershell -NoProfile -ExecutionPolicy Bypass -Command "Invoke-WebRequest -Uri 'https://github.com/silent9669/weeb-tui/releases/latest/download/install.ps1' -OutFile '%TEMP%\weeb-tui-install.ps1' -UseBasicParsing; & '%TEMP%\weeb-tui-install.ps1'"
exit /b %ERRORLEVEL%
