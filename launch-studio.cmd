@echo off
rem Double-click entry point. No project is opened or created automatically.
start "" powershell.exe -NoProfile -WindowStyle Hidden -ExecutionPolicy RemoteSigned -File "%~dp0launch-studio.ps1"
