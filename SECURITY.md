# Security Policy
Security contact: @LargeModGames

## Supported Versions

We maintain security fixes only for the current stable release stream.

| Version | Supported          |
| ------- | ------------------ |
| 0.45  | :white_check_mark: |
| < 0.45  | :x:                |

## Desktop app

The desktop app (preview) shows the browser GUI's page in the system WebView (WebView2 on Windows, WKWebView on macOS, WebKitGTK on Linux) instead of a browser tab. The page still comes from spotatui's own server on `127.0.0.1`, with the same protections:

- the server listens on loopback only, and the window opens with a one-time launch code, valid for 60 seconds, that the page trades for a session token over its socket;
- every request must name the exact loopback host and port, and a request with an `Origin` must come from that same origin, which blocks DNS rebinding and other sites;
- the page gets no Tauri IPC: the app registers no commands and grants the page no capabilities;
- the window only navigates within that loopback origin, so a link cannot load another site inside the app.

The installers are not code-signed yet. Download them only from this repository's GitHub releases, and compare a download with its line in the release's `checksums.txt` when in doubt.

The app updates itself from the latest stable release's `latest.json`. It installs an update only when the bundle's signature matches the public key built into the app, so a changed download or a release asset uploaded without the private key is refused. The private key exists only as a repository secret and on the maintainer's machine.

## Reporting a Vulnerability

- Please report vulnerabilities privately via GitHub Security Advisories (Security > Report a vulnerability) instead of opening a public issue.
- Include details that help us reproduce and assess impact: spotatui version, OS, installation method, minimal repro steps, and any relevant logs (with secrets stripped).
- You can expect an acknowledgment within 3 business days and status updates at least weekly until resolution.
- If the issue is confirmed, we will coordinate a fix and release notes before public disclosure.***
