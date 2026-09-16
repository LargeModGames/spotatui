<#
.SYNOPSIS
  degen-radio installer for Windows.

  irm https://raw.githubusercontent.com/ethereumdegen/degen-radio/main/install.ps1 | iex

  Environment overrides:
    DEGEN_RADIO_VERSION        install a specific tag; default: latest
    DEGEN_RADIO_INSTALL_DIR    where to put the binary; default: %LOCALAPPDATA%\degen-radio\bin
    DEGEN_RADIO_NO_MODIFY_PATH set to any value to skip updating your user PATH
#>
$ErrorActionPreference = 'Stop'

$Repo   = 'ethereumdegen/degen-radio'
$Binary = 'degen-radio'
$InstallDir = if ($env:DEGEN_RADIO_INSTALL_DIR) { $env:DEGEN_RADIO_INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA 'degen-radio\bin' }

function Info($m) { Write-Host "· $m" -ForegroundColor DarkGray }
function Ok($m)   { Write-Host "√ $m" -ForegroundColor Green }
function Warn($m) { Write-Host "! $m" -ForegroundColor Yellow }
function Fail($m) { Write-Host "x $m" -ForegroundColor Red; exit 1 }

# --- detect arch -----------------------------------------------------------
$arch = $env:PROCESSOR_ARCHITECTURE
if ($arch -ne 'AMD64') {
  if ($arch -eq 'ARM64') {
    Warn 'No native ARM64 Windows build yet; installing the x64 build (runs under emulation).'
  } else {
    Fail "Unsupported architecture '$arch'. Build from source instead: cargo install --locked degen-radio"
  }
}
$asset = "$Binary-windows-x86_64.zip"

# --- resolve version / URL -------------------------------------------------
if ($env:DEGEN_RADIO_VERSION) {
  $tag = 'v' + ($env:DEGEN_RADIO_VERSION -replace '^v', '')
  $base = "https://github.com/$Repo/releases/download/$tag"
  $label = $tag
} else {
  $base = "https://github.com/$Repo/releases/latest/download"
  $label = 'latest'
}

Info "installing degen-radio ($label) for windows/x86_64"

$tmp = Join-Path ([System.IO.Path]::GetTempPath()) ("degen-radio-" + [System.Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $tmp -Force | Out-Null
try {
  $zip = Join-Path $tmp $asset
  try {
    Invoke-WebRequest -Uri "$base/$asset" -OutFile $zip -UseBasicParsing
  } catch {
    Fail "could not download $asset. That build may not exist yet for this release; try: cargo install --locked degen-radio"
  }

  # --- verify checksum (best effort) --------------------------------------
  try {
    $sumFile = "$zip.sha256"
    Invoke-WebRequest -Uri "$base/$asset.sha256" -OutFile $sumFile -UseBasicParsing
    $expected = ((Get-Content $sumFile -Raw) -split '\s+' | Where-Object { $_ -match '^[0-9a-fA-F]{64}$' } | Select-Object -First 1)
    $actual = (Get-FileHash -Algorithm SHA256 $zip).Hash
    if ($expected -and $actual -and ($expected -ne $actual)) {
      Fail "checksum mismatch (expected $expected, got $actual); aborting"
    } elseif ($expected) {
      Ok 'checksum verified'
    } else {
      Warn 'no published checksum; skipping verification'
    }
  } catch {
    Warn 'no published checksum; skipping verification'
  }

  # --- extract & install --------------------------------------------------
  Expand-Archive -Path $zip -DestinationPath $tmp -Force
  $exe = Join-Path $tmp "$Binary.exe"
  if (-not (Test-Path $exe)) { Fail "archive did not contain $Binary.exe" }

  New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
  Copy-Item -Path $exe -Destination (Join-Path $InstallDir "$Binary.exe") -Force
  Ok "installed to $InstallDir\$Binary.exe"

  # --- add to user PATH ---------------------------------------------------
  $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
  if (($userPath -split ';') -notcontains $InstallDir) {
    if ($env:DEGEN_RADIO_NO_MODIFY_PATH) {
      Warn "$InstallDir is not on your PATH. Add it for this session with:"
      Write-Host "    `$env:Path = `"$InstallDir;`$env:Path`""
    } else {
      [Environment]::SetEnvironmentVariable('Path', ((@($userPath, $InstallDir) | Where-Object { $_ }) -join ';'), 'User')
      $env:Path = "$env:Path;$InstallDir"
      Info "added $InstallDir to your user PATH (restart your terminal to pick it up)"
    }
  }
} finally {
  Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
}

Write-Host ''
Write-Host "Done. Run $Binary to get started." -ForegroundColor Green
