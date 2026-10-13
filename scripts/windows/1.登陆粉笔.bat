@echo off
chcp 65001 >nul
cd /d "%~dp0"
mistake-compass.exe login
echo 按回车键关闭
pause >nul
