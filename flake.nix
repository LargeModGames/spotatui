{
  description = "A Spotify client for the terminal written in Rust, powered by Ratatui";
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };
  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs {
          inherit system;
        };
        cargoVersion = (builtins.fromTOML (builtins.readFile ./Cargo.toml)).package.version;
        commitHash = toString (self.shortRev or self.dirtyShortRev or self.lastModified or "dirty");
        nativeBuildInputs = with pkgs; [
          pkg-config
          patchelf
          llvmPackages.clang
          llvmPackages.libclang
        ];
        buildInputs =
          with pkgs;
          [
            openssl
          ]
          ++ pkgs.lib.optionals pkgs.stdenv.isLinux [
            alsa-lib
            dbus
            pipewire
          ]
          # Build inputs for nix-darwin
          ++ pkgs.lib.optionals pkgs.stdenv.isDarwin [
            # macOS specific dependencies - use latest supported Apple SDK
            pkgs.apple-sdk
            pkgs.portaudio
          ];
        # Update alongside the [patch.crates-io] rev in Cargo.toml; the Nix build
        # fails with a hash mismatch that reports the correct value otherwise.
        librespotOutputHash = "sha256-N/ImWrtEyhKyjvZd8zVCelKtsAV1kHoFHMwCoe5ddI0=";
      in
      {
        # Build dependencies for rust
        packages = rec {
          default = pkgs.rustPlatform.buildRustPackage {
            pname = "spotatui";
            version = "${cargoVersion}-${commitHash}";
            src = self;

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
            inherit nativeBuildInputs buildInputs;
            meta = with pkgs.lib; {
              description = "A Spotify client for the terminal written in Rust, powered by Ratatui";
              homepage = "https://github.com/LargeModGames/spotatui";
              license = licenses.mit;
              mainProgram = "spotatui";
            };
          };
          # Alias to reference it with .spotatui instead of default
          spotatui = self.packages.${system}.default;
        };

        # Execute with `nix run github:LargeModGames/spotatui`
        apps = {
          default = {
            type = "app";
            program = "${self.packages.${system}.default}/bin/spotatui";
          };
        };

        # Devtools for nix develop
        devShells.default = pkgs.mkShell {
          nativeBuildInputs =
            nativeBuildInputs
            ++ (with pkgs; [
              rustc
              cargo
              rust-analyzer
              rustfmt
              clippy
            ]);
          inherit buildInputs;
          LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";
        };
      }
    );
}
