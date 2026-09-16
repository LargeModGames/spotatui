@echo off
REM degen-radio installer for Windows CMD.
REM
REM   curl -fsSL https://raw.githubusercontent.com/ethereumdegen/degen-radio/main/install.cmd -o install.cmd && install.cmd
REM
REM Delegates to the PowerShell installer so there is a single source of truth.
powershell -NoProfile -ExecutionPolicy Bypass -Command "irm https://raw.githubusercontent.com/ethereumdegen/degen-radio/main/install.ps1 | iex"
