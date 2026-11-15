<h1 align="center">
  <img src=".github/spotatui-logo.png" alt="spotatui logo" width="200"><br>
  spotatui
</h1>
<p align="center">
  一个面向终端和桌面的快速音乐播放器，用 Rust 编写。<br>
  通过内置客户端播放 Spotify，另外支持本地文件、Subsonic/Navidrome、网络电台、YouTube 和 Qobuz，macOS 上还支持 Apple Music。
</p>
<p align="center">
  <a href="https://spotatui.com/download"><strong>下载</strong></a> · <a href="#安装">安装</a> · <a href="docs/README.md">文档</a> · <a href="CONTRIBUTING.md">贡献</a> · <a href="https://spotatui.com/discord">Discord</a>
</p>

[![Crates.io](https://img.shields.io/crates/v/spotatui.svg)](https://crates.io/crates/spotatui)
[![Discord](https://img.shields.io/discord/1534820237122207815?logo=discord&logoColor=white&label=Discord&color=5865F2)](https://spotatui.com/discord)
[![Songs played using spotatui](https://img.shields.io/badge/dynamic/json?url=https://spotatui-counter.spotatui.workers.dev&query=count&label=Songs%20played%20using%20spotatui&labelColor=0b0f14&color=1ed760&style=flat-square&cacheSeconds=600)](#播放历史)
[![spotatui Contributors](https://img.shields.io/badge/dynamic/json?url=https://raw.githubusercontent.com/LargeModGames/spotatui/main/.all-contributorsrc&query=%24.contributors.length&label=spotatui%20contributors&color=1ed760&style=flat-square)](#spotatui-贡献者)

**桌面应用**

![spotatui 桌面应用：专辑页面，然后是可视化器、同步歌词和封面艺术的分栏视图](.github/demo-desktop.gif)

**终端应用**

![终端中的 spotatui：一张专辑、封面艺术、同步歌词和五种可视化器样式](.github/demo.gif)

<!-- START doctoc generated TOC please keep comment here to allow auto update -->
<!-- DON'T EDIT THIS SECTION, INSTEAD RE-RUN doctoc TO UPDATE -->
## 目录

- [关于](#关于)
- [安装](#安装)
- [快速上手](#快速上手)
- [音源](#音源)
- [了解更多](#了解更多)
- [贡献](#贡献)
- [播放历史](#播放历史)
- [Star 历史](#star-历史)
- [维护者](#维护者)
- [代码签名政策](#代码签名政策)
- [spotatui 贡献者](#spotatui-贡献者)

<!-- END doctoc generated TOC please keep comment here to allow auto update -->

## 关于

spotatui 是一个拥有两个前端的音乐播放器：桌面应用和终端应用。它们共享同一个登录、同一份配置和同一份收听历史。Spotify 是可选的一一它是七种音源之一。

- 同步歌词，以及实时音频可视化器。
- 一个可以混合不同音源曲目的播放队列。
- [播放列表同步](docs/playlist-sync.md)：把一份播放列表镜像到 Spotify、Qobuz、Subsonic 或 YouTube。
- [Lua 插件](PLUGINS.md)：命令、按键、弹窗和主题。
- 应用内置 [AI DJ](docs/ai-dj.md)，或者让你的编程代理通过 [MCP](docs/mcp-setup.md) 当 DJ。
- 带统计和可分享年度回顾的本地收听历史。
- 用于播放、搜索和播放列表的 [CLI](docs/cli.md)。

**Spotify 需要 Premium。** spotatui 用自己的内置客户端登录，所以你不需要开发者应用，也不需要 client ID。

spotatui 最初是 [spotify-tui](https://github.com/Rigellute/spotify-tui) 的一个分支，之后发展出了自己的播放器、音源和桌面应用。

## 安装

Spotify 是可选的：本地文件、网络电台和 YouTube 无需账户即可使用，Subsonic/Navidrome 和 Qobuz 则需要相应的账户。

**桌面应用**

| 系统 | 下载 |
|---|---|
| Windows 10/11 | [安装程序](https://spotatui.com/download/windows) |
| macOS | [Apple Silicon](https://spotatui.com/download/macos) · [Intel](https://spotatui.com/download/macos-intel) |
| Linux | [.deb](https://spotatui.com/download/linux-deb) · [.rpm](https://spotatui.com/download/linux-rpm) · [AppImage](https://spotatui.com/download/linux-appimage) |

安装程序尚未签名，所以 Windows 和 macOS 会在首次启动时发出警告。[桌面应用](docs/installation.md#desktop-app)说明了如何打开它，并列出适用于 Linux 的 arm64 文件。

**终端应用**

| 系统 | 命令 |
|---|---|
| macOS、Linux、WSL | `curl -fsSL https://spotatui.com/install.sh \| bash` |
| Windows | `irm https://spotatui.com/install.ps1 \| iex` 或 `winget install spotatui` |
| Arch Linux | `yay -S spotatui-bin` |
| 使用 Rust | `cargo install --locked spotatui` |

Homebrew、Nix、Void、`.deb` 包以及从源码构建都在[安装指南](docs/installation.md)中。

- **不同构建的功能：** 预编译的终端二进制包含所有音源、MCP 服务器和 AI DJ。`cargo install` 只构建 Spotify，除非你添加[这些 feature](docs/installation.md#which-build-has-which-features)；Nix flake 也只构建 Spotify。
- **更新：** 终端应用会自动更新。如果它是由包管理器安装的，请设置 `behavior.disable_auto_update: true`。参见[更新](docs/installation.md#updating)。

## 快速上手

运行 `spotatui`。首次启动会询问你想使用哪些音源。选择 Spotify 登录：spotatui 会显示一个代码并打开 Spotify 的配对页面，你在那里确认。

然后：

- `?` 列出所有按键。
- `/` 搜索。用方向键选择 **Songs** 面板，按 `Enter` 进入，再按 `Enter` 播放一首歌。
- `d` 切换音源和设备。
- `z` 把一首歌加入队列，`Shift+Q` 打开队列。
- `spotatui --help` 显示 [CLI](docs/cli.md)。

## 音源

| 音源 | 你需要 |
|---|---|
| **Spotify** | 一个 Premium 账户 |
| **本地文件** | 一个音乐文件夹 |
| **Subsonic / Navidrome** | 一个服务器账户 |
| **网络电台** | 什么都不需要（来自 [radio-browser.info](https://www.radio-browser.info) 的 30k+ 电台） |
| **YouTube** | `PATH` 中有 [yt-dlp](https://github.com/yt-dlp/yt-dlp) |
| **Qobuz** | 一个付费 Qobuz 账户 |
| **Apple Music** | macOS 和「音乐」App |

本地文件、Subsonic、网络电台、YouTube 和 Qobuz 通过 spotatui 自己的音频引擎播放。Apple Music 在「音乐」App 中播放，spotatui 充当它的遥控器。每种音源的设置见[配置](docs/configuration.md)。

Spotify 在 spotatui 本身中播放，也可以在任意 Spotify Connect 设备上播放，或在你网络中的 Cast 和 Connect 音箱上播放。参见 [Spotify 在哪里播放](docs/native-streaming.md#where-spotify-plays)。对于无损音频，或原生串流无法播放的账户，spotatui 可以运行 Spotify 自己的播放器：参见 [Soloist](docs/soloist.md)。

## 了解更多

| 你想 | 阅读 |
|---|---|
| 用其他方式安装，或从源码构建 | [安装](docs/installation.md) |
| 修改设置、按键或主题 | [配置](docs/configuration.md)、[按键绑定](docs/keybindings.md)、[主题](docs/themes.md) |
| 从 shell 控制 spotatui | [CLI](docs/cli.md) |
| 编写或安装 Lua 插件 | [PLUGINS.md](PLUGINS.md)、[脚本 API](docs/scripting.md) |
| 让 AI 挑选音乐 | [AI DJ](docs/ai-dj.md)、[MCP 设置](docs/mcp-setup.md) |
| 报告 bug | [新建 issue](https://github.com/LargeModGames/spotatui/issues/new/choose)，附上 `spotatui --debug` [日志](docs/configuration.md#logging) |
| 其他一切 | [全部文档](docs/README.md) |

## 贡献

spotatui 由一个小的团队和不断壮大的贡献者群体共同构建，你不需要写代码也能帮忙。从 [CONTRIBUTING.md](CONTRIBUTING.md) 开始，或者挑一个 [good first issue](https://github.com/LargeModGames/spotatui/issues?q=is%3Aissue%20state%3Aopen%20label%3A%22good%20first%20issue%22)。

原生播放和 Spotify Connect 运行在我们 fork 的 librespot——[spotatui-librespot](https://github.com/LargeModGames/spotatui-librespot) 上。它是项目的一部分：在那里合并一个 PR 也算作 spotatui 的贡献。参见[为 librespot fork 做贡献](CONTRIBUTING.md#contributing-to-the-librespot-fork)。

## 播放历史

![播放历史](https://spotatui-counter.spotatui.workers.dev/chart.svg)

## Star 历史

<a href="https://www.star-history.com/?type=date&legend=top-left&repos=LargeModGames%2Fspotatui">
 <picture>
   <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/chart?repos=LargeModGames/spotatui&type=date&theme=dark&legend=top-left&sealed_token=iryqtVEb4hcMJpt3uuX43JaGNlZRMV4-stDO4M8-7zH7g186IdrwSCU1PMWBv_YdK7DC4y6qw4sEKu9JXmHbR6c7clp1u8sL1AXI3D_dMW_KTnIjJYKDGw" />
   <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/chart?repos=LargeModGames/spotatui&type=date&legend=top-left&sealed_token=iryqtVEb4hcMJpt3uuX43JaGNlZRMV4-stDO4M8-7zH7g186IdrwSCU1PMWBv_YdK7DC4y6qw4sEKu9JXmHbR6c7clp1u8sL1AXI3D_dMW_KTnIjJYKDGw" />
   <img alt="Star History Chart" src="https://api.star-history.com/chart?repos=LargeModGames/spotatui&type=date&legend=top-left&sealed_token=iryqtVEb4hcMJpt3uuX43JaGNlZRMV4-stDO4M8-7zH7g186IdrwSCU1PMWBv_YdK7DC4y6qw4sEKu9JXmHbR6c7clp1u8sL1AXI3D_dMW_KTnIjJYKDGw" />
 </picture>
</a>


## 维护者

- **[LargeModGames](https://github.com/LargeModGames)**（X 上的 [@LargeModGames](https://x.com/LargeModGames)）
- [shilicioo](https://github.com/shilicioo)
- [2570165831](https://github.com/2570165831)
- [Oud](https://github.com/Oud-Idk)

最初由 [Alexander Keliris](https://github.com/Rigellute) 从 [spotify-tui](https://github.com/Rigellute/spotify-tui) fork 而来。spotify-tui 的贡献者以及 spotatui 使用的库见 [CREDITS.md](CREDITS.md)。

## 代码签名政策

免费代码签名由 [SignPath.io](https://about.signpath.io) 提供，证书来自 [SignPath Foundation](https://signpath.org)。

Windows 发布文件由 GitHub 托管 runner 上的[发布工作流](.github/workflows/cd.yml)从本仓库构建。每次签名请求都由人工批准。

- 提交者和审查者：[LargeModGames](https://github.com/LargeModGames)、[ThatModGames](https://github.com/ThatModGames)、[shilicioo](https://github.com/shilicioo)、[2570165831](https://github.com/2570165831)、[Oud](https://github.com/Oud-Idk)
- 批准者：[LargeModGames](https://github.com/LargeModGames)

隐私：spotatui 发送的数据以及如何关闭每一部分，请参见[隐私政策](https://spotatui.com/privacy)。

## spotatui 贡献者

每一位提供过帮助的人，无论是否写了代码。想加入这份名单，请参见[致谢](CONTRIBUTING.md#recognition)。

<!-- ALL-CONTRIBUTORS-LIST:START - Do not remove or modify this section -->
<!-- prettier-ignore-start -->
<!-- markdownlint-disable -->
<table>
  <tbody>
    <tr>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/LargeModGames"><img src="https://avatars.githubusercontent.com/u/84450916?v=4&size=100" width="100px" height="100px" alt="LargeModGames"/><br /><sub><b>LargeModGames</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=LargeModGames" title="Code">💻</a> <a href="https://github.com/LargeModGames/spotatui/commits?author=LargeModGames" title="Documentation">📖</a> <a href="#maintenance-LargeModGames" title="Maintenance">🚧</a> <a href="#ideas-LargeModGames" title="Ideas, Planning, & Feedback">🤔</a> <a href="#infra-LargeModGames" title="Infrastructure (Hosting, Build-Tools, etc)">🚇</a> <a href="https://github.com/LargeModGames/spotatui/commits?author=LargeModGames" title="Tests">⚠️</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/shilicioo"><img src="https://avatars.githubusercontent.com/u/56956072?v=4&size=100" width="100px" height="100px" alt="shilicioo"/><br /><sub><b>shilicioo</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=shilicioo" title="Code">💻</a> <a href="#maintenance-shilicioo" title="Maintenance">🚧</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/2570165831"><img src="https://avatars.githubusercontent.com/u/200382837?v=4&size=100" width="100px" height="100px" alt="2570165831"/><br /><sub><b>2570165831</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=2570165831" title="Code">💻</a> <a href="#maintenance-2570165831" title="Maintenance">🚧</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/Oud-Idk"><img src="https://avatars.githubusercontent.com/u/80905197?v=4&size=100" width="100px" height="100px" alt="Oud"/><br /><sub><b>Oud</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=Oud-Idk" title="Code">💻</a> <a href="#maintenance-Oud-Idk" title="Maintenance">🚧</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/MysteriousWolf"><img src="https://avatars.githubusercontent.com/u/5306409?v=4&size=100" width="100px" height="100px" alt="MysteriousWolf"/><br /><sub><b>MysteriousWolf</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=MysteriousWolf" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/rawcode1337"><img src="https://avatars.githubusercontent.com/u/80097670?v=4&size=100" width="100px" height="100px" alt="rawcode1337"/><br /><sub><b>rawcode1337</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=rawcode1337" title="Code">💻</a> <a href="https://github.com/LargeModGames/spotatui/issues?q=author%3Arawcode1337" title="Bug reports">🐛</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/copeison"><img src="https://avatars.githubusercontent.com/u/184175589?v=4&size=100" width="100px" height="100px" alt="copeison"/><br /><sub><b>copeison</b></sub></a><br /><a href="#platform-copeison" title="Packaging/porting to new platform">📦</a></td>
    </tr>
    <tr>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/jacklorusso"><img src="https://avatars.githubusercontent.com/u/19835679?v=4&size=100" width="100px" height="100px" alt="jacklorusso"/><br /><sub><b>jacklorusso</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=jacklorusso" title="Documentation">📖</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/H41L33"><img src="https://avatars.githubusercontent.com/u/140116782?v=4&size=100" width="100px" height="100px" alt="H41L33"/><br /><sub><b>H41L33</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=H41L33" title="Documentation">📖</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://mzte.de"><img src="https://avatars.githubusercontent.com/u/28735087?v=4&size=100" width="100px" height="100px" alt="LordMZTE"/><br /><sub><b>LordMZTE</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=LordMZTE" title="Code">💻</a> <a href="#platform-LordMZTE" title="Packaging/porting to new platform">📦</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/bodoque-01"><img src="https://avatars.githubusercontent.com/u/63447579?v=4&size=100" width="100px" height="100px" alt="Sebastian Sarco"/><br /><sub><b>Sebastian Sarco</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=bodoque-01" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/El-Mundos"><img src="https://avatars.githubusercontent.com/u/70759168?v=4&size=100" width="100px" height="100px" alt="Sergio Tabernero Hernández"/><br /><sub><b>Sergio Tabernero Hernández</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=El-Mundos" title="Code">💻</a> <a href="#platform-El-Mundos" title="Packaging/porting to new platform">📦</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://dpnova.github.io/"><img src="https://avatars.githubusercontent.com/u/229943?v=4&size=100" width="100px" height="100px" alt="David Novakovic"/><br /><sub><b>David Novakovic</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=dpnova" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="http://nthpaul.com"><img src="https://avatars.githubusercontent.com/u/70828466?v=4&size=100" width="100px" height="100px" alt="Paul"/><br /><sub><b>Paul</b></sub></a><br /><a href="#design-nthpaul" title="Design">🎨</a> <a href="https://github.com/LargeModGames/spotatui/commits?author=nthpaul" title="Code">💻</a></td>
    </tr>
    <tr>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/AFE123x"><img src="https://avatars.githubusercontent.com/u/121839885?v=4&size=100" width="100px" height="100px" alt="Arun Felix"/><br /><sub><b>Arun Felix</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=AFE123x" title="Code">💻</a> <a href="https://github.com/LargeModGames/spotatui/issues?q=author%3AAFE123x" title="Bug reports">🐛</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/MaySeikatsu"><img src="https://avatars.githubusercontent.com/u/127960577?v=4&size=100" width="100px" height="100px" alt="MaySeikatsu"/><br /><sub><b>MaySeikatsu</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=MaySeikatsu" title="Code">💻</a> <a href="https://github.com/LargeModGames/spotatui/commits?author=MaySeikatsu" title="Documentation">📖</a> <a href="#platform-MaySeikatsu" title="Packaging/porting to new platform">📦</a></td>
      <td align="center" valign="top" width="14.28%"><a href="http://prabo.org"><img src="https://avatars.githubusercontent.com/u/32436755?v=4&size=100" width="100px" height="100px" alt="Lorenzo Bodini"/><br /><sub><b>Lorenzo Bodini</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=topongo" title="Code">💻</a> <a href="#design-topongo" title="Design">🎨</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/Vi1i"><img src="https://avatars.githubusercontent.com/u/6485370?v=4&size=100" width="100px" height="100px" alt="Vi1i Petal"/><br /><sub><b>Vi1i Petal</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=Vi1i" title="Code">💻</a> <a href="https://github.com/LargeModGames/spotatui/issues?q=author%3AVi1i" title="Bug reports">🐛</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/wrsturgeon"><img src="https://avatars.githubusercontent.com/u/79714036?v=4&size=100" width="100px" height="100px" alt="Will Sturgeon"/><br /><sub><b>Will Sturgeon</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=wrsturgeon" title="Code">💻</a> <a href="https://github.com/LargeModGames/spotatui/issues?q=author%3Awrsturgeon" title="Bug reports">🐛</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/wfinken"><img src="https://avatars.githubusercontent.com/u/891256?v=4&size=100" width="100px" height="100px" alt="wfinken"/><br /><sub><b>wfinken</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=wfinken" title="Code">💻</a> <a href="https://github.com/LargeModGames/spotatui/issues?q=author%3Awfinken" title="Bug reports">🐛</a></td>
      <td align="center" valign="top" width="14.28%"><a href="http://kathund.dev"><img src="https://avatars.githubusercontent.com/u/55346310?v=4&size=100" width="100px" height="100px" alt="Jacob"/><br /><sub><b>Jacob</b></sub></a><br /><a href="#platform-Kathund" title="Packaging/porting to new platform">📦</a></td>
    </tr>
    <tr>
      <td align="center" valign="top" width="14.28%"><a href="http://dominicklee.net"><img src="https://avatars.githubusercontent.com/u/43938540?v=4&size=100" width="100px" height="100px" alt="Dominick Lee"/><br /><sub><b>Dominick Lee</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=domogami" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/higorprado"><img src="https://avatars.githubusercontent.com/u/1037397?v=4&size=100" width="100px" height="100px" alt="Higor Prado"/><br /><sub><b>Higor Prado</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=higorprado" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/eiseq"><img src="https://avatars.githubusercontent.com/u/80960013?v=4&size=100" width="100px" height="100px" alt="Vitali Kaplich"/><br /><sub><b>Vitali Kaplich</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=eiseq" title="Documentation">📖</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/1knth"><img src="https://avatars.githubusercontent.com/u/115324660?v=4&size=100" width="100px" height="100px" alt="knth"/><br /><sub><b>knth</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=1knth" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="http://www.dev.arynwood.com"><img src="https://avatars.githubusercontent.com/u/64027767?v=4&size=100" width="100px" height="100px" alt="Lorelei Noble"/><br /><sub><b>Lorelei Noble</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=SkyeVault" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://benalleng.com"><img src="https://avatars.githubusercontent.com/u/108441023?v=4&size=100" width="100px" height="100px" alt="Ben Allen"/><br /><sub><b>Ben Allen</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=benalleng" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/Moaht"><img src="https://avatars.githubusercontent.com/u/93605688?v=4&size=100" width="100px" height="100px" alt="Thomas Allan"/><br /><sub><b>Thomas Allan</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=Moaht" title="Code">💻</a></td>
    </tr>
    <tr>
      <td align="center" valign="top" width="14.28%"><a href="http://www.pratyoosh.me"><img src="https://avatars.githubusercontent.com/u/170789120?v=4&size=100" width="100px" height="100px" alt="Pratyoosh Prakash"/><br /><sub><b>Pratyoosh Prakash</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=rlpratyoosh" title="Code">💻</a> <a href="https://github.com/LargeModGames/spotatui/issues?q=author%3Arlpratyoosh" title="Bug reports">🐛</a></td>
      <td align="center" valign="top" width="14.28%"><a href="http://tahmid.io"><img src="https://avatars.githubusercontent.com/u/107484759?v=4&size=100" width="100px" height="100px" alt="Tahmid Ahmed"/><br /><sub><b>Tahmid Ahmed</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=tahminator" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/Ritze03"><img src="https://avatars.githubusercontent.com/u/63347117?v=4&size=100" width="100px" height="100px" alt="Ritze"/><br /><sub><b>Ritze</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=Ritze03" title="Code">💻</a> <a href="https://github.com/LargeModGames/spotatui/issues?q=author%3ARitze03" title="Bug reports">🐛</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/itlogsandwich"><img src="https://avatars.githubusercontent.com/u/155230216?v=4&size=100" width="100px" height="100px" alt="Hansen"/><br /><sub><b>Hansen</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=itlogsandwich" title="Documentation">📖</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/yhay81"><img src="https://avatars.githubusercontent.com/u/11132792?v=4&size=100" width="100px" height="100px" alt="Yusuke Hayashi"/><br /><sub><b>Yusuke Hayashi</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=yhay81" title="Code">💻</a> <a href="https://github.com/LargeModGames/spotatui/commits?author=yhay81" title="Documentation">📖</a></td>
      <td align="center" valign="top" width="14.28%"><a href="http://felixzieger.de"><img src="https://avatars.githubusercontent.com/u/67903933?v=4&size=100" width="100px" height="100px" alt="Felix"/><br /><sub><b>Felix</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=felixzieger" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/DinoLeung"><img src="https://avatars.githubusercontent.com/u/7704357?v=4&size=100" width="100px" height="100px" alt="Dino Leung"/><br /><sub><b>Dino Leung</b></sub></a><br /><a href="#platform-DinoLeung" title="Packaging/porting to new platform">📦</a></td>
    </tr>
    <tr>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/istipisti113"><img src="https://avatars.githubusercontent.com/u/42544437?v=4&size=100" width="100px" height="100px" alt="Szabó István"/><br /><sub><b>Szabó István</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=istipisti113" title="Documentation">📖</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://alessandrozanni.it/"><img src="https://avatars.githubusercontent.com/u/78210329?v=4&size=100" width="100px" height="100px" alt="Alessandro Zanni"/><br /><sub><b>Alessandro Zanni</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=alessandro-zanni" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/nohint404"><img src="https://avatars.githubusercontent.com/u/238106931?v=4&size=100" width="100px" height="100px" alt="nohint404"/><br /><sub><b>nohint404</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=nohint404" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/TongTong0828"><img src="https://avatars.githubusercontent.com/u/151875930?v=4&size=100" width="100px" height="100px" alt="TongTong0828"/><br /><sub><b>TongTong0828</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=TongTong0828" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/Hi-1mYara"><img src="https://avatars.githubusercontent.com/u/254750611?v=4&size=100" width="100px" height="100px" alt="Hi-1mYara"/><br /><sub><b>Hi-1mYara</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=Hi-1mYara" title="Code">💻</a> <a href="https://github.com/LargeModGames/spotatui/commits?author=Hi-1mYara" title="Documentation">📖</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://salty-nebula-7f4.notion.site/Yvoolab-3c6f362dc885816bae88f25d128e85cd"><img src="https://avatars.githubusercontent.com/u/202315522?v=4&size=100" width="100px" height="100px" alt="Yvoo"/><br /><sub><b>Yvoo</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=yvoolab" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/mschmicking"><img src="https://avatars.githubusercontent.com/u/17197791?v=4&size=100" width="100px" height="100px" alt="Maurice Schmicking"/><br /><sub><b>Maurice Schmicking</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=mschmicking" title="Code">💻</a></td>
    </tr>
    <tr>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/tayfuryldz"><img src="https://avatars.githubusercontent.com/u/238304586?v=4&size=100" width="100px" height="100px" alt="Tayfur Yıldız"/><br /><sub><b>Tayfur Yıldız</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=tayfuryldz" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/chiliec"><img src="https://avatars.githubusercontent.com/u/982358?v=4&size=100" width="100px" height="100px" alt="Vladimir Babin"/><br /><sub><b>Vladimir Babin</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=chiliec" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="http://wanjinhao1.github.io"><img src="https://avatars.githubusercontent.com/u/34562501?v=4&size=100" width="100px" height="100px" alt="Kratos"/><br /><sub><b>Kratos</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=wanjinhao1" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/niukanen1"><img src="https://avatars.githubusercontent.com/u/57656076?v=4&size=100" width="100px" height="100px" alt="niukanen1"/><br /><sub><b>niukanen1</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=niukanen1" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/TrevorSatori"><img src="https://avatars.githubusercontent.com/u/80570915?v=4&size=100" width="100px" height="100px" alt="Trevor Satori"/><br /><sub><b>Trevor Satori</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=TrevorSatori" title="Code">💻</a> <a href="https://github.com/LargeModGames/spotatui/issues?q=author%3ATrevorSatori" title="Bug reports">🐛</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/drakeo338"><img src="https://avatars.githubusercontent.com/u/328244157?v=4&size=100" width="100px" height="100px" alt="Y.B."/><br /><sub><b>Y.B.</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=drakeo338" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/uemrey0"><img src="https://avatars.githubusercontent.com/u/64688960?v=4&size=100" width="100px" height="100px" alt="Ufuk Emre Yücetürk"/><br /><sub><b>Ufuk Emre Yücetürk</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=uemrey0" title="Code">💻</a></td>
    </tr>
    <tr>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/WaterWhisperer"><img src="https://avatars.githubusercontent.com/u/164724580?v=4&size=100" width="100px" height="100px" alt="WaterWhisperer"/><br /><sub><b>WaterWhisperer</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=WaterWhisperer" title="Documentation">📖</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/Wassergeist"><img src="https://avatars.githubusercontent.com/u/91049818?v=4&size=100" width="100px" height="100px" alt="Marc Bauer"/><br /><sub><b>Marc Bauer</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=Wassergeist" title="Code">💻</a> <a href="#financial-Wassergeist" title="Financial">💵</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/RohithPariki"><img src="https://avatars.githubusercontent.com/u/159949499?v=4&size=100" width="100px" height="100px" alt="RohithPariki"/><br /><sub><b>RohithPariki</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=RohithPariki" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/Muu79"><img src="https://avatars.githubusercontent.com/u/35148226?v=4&size=100" width="100px" height="100px" alt="Muaaz Bhyat"/><br /><sub><b>Muaaz Bhyat</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=Muu79" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/jum-apzn"><img src="https://avatars.githubusercontent.com/u/335973771?v=4&size=100" width="100px" height="100px" alt="점[dot]"/><br /><sub><b>점[dot]</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=jum-apzn" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="http://web.samtalks.us"><img src="https://avatars.githubusercontent.com/u/15819181?v=4&size=100" width="100px" height="100px" alt="Samuel"/><br /><sub><b>Samuel</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=TheCodedKid" title="Documentation">📖</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/TastyHeadphones"><img src="https://avatars.githubusercontent.com/u/76083688?v=4&size=100" width="100px" height="100px" alt="TastyHeadphones"/><br /><sub><b>TastyHeadphones</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=TastyHeadphones" title="Documentation">📖</a></td>
    </tr>
    <tr>
      <td align="center" valign="top" width="14.28%"><a href="https://zovo.one"><img src="https://avatars.githubusercontent.com/u/51033404?v=4&size=100" width="100px" height="100px" alt="Michael "/><br /><sub><b>Michael </b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=theluckystrike" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/Battlelamb"><img src="https://avatars.githubusercontent.com/u/12995996?v=4&size=100" width="100px" height="100px" alt="Battlelamb"/><br /><sub><b>Battlelamb</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=Battlelamb" title="Documentation">📖</a> <a href="https://github.com/LargeModGames/spotatui/commits?author=Battlelamb" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="http://markus.gustavsson.co.nz"><img src="https://avatars.githubusercontent.com/u/103984290?v=4&size=100" width="100px" height="100px" alt="markus"/><br /><sub><b>markus</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=markusgust" title="Documentation">📖</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/dailyun"><img src="https://avatars.githubusercontent.com/u/144075925?v=4&size=100" width="100px" height="100px" alt="Drying"/><br /><sub><b>Drying</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=dailyun" title="Documentation">📖</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://jadhavgaurav.github.io"><img src="https://avatars.githubusercontent.com/u/134488140?v=4&size=100" width="100px" height="100px" alt="Gaurav Vijay Jadhav"/><br /><sub><b>Gaurav Vijay Jadhav</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=jadhavgaurav" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://anandb71.in"><img src="https://avatars.githubusercontent.com/u/169837340?v=4&size=100" width="100px" height="100px" alt="Anand B"/><br /><sub><b>Anand B</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=Anandb71" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://xdaforums.com/m/termuxsolutions.13482532/"><img src="https://avatars.githubusercontent.com/u/325067280?v=4&size=100" width="100px" height="100px" alt="AAA"/><br /><sub><b>AAA</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=kukirin-ui" title="Documentation">📖</a> <a href="https://github.com/LargeModGames/spotatui/commits?author=kukirin-ui" title="Code">💻</a></td>
    </tr>
    <tr>
      <td align="center" valign="top" width="14.28%"><a href="https://codedbearder.com"><img src="https://avatars.githubusercontent.com/u/1291396?v=4&size=100" width="100px" height="100px" alt="Arnar"/><br /><sub><b>Arnar</b></sub></a><br /><a href="#platform-arnarg" title="Packaging/porting to new platform">📦</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/OpenR00t"><img src="https://avatars.githubusercontent.com/u/17520570?v=4&size=100" width="100px" height="100px" alt="OpenR00t"/><br /><sub><b>OpenR00t</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=OpenR00t" title="Code">💻</a> <a href="#design-OpenR00t" title="Design">🎨</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/mvanhorn"><img src="https://avatars.githubusercontent.com/u/455140?v=4&size=100" width="100px" height="100px" alt="Matt Van Horn"/><br /><sub><b>Matt Van Horn</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=mvanhorn" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/devtechedge"><img src="https://avatars.githubusercontent.com/u/294291171?v=4&size=100" width="100px" height="100px" alt="Dev M"/><br /><sub><b>Dev M</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=devtechedge" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/montionugera"><img src="https://avatars.githubusercontent.com/u/5601426?v=4&size=100" width="100px" height="100px" alt="Pasit Nusso"/><br /><sub><b>Pasit Nusso</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=montionugera" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/psymariux"><img src="https://avatars.githubusercontent.com/u/238106931?v=4&size=100" width="100px" height="100px" alt="PsyMariux"/><br /><sub><b>PsyMariux</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=psymariux" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/uznog"><img src="https://avatars.githubusercontent.com/u/65494005?v=4&size=100" width="100px" height="100px" alt="Konrad Baran"/><br /><sub><b>Konrad Baran</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=uznog" title="Code">💻</a></td>
    </tr>
    <tr>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/zaydmulani09"><img src="https://avatars.githubusercontent.com/u/257340907?v=4&size=100" width="100px" height="100px" alt="zayd"/><br /><sub><b>zayd</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=zaydmulani09" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/BartholomewJr"><img src="https://avatars.githubusercontent.com/u/348017353?v=4&size=100" width="100px" height="100px" alt="BartholomewJr"/><br /><sub><b>BartholomewJr</b></sub></a><br /><a href="#financial-BartholomewJr" title="Financial">💵</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://my-portafolio-plum.vercel.app/"><img src="https://avatars.githubusercontent.com/u/83091452?v=4&size=100" width="100px" height="100px" alt="Yiuseppe24"/><br /><sub><b>Yiuseppe24</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=JosephAnderson234" title="Code">💻</a> <a href="#design-JosephAnderson234" title="Design">🎨</a></td>
      <td align="center" valign="top" width="14.28%"><a href="http://www.anuragx.in"><img src="https://avatars.githubusercontent.com/u/62888566?v=4&size=100" width="100px" height="100px" alt="anurag verma"/><br /><sub><b>anurag verma</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=deauthe" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/IDLinares"><img src="https://avatars.githubusercontent.com/u/32106028?v=4&size=100" width="100px" height="100px" alt="Isaiah Linares"/><br /><sub><b>Isaiah Linares</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=IDLinares" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/everyoneexe"><img src="https://avatars.githubusercontent.com/u/198324830?v=4&size=100" width="100px" height="100px" alt="everyoneexe"/><br /><sub><b>everyoneexe</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=everyoneexe" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/Radhik-Amp"><img src="https://avatars.githubusercontent.com/u/158242842?v=4&size=100" width="100px" height="100px" alt="Radhik"/><br /><sub><b>Radhik</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=Radhik-Amp" title="Code">💻</a></td>
    </tr>
    <tr>
      <td align="center" valign="top" width="14.28%"><a href="http://mattmarsico.com"><img src="https://avatars.githubusercontent.com/u/60232670?v=4&size=100" width="100px" height="100px" alt="Matt Marsico"/><br /><sub><b>Matt Marsico</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=mattmars3" title="Documentation">📖</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://www.abdulkarimmia.in/"><img src="https://avatars.githubusercontent.com/u/97598943?v=4&size=100" width="100px" height="100px" alt="Abdul karim mia"/><br /><sub><b>Abdul karim mia</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=abdul-karim-mia" title="Documentation">📖</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://tejeshvaish.com"><img src="https://avatars.githubusercontent.com/u/188414140?v=4&size=100" width="100px" height="100px" alt="Tejesh"/><br /><sub><b>Tejesh</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=tridev-tej" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/bfs2021"><img src="https://avatars.githubusercontent.com/u/101915083?v=4&size=100" width="100px" height="100px" alt="zgy"/><br /><sub><b>zgy</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=bfs2021" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="http://tree.jaopd.dev"><img src="https://avatars.githubusercontent.com/u/177440418?v=4&size=100" width="100px" height="100px" alt="João Pedro"/><br /><sub><b>João Pedro</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=jaopdc11" title="Code">💻</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/Vansh170707"><img src="https://avatars.githubusercontent.com/u/219549318?v=4&size=100" width="100px" height="100px" alt="Vansh"/><br /><sub><b>Vansh</b></sub></a><br /><a href="https://github.com/LargeModGames/spotatui/commits?author=Vansh170707" title="Code">💻</a></td>
    </tr>
  </tbody>
</table>

<!-- markdownlint-restore -->
<!-- prettier-ignore-end -->

<!-- ALL-CONTRIBUTORS-LIST:END -->

This project follows the [all-contributors](https://github.com/all-contributors/all-contributors) specification. Contributions of any kind welcome!
