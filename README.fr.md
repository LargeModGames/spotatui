<h1 align="center">
  <img src=".github/spotatui-logo.png" alt="spotatui logo" width="200"><br>
  spotatui
</h1>
<p align="center">
  Un lecteur de musique rapide pour le terminal et le bureau, écrit en Rust.<br>
  Spotify via son propre client intégré, plus les fichiers locaux, Subsonic/Navidrome, la radio internet, YouTube et Qobuz, et Apple Music sur macOS.
</p>
<p align="center">
  <a href="https://spotatui.com/download"><strong>Télécharger</strong></a> · <a href="#installation">Installer</a> · <a href="docs/README.md">Docs</a> · <a href="CONTRIBUTING.md">Contribuer</a> · <a href="https://spotatui.com/discord">Discord</a>
</p>

[![Crates.io](https://img.shields.io/crates/v/spotatui.svg)](https://crates.io/crates/spotatui)
[![Discord](https://img.shields.io/discord/1534820237122207815?logo=discord&logoColor=white&label=Discord&color=5865F2)](https://spotatui.com/discord)
[![Songs played using spotatui](https://img.shields.io/badge/dynamic/json?url=https://spotatui-counter.spotatui.workers.dev&query=count&label=Songs%20played%20using%20spotatui&labelColor=0b0f14&color=1ed760&style=flat-square&cacheSeconds=600)](#historique-des-écoutes)
[![spotatui Contributors](https://img.shields.io/badge/dynamic/json?url=https://raw.githubusercontent.com/LargeModGames/spotatui/main/.all-contributorsrc&query=%24.contributors.length&label=spotatui%20contributors&color=1ed760&style=flat-square)](#contributeurs-de-spotatui)

**Application de bureau**

![L'application de bureau spotatui : une page d'album, puis le visualiseur, les paroles synchronisées et la pochette en panneaux](.github/demo-desktop.gif)

**Application terminal**

![spotatui dans le terminal : un album, la pochette, les paroles synchronisées et cinq styles de visualiseur](.github/demo.gif)

<!-- START doctoc generated TOC please keep comment here to allow auto update -->
<!-- DON'T EDIT THIS SECTION, INSTEAD RE-RUN doctoc TO UPDATE -->
## Table des matières

- [À propos](#à-propos)
- [Installation](#installation)
- [Démarrage rapide](#démarrage-rapide)
- [Sources](#sources)
- [En savoir plus](#en-savoir-plus)
- [Contribuer](#contribuer)
- [Historique des écoutes](#historique-des-écoutes)
- [Historique des étoiles](#historique-des-étoiles)
- [Mainteneurs](#mainteneurs)
- [Politique de signature du code](#politique-de-signature-du-code)
- [Contributeurs de spotatui](#contributeurs-de-spotatui)

<!-- END doctoc generated TOC please keep comment here to allow auto update -->

## À propos

spotatui est un lecteur de musique avec deux interfaces : une application de bureau et une application terminal. Elles partagent une seule connexion, une seule configuration et un seul historique d'écoute. Spotify est facultatif : c'est l'une des sept sources.

- Paroles synchronisées et visualiseur audio en temps réel.
- Une file de lecture qui mélange des pistes de différentes sources.
- [Synchronisation de playlists](docs/playlist-sync.md) : reflète une playlist sur Spotify, Qobuz, Subsonic ou YouTube.
- [Plugins Lua](PLUGINS.md) pour les commandes, les touches, les popups et les thèmes.
- Un [DJ IA](docs/ai-dj.md) intégré à l'application, ou votre agent de codage comme DJ via [MCP](docs/mcp-setup.md).
- Un historique d'écoute local avec des statistiques et un récapitulatif partageable.
- Une [CLI](docs/cli.md) pour la lecture, la recherche et les playlists.

**Spotify nécessite Premium.** spotatui se connecte avec son propre client intégré : pas besoin d'application développeur ni d'ID client.

spotatui est né d'un fork de [spotify-tui](https://github.com/Rigellute/spotify-tui) et a depuis développé son propre lecteur, ses sources et son application de bureau.

## Installation

Spotify est facultatif : les fichiers locaux, la radio internet et YouTube fonctionnent sans compte, tandis que Subsonic/Navidrome et Qobuz demandent un compte.

**Application de bureau**

| Système | Téléchargement |
|---|---|
| Windows 10/11 | [Programme d'installation](https://spotatui.com/download/windows) |
| macOS | [Apple Silicon](https://spotatui.com/download/macos) · [Intel](https://spotatui.com/download/macos-intel) |
| Linux | [.deb](https://spotatui.com/download/linux-deb) · [.rpm](https://spotatui.com/download/linux-rpm) · [AppImage](https://spotatui.com/download/linux-appimage) |

Les programmes d'installation ne sont pas encore signés : Windows et macOS affichent donc un avertissement au premier lancement. [Application de bureau](docs/installation.md#desktop-app) montre comment l'ouvrir et liste les fichiers arm64 pour Linux.

**Application terminal**

| Système | Commande |
|---|---|
| macOS, Linux, WSL | `curl -fsSL https://spotatui.com/install.sh \| bash` |
| Windows | `irm https://spotatui.com/install.ps1 \| iex` ou `winget install spotatui` |
| Arch Linux | `yay -S spotatui-bin` |
| Avec Rust | `cargo install --locked spotatui` |

Homebrew, Nix, Void, les paquets `.deb` et la compilation depuis les sources sont dans le [guide d'installation](docs/installation.md).

- **Fonctionnalités par build :** les binaires terminal précompilés incluent toutes les sources, le serveur MCP et le DJ IA. `cargo install` ne compile que Spotify, sauf si vous ajoutez [les fonctionnalités](docs/installation.md#which-build-has-which-features), et le flake Nix ne compile que Spotify.
- **Mises à jour :** l'application terminal se met à jour elle-même. Si un gestionnaire de paquets l'a installée, définissez `behavior.disable_auto_update: true`. Voir [Mise à jour](docs/installation.md#updating).

## Démarrage rapide

Lancez `spotatui`. Au premier lancement, il vous demande quelles sources vous voulez. Choisissez Spotify pour vous connecter : spotatui affiche un code et ouvre la page d'appairage de Spotify, où vous le confirmez.

Ensuite :

- `?` liste toutes les touches.
- `/` recherche. Choisissez le panneau **Songs** avec les flèches, appuyez sur `Entrée` pour y entrer, puis de nouveau `Entrée` pour jouer une piste.
- `d` change la source et l'appareil.
- `z` met une piste en file, et `Maj+Q` ouvre la file.
- `spotatui --help` affiche la [CLI](docs/cli.md).

## Sources

| Source | Il vous faut |
|---|---|
| **Spotify** | Un compte Premium |
| **Fichiers locaux** | Un dossier de musique |
| **Subsonic / Navidrome** | Un compte serveur |
| **Radio internet** | Rien (plus de 30 000 stations de [radio-browser.info](https://www.radio-browser.info)) |
| **YouTube** | [yt-dlp](https://github.com/yt-dlp/yt-dlp) dans votre `PATH` |
| **Qobuz** | Un compte Qobuz payant |
| **Apple Music** | macOS et l'app Musique |

Les fichiers locaux, Subsonic, la radio internet, YouTube et Qobuz passent par le moteur audio propre de spotatui. Apple Music joue dans l'app Musique, et spotatui est sa télécommande. La configuration de chaque source est dans [Configuration](docs/configuration.md).

Spotify joue dans spotatui lui-même, sur n'importe quel appareil Spotify Connect, ou sur des enceintes Cast et Connect de votre réseau. Voir [Où Spotify joue](docs/native-streaming.md#where-spotify-plays). Pour l'audio sans perte, ou un compte que le streaming natif ne peut pas jouer, spotatui peut lancer le propre lecteur de Spotify : voir [Soloist](docs/soloist.md).

## En savoir plus

| Vous voulez | Lire |
|---|---|
| Installer autrement, ou compiler depuis les sources | [Installation](docs/installation.md) |
| Changer les réglages, les touches ou le thème | [Configuration](docs/configuration.md), [Raccourcis clavier](docs/keybindings.md), [Thèmes](docs/themes.md) |
| Contrôler spotatui depuis votre shell | [CLI](docs/cli.md) |
| Écrire ou installer un plugin Lua | [PLUGINS.md](PLUGINS.md), [API de script](docs/scripting.md) |
| Laisser une IA choisir la musique | [DJ IA](docs/ai-dj.md), [Configuration MCP](docs/mcp-setup.md) |
| Signaler un bug | [Nouvelle issue](https://github.com/LargeModGames/spotatui/issues/new/choose), avec un [journal](docs/configuration.md#logging) `spotatui --debug` |
| Tout le reste | [Toute la doc](docs/README.md) |

## Contribuer

spotatui est construit par une petite équipe et un groupe grandissant de contributeurs, et vous n'avez pas besoin d'écrire du code pour aider. Commencez par [CONTRIBUTING.md](CONTRIBUTING.md), ou choisissez une [good first issue](https://github.com/LargeModGames/spotatui/issues?q=is%3Aissue%20state%3Aopen%20label%3A%22good%20first%20issue%22).

La lecture native et Spotify Connect reposent sur notre fork de librespot, [spotatui-librespot](https://github.com/LargeModGames/spotatui-librespot). Il fait partie du projet : une PR fusionnée là-bas compte comme une contribution à spotatui. Voir [Contribuer au fork librespot](CONTRIBUTING.md#contributing-to-the-librespot-fork).

## Historique des écoutes

![Historique des écoutes](https://spotatui-counter.spotatui.workers.dev/chart.svg)

## Historique des étoiles

<a href="https://www.star-history.com/?type=date&legend=top-left&repos=LargeModGames%2Fspotatui">
 <picture>
   <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/chart?repos=LargeModGames/spotatui&type=date&theme=dark&legend=top-left&sealed_token=iryqtVEb4hcMJpt3uuX43JaGNlZRMV4-stDO4M8-7zH7g186IdrwSCU1PMWBv_YdK7DC4y6qw4sEKu9JXmHbR6c7clp1u8sL1AXI3D_dMW_KTnIjJYKDGw" />
   <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/chart?repos=LargeModGames/spotatui&type=date&legend=top-left&sealed_token=iryqtVEb4hcMJpt3uuX43JaGNlZRMV4-stDO4M8-7zH7g186IdrwSCU1PMWBv_YdK7DC4y6qw4sEKu9JXmHbR6c7clp1u8sL1AXI3D_dMW_KTnIjJYKDGw" />
   <img alt="Star History Chart" src="https://api.star-history.com/chart?repos=LargeModGames/spotatui&type=date&legend=top-left&sealed_token=iryqtVEb4hcMJpt3uuX43JaGNlZRMV4-stDO4M8-7zH7g186IdrwSCU1PMWBv_YdK7DC4y6qw4sEKu9JXmHbR6c7clp1u8sL1AXI3D_dMW_KTnIjJYKDGw" />
 </picture>
</a>


## Mainteneurs

- **[LargeModGames](https://github.com/LargeModGames)** ([@LargeModGames](https://x.com/LargeModGames) sur X)
- [shilicioo](https://github.com/shilicioo)
- [2570165831](https://github.com/2570165831)
- [Oud](https://github.com/Oud-Idk)

Issu à l'origine d'un fork de [spotify-tui](https://github.com/Rigellute/spotify-tui) par [Alexander Keliris](https://github.com/Rigellute). Les contributeurs de spotify-tui et les bibliothèques utilisées par spotatui sont dans [CREDITS.md](CREDITS.md).

## Politique de signature du code

Signature de code gratuite fournie par [SignPath.io](https://about.signpath.io), certificat par [SignPath Foundation](https://signpath.org).

Les fichiers de version Windows sont construits depuis ce dépôt par le [workflow de version](.github/workflows/cd.yml) sur des runners hébergés par GitHub. Chaque demande de signature est approuvée à la main.

- Committeurs et relecteurs : [LargeModGames](https://github.com/LargeModGames), [ThatModGames](https://github.com/ThatModGames), [shilicioo](https://github.com/shilicioo), [2570165831](https://github.com/2570165831), [Oud](https://github.com/Oud-Idk)
- Approbateurs : [LargeModGames](https://github.com/LargeModGames)

Confidentialité : voir la [politique de confidentialité](https://spotatui.com/privacy) pour les données que spotatui envoie et comment désactiver chaque partie.

## Contributeurs de spotatui

Tous ceux qui ont aidé, avec ou sans code. Pour figurer sur cette liste, voir [Reconnaissance](CONTRIBUTING.md#recognition).

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
