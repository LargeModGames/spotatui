#!/usr/bin/env bash
# spotatui installer for macOS, Linux, and WSL.
#
#   curl -fsSL https://spotatui.com/install.sh | bash
#
# Environment overrides:
#   SPOTATUI_VERSION      install a specific tag (e.g. v0.40.3); default: latest
#   SPOTATUI_INSTALL_DIR  where to put the binary; default: $HOME/.local/bin
set -euo pipefail

REPO="LargeModGames/spotatui"
BINARY="spotatui"
INSTALL_DIR="${SPOTATUI_INSTALL_DIR:-$HOME/.local/bin}"

# --- pretty output ---------------------------------------------------------
if [ -t 2 ] && [ -z "${NO_COLOR:-}" ]; then
  BOLD="$(printf '\033[1m')"; DIM="$(printf '\033[2m')"; RED="$(printf '\033[31m')"
  GREEN="$(printf '\033[32m')"; YELLOW="$(printf '\033[33m')"; RESET="$(printf '\033[0m')"
else
  BOLD=""; DIM=""; RED=""; GREEN=""; YELLOW=""; RESET=""
fi
info()  { printf '%s\n' "${DIM}·${RESET} $*" >&2; }
ok()    { printf '%s\n' "${GREEN}✓${RESET} $*" >&2; }
warn()  { printf '%s\n' "${YELLOW}!${RESET} $*" >&2; }
error() { printf '%s\n' "${RED}✗${RESET} $*" >&2; exit 1; }

# --- prerequisites ---------------------------------------------------------
if command -v curl >/dev/null 2>&1; then
  dl() { curl -fsSL "$1" -o "$2"; }
elif command -v wget >/dev/null 2>&1; then
  dl() { wget -qO "$2" "$1"; }
else
  error "need curl or wget to download spotatui"
fi

sha256_of() { # print the sha256 of file $1 using whatever tool is available
  if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1" | awk '{print $1}'
  elif command -v shasum   >/dev/null 2>&1; then shasum -a 256 "$1" | awk '{print $1}'
  elif command -v openssl  >/dev/null 2>&1; then openssl dgst -sha256 "$1" | awk '{print $NF}'
  else echo ""; fi
}

# --- detect platform -------------------------------------------------------
os="$(uname -s)"; arch="$(uname -m)"
case "$os" in
  Linux)  platform_os="linux" ;;
  Darwin) platform_os="macos" ;;
  *) error "unsupported OS '$os'. Build from source instead: ${BOLD}cargo install --locked spotatui${RESET}" ;;
esac
case "$arch" in
  x86_64|amd64)  platform_arch="x86_64" ;;
  aarch64|arm64) platform_arch="aarch64" ;;
  *) error "no prebuilt binary for '$arch'. Build from source instead: ${BOLD}cargo install --locked spotatui${RESET}" ;;
esac
asset="${BINARY}-${platform_os}-${platform_arch}.tar.gz"

# --- resolve version / URL -------------------------------------------------
if [ -n "${SPOTATUI_VERSION:-}" ]; then
  tag="${SPOTATUI_VERSION#v}"; tag="v${tag}"
  base="https://github.com/${REPO}/releases/download/${tag}"
  label="$tag"
else
  base="https://github.com/${REPO}/releases/latest/download"
  label="latest"
fi

info "installing ${BOLD}spotatui${RESET} (${label}) for ${platform_os}/${platform_arch}"

tmp="$(mktemp -d)"; trap 'rm -rf "$tmp"' EXIT
if ! dl "${base}/${asset}" "${tmp}/${asset}"; then
  error "could not download ${asset}. That build may not exist yet for this release; try ${BOLD}cargo install --locked spotatui${RESET}"
fi

# --- verify checksum (best effort) -----------------------------------------
if dl "${base}/${asset}.sha256" "${tmp}/${asset}.sha256" 2>/dev/null; then
  expected="$(awk '{print $1}' "${tmp}/${asset}.sha256")"
  actual="$(sha256_of "${tmp}/${asset}")"
  if [ -z "$actual" ]; then
    warn "no sha256 tool found; skipping checksum verification"
  elif [ "$expected" != "$actual" ]; then
    error "checksum mismatch (expected ${expected}, got ${actual}); aborting"
  else
    ok "checksum verified"
  fi
else
  warn "no published checksum; skipping verification"
fi

# --- extract & install -----------------------------------------------------
tar -xzf "${tmp}/${asset}" -C "$tmp"
[ -f "${tmp}/${BINARY}" ] || error "archive did not contain '${BINARY}'"

mkdir -p "$INSTALL_DIR"
install -m 0755 "${tmp}/${BINARY}" "${INSTALL_DIR}/${BINARY}" 2>/dev/null \
  || { cp "${tmp}/${BINARY}" "${INSTALL_DIR}/${BINARY}" && chmod 0755 "${INSTALL_DIR}/${BINARY}"; }
ok "installed to ${BOLD}${INSTALL_DIR}/${BINARY}${RESET}"

# --- PATH management -------------------------------------------------------
# Append the install dir to the right shell profile (idempotently) and set
# PATH_RC to the file we touched. Returns non-zero only if it couldn't write.
PATH_RC=""
add_to_path() {
  dir="$1"; disp="$1"
  case "$dir" in "$HOME"/*) disp="\$HOME/${dir#"$HOME"/}" ;; esac
  case "$(basename "${SHELL:-sh}")" in
    zsh)  PATH_RC="${ZDOTDIR:-$HOME}/.zshrc";      line="export PATH=\"$disp:\$PATH\"" ;;
    bash) PATH_RC="$HOME/.bashrc";                 line="export PATH=\"$disp:\$PATH\"" ;;
    fish) PATH_RC="$HOME/.config/fish/config.fish"; line="fish_add_path $dir" ;;
    *)    PATH_RC="$HOME/.profile";                line="export PATH=\"$disp:\$PATH\"" ;;
  esac
  [ -f "$PATH_RC" ] && grep -qF -- "$line" "$PATH_RC" 2>/dev/null && return 0  # already there
  mkdir -p "$(dirname "$PATH_RC")" 2>/dev/null || true
  { printf '\n# Added by spotatui installer\n'; printf '%s\n' "$line"; } >> "$PATH_RC" 2>/dev/null || return 1
  return 0
}

manual_path_hint() {
  warn "${INSTALL_DIR} is not on your PATH. Add it, e.g.:"
  printf '%s\n' "    export PATH=\"${INSTALL_DIR}:\$PATH\"" >&2
}

case ":${PATH}:" in
  *":${INSTALL_DIR}:"*)
    : # already on PATH, nothing to do
    ;;
  *)
    if [ -n "${SPOTATUI_NO_MODIFY_PATH:-}" ]; then
      manual_path_hint
    elif add_to_path "$INSTALL_DIR"; then
      ok "added ${INSTALL_DIR} to your PATH in ${BOLD}${PATH_RC}${RESET}"
      info "restart your terminal or run ${BOLD}source ${PATH_RC}${RESET} to use it now"
    else
      manual_path_hint
    fi
    ;;
esac

printf '\n%s\n' "${GREEN}Done.${RESET} Run ${BOLD}${BINARY}${RESET} to get started." >&2
