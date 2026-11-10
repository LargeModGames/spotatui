# Use spotatui from ChatGPT

ChatGPT can control spotatui and act as your DJ: in the browser, in the desktop app and in the ChatGPT app on your phone, in an ordinary chat with your memories.

A coding agent starts `spotatui mcp` itself (see [MCP setup](mcp-setup.md)). ChatGPT cannot: a chat runs on OpenAI's servers, not on your computer. OpenAI's **Secure MCP Tunnel** connects the two. A small program on your computer, `tunnel-client`, opens an outgoing connection to OpenAI and starts `spotatui mcp` for ChatGPT. No port opens on your computer.

```text
ChatGPT ──> Secure MCP Tunnel (OpenAI) ──> tunnel-client ──stdio──> spotatui mcp ──loopback TCP──> running spotatui
```

## What you need

- **spotatui with MCP on.** Do steps 1 to 4 of [MCP setup](mcp-setup.md): `spotatui mcp status` must print `ok` three times.
- **A ChatGPT plan with custom MCP servers.** On chatgpt.com, open **Plugins**, then **Add**. If the menu has **Add custom MCP server**, your plan can do this. It works on Plus.
- **An OpenAI Platform login** at [platform.openai.com](https://platform.openai.com), with the same account. The tunnel and its key worked on an account with no API credits.
- **Your computer on**, with `tunnel-client` and spotatui running, when you want to use it.
- **On Windows, PowerShell 7.1 or later** (`pwsh`) for the commands on this page. Windows PowerShell 5.1 has no `Read-Host -MaskInput`.

## Set it up

### 1. Create a tunnel

On platform.openai.com, open **Settings**, then **Tunnels**, then **Create tunnel**. Give it a name and a description, and under **ChatGPT workspaces** pick your workspace. Click **Create** and copy the tunnel ID (`tunnel_` and 32 characters).

### 2. Create a runtime key

Open **API keys**, then **Create new secret key**:

- **Permissions:** Restricted.
- **Tunnels:** Read and Use. Leave every other permission at None.

Copy the key. OpenAI shows it only once. The key can do nothing but run your tunnel. If it leaks, delete it and create a new one.

### 3. Get tunnel-client

- **Windows:** download `tunnel-client-runtime-<version>-windows-<arch>.zip` from [the releases](https://github.com/openai/tunnel-client/releases), check it against `SHA256SUMS.txt`, and unzip it. The program is `tunnel-client-runtime.exe` in that folder.
- **Linux:** download `tunnel-client-runtime-<version>-linux-<arch>.zip` the same way, unzip it, and install it: `mkdir -p ~/.local/bin && install -m 755 tunnel-client-runtime ~/.local/bin/`.
- **macOS:** `brew install openai/tools/tunnel-client`. The program is `tunnel-client`.

### 4. Start it

Make sure spotatui runs, then start the client with your tunnel ID. The key prompt does not show what you type.

PowerShell:

```powershell
$env:CONTROL_PLANE_API_KEY = Read-Host -MaskInput 'Tunnel key'
.\tunnel-client-runtime.exe run --mcp.command "spotatui mcp" --control-plane.tunnel-id tunnel_<your id>
```

bash or zsh (on macOS, use `tunnel-client` in place of `~/.local/bin/tunnel-client-runtime`):

```bash
printf 'Tunnel key: '; read -rs CONTROL_PLANE_API_KEY; echo; export CONTROL_PLANE_API_KEY
~/.local/bin/tunnel-client-runtime run --mcp.command "spotatui mcp" --control-plane.tunnel-id tunnel_<your id>
```

The client is up when `http://127.0.0.1:8080/readyz` answers `ready`. That does not prove the key works: give it a minute, then open `http://127.0.0.1:8080/health/control-plane`. It must show `"state":"polling"`, a `last_success` time and `"consecutive_failures":0`. Keep the client running.

### 5. Add the plugin to ChatGPT

On chatgpt.com, open **Plugins**, then **Add**, then **Add custom MCP server**:

- **Name:** spotatui.
- **Connection:** Tunnel. Pick your tunnel, or paste its ID.
- **Authentication:** No authentication. spotatui has no login of its own, and ChatGPT reaches the tunnel only from the workspaces you picked in step 1.

Tick **I understand and want to continue**, click **Create as a plugin**, then **Connect spotatui**.

### 6. Try it

Start a new chat, type `@spotatui`, pick it, and ask what is playing. ChatGPT shows the current track. The plugin belongs to your account, so it also works in the desktop app and on your phone.

## Run it in the background

### Windows

A scheduled task starts the client at login, with no window, and a small script starts it again when it stops.

Save your key in a file that only you can read:

```powershell
$dir = "$env:LOCALAPPDATA\tunnel-client"
New-Item -ItemType Directory -Force $dir | Out-Null
Set-Content -NoNewline -Path "$dir\runtime.key" -Value (Read-Host -MaskInput 'Tunnel key')
icacls "$dir\runtime.key" /inheritance:r /grant:r "${env:USERNAME}:F"
```

Put the unzipped client in `%LOCALAPPDATA%\tunnel-client\bin`, and save this as `%LOCALAPPDATA%\tunnel-client\start-tunnel.ps1` with your tunnel ID:

```powershell
$dir = $PSScriptRoot
$keyFile = Join-Path $dir 'runtime.key'
$log = Join-Path $dir 'tunnel-client.log'
$tunnelId = 'tunnel_<your id>'

while ($true) {
  if ((Test-Path $log) -and (Get-Item $log).Length -gt 5MB) { Remove-Item $log }
  if (-not (Test-Path $keyFile)) {
    "$(Get-Date -Format s) missing $keyFile, retry in 60 s" | Add-Content $log
    Start-Sleep -Seconds 60
    continue
  }
  $env:CONTROL_PLANE_API_KEY = (Get-Content -Raw $keyFile).Trim()
  "=== $(Get-Date -Format s) start" | Add-Content $log
  & (Join-Path $dir 'bin\tunnel-client-runtime.exe') run --mcp.command 'spotatui mcp' --control-plane.tunnel-id $tunnelId *>> $log
  "=== $(Get-Date -Format s) exit $LASTEXITCODE, restart in 15 s" | Add-Content $log
  Start-Sleep -Seconds 15
}
```

Register the task and start it (PowerShell 7):

```powershell
$script = "$env:LOCALAPPDATA\tunnel-client\start-tunnel.ps1"
$pwsh = (Get-Command pwsh).Source
$action = New-ScheduledTaskAction -Execute "$env:SystemRoot\System32\conhost.exe" -Argument "--headless `"$pwsh`" -NoProfile -NonInteractive -ExecutionPolicy Bypass -File `"$script`""
$trigger = New-ScheduledTaskTrigger -AtLogOn -User "$env:USERDOMAIN\$env:USERNAME"
$settings = New-ScheduledTaskSettingsSet -ExecutionTimeLimit ([TimeSpan]::Zero) -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries -StartWhenAvailable -MultipleInstances IgnoreNew
$principal = New-ScheduledTaskPrincipal -UserId "$env:USERDOMAIN\$env:USERNAME" -LogonType Interactive -RunLevel Limited
Register-ScheduledTask -TaskName 'spotatui ChatGPT tunnel' -Action $action -Trigger $trigger -Settings $settings -Principal $principal
Start-ScheduledTask 'spotatui ChatGPT tunnel'
```

The log is `%LOCALAPPDATA%\tunnel-client\tunnel-client.log`. `Stop-ScheduledTask 'spotatui ChatGPT tunnel'` stops it and `Unregister-ScheduledTask 'spotatui ChatGPT tunnel'` removes it.

### macOS

Not tested. Homebrew can run the client at login with `brew services`, which reads a profile file. Save your key in a file that only you can read:

```bash
mkdir -p ~/.config/tunnel-client && chmod 700 ~/.config/tunnel-client
printf 'Tunnel key: '; read -rs key; echo
printf '%s' "$key" > ~/.config/tunnel-client/runtime.key && chmod 600 ~/.config/tunnel-client/runtime.key; unset key
```

Then save this as `~/.config/tunnel-client/spotatui.yaml`, with your tunnel ID, your home folder and the absolute path of `spotatui` (`command -v spotatui`):

```yaml
config_version: 1
control_plane:
  tunnel_id: tunnel_<your id>
  api_key: file:/Users/<you>/.config/tunnel-client/runtime.key
mcp:
  commands:
    - channel: main
      command: /absolute/path/to/spotatui mcp
```

Check it with `env -u CONTROL_PLANE_API_KEY tunnel-client doctor --profile-file ~/.config/tunnel-client/spotatui.yaml` (the `env -u` makes it read the key file, not a key left in your shell from step 4), then follow [Start at macOS login](https://github.com/openai/tunnel-client#start-at-macos-login) in the tunnel-client README with this file as your profile. `doctor` checks the key's format, not whether OpenAI accepts it, so after the service starts, check `/health/control-plane` as in step 4.

### Linux

Not tested. A systemd user service can run it. Save the key as `CONTROL_PLANE_API_KEY=<key>` in `~/.config/tunnel-client/env` with `chmod 600`, then create `~/.config/systemd/user/spotatui-tunnel.service`:

```ini
[Unit]
Description=Secure MCP Tunnel for spotatui
After=network-online.target

[Service]
EnvironmentFile=%h/.config/tunnel-client/env
ExecStart=%h/.local/bin/tunnel-client-runtime run --mcp.command "/absolute/path/to/spotatui mcp" --control-plane.tunnel-id tunnel_<your id>
Restart=always
RestartSec=15

[Install]
WantedBy=default.target
```

`ExecStart` uses the `~/.local/bin` install from step 3. Give `spotatui` as an absolute path: a user service does not read your shell profile, so `spotatui` may not be on its `PATH`. Then `systemctl --user enable --now spotatui-tunnel`.

## Good to know

- **Run one client per tunnel.** A stdio tunnel takes one client at a time. Stop the one in your terminal before you start the background one.
- **spotatui can start later.** The tools stay visible in ChatGPT while spotatui is closed. A call then returns "spotatui is not available" until you start spotatui.
- **Your data passes through OpenAI.** Tool calls and their results go through the tunnel and ChatGPT, like everything else in the chat. `get_listening_history` sends your top artists, tracks and albums, your recent plays and your play count, the current track, and the DJ vibe you set, which is your own text. It sends names, no IDs or timestamps.
- **The key only runs the tunnel.** Anyone who can use your ChatGPT account can control your player through the plugin.

## Troubleshooting

| Symptom | Cause and fix |
|---|---|
| No **Add custom MCP server** in the **Add** menu | Your ChatGPT plan does not allow custom MCP servers. |
| The log says `control plane API key is malformed` | The key is wrong or the key file holds something else. Save the key again. |
| ChatGPT says the plugin is not available | `tunnel-client` is not running, or its polls fail. Check `http://127.0.0.1:8080/health/control-plane`, then read the log before you change the key. A polling `401` or `403` means a wrong tunnel ID, a revoked key, or a key without Tunnels Read and Use. A DNS or network error is a connection problem, and a new key does not help. |
| Calls return "spotatui is not available" | spotatui is not running, or `mcp_enabled` is off. Start spotatui and check `spotatui mcp status`. |
