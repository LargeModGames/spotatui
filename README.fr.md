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

<p align="center">
  <sub><a href="README.md">English</a> | <a href="README.zh-Hans.md">简体中文</a> | <a href="README.zh-Hant.md">繁體中文</a> | <b>Français</b> | <a href="README.de.md">Deutsch</a></sub>
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

La liste complète des contributeurs se trouve dans le [README anglais](README.md#spotatui-contributors). Ce projet suit la spécification [all-contributors](https://github.com/all-contributors/all-contributors).
