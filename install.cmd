@echo off
REM spotatui installer for Windows CMD.
REM
REM   curl -fsSL https://spotatui.com/install.cmd -o install.cmd && install.cmd
REM
REM Delegates to the PowerShell installer so there is a single source of truth.
powershell -NoProfile -ExecutionPolicy Bypass -Command "irm https://spotatui.com/install.ps1 | iex"
