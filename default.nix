{ pkgs ? import <nixpkgs> {} }:
let
  librespotOutputHash = "sha256-N/ImWrtEyhKyjvZd8zVCelKtsAV1kHoFHMwCoe5ddI0=";
in
  pkgs.rustPlatform.buildRustPackage rec {
    pname = "spotatui";
    version = "0.34.3";

    src = pkgs.lib.cleanSource ./.;

    cargoLock = {
      lockFile = ./Cargo.lock;
      outputHashes = {
        "librespot-audio-0.8.0" = librespotOutputHash;
        "librespot-connect-0.8.0" = librespotOutputHash;
        "librespot-core-0.8.0" = librespotOutputHash;
        "librespot-metadata-0.8.0" = librespotOutputHash;
        "librespot-oauth-0.8.0" = librespotOutputHash;
        "librespot-playback-0.8.0" = librespotOutputHash;
        "librespot-protocol-0.8.0" = librespotOutputHash;
      };
    };

    nativeBuildInputs = with pkgs; [
      pkg-config
      patchelf
      llvmPackages.clang
      llvmPackages.libclang
    ];

    buildInputs = with pkgs; [
      openssl
      alsa-lib
      dbus
      pipewire
    ];

    postFixup = ''
  patchelf \
  --set-rpath "${pkgs.lib.makeLibraryPath [
  pkgs.openssl
  pkgs.alsa-lib
  pkgs.dbus
  pkgs.pipewire
  ]}" \
  $out/bin/spotatui
  '';

    meta = with pkgs.lib; {
      description = "Terminal UI Spotify client";
      homepage = "https://github.com/LargeModGames/spotatui";
      license = licenses.mit;
      mainProgram = "spotatui";
      platforms = platforms.linux;
    };
  }
