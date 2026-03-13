{
  description = "A Spotify client for the terminal written in Rust, powered by Ratatui";
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };
  outputs = {
    self,
    nixpkgs,
    flake-utils,
  }:
    flake-utils.lib.eachDefaultSystem (
      system: let
        pkgs = import nixpkgs {
          inherit system;
        };
        cargoVersion = (builtins.fromTOML (builtins.readFile ./Cargo.toml)).package.version;
        commitHash = toString (self.shortRev or self.dirtyShortRev or self.lastModified or "dirty");
      in {
        # Build dependencies for rust
        packages = rec {
          default = pkgs.rustPlatform.buildRustPackage {
            pname = "spotatui";
            version = "${cargoVersion}-${commitHash}";
            src = self;

            cargoLock = {
              lockFile = ./Cargo.lock;
            };
            nativeBuildInputs = with pkgs; [
              pkg-config
              patchelf
              llvmPackages.clang
              llvmPackages.libclang
            ];
            buildInputs = with pkgs;
              [
                openssl
                alsa-lib
                dbus
                pipewire
              ]
              # Build inputs for nix-darwin
              ++ pkgs.lib.optionals pkgs.stdenv.isDarwin [
                pkgs.darwin.apple_sdk.frameworks.Security
                pkgs.darwin.apple_sdk.frameworks.SystemConfiguration
              ];
            meta = with pkgs.lib; {
              description = "A Spotify client for the terminal written in Rust, powered by Ratatui";
              homepage = "https://github.com/LargeModGames/spotatui";
              license = licenses.mit;
              mainProgram = "spotatui";
            };
          };
          # Alias to reference it with .spotatui instead of default
          spotatui = self.packages.${system}.default;

          # Execute with `nix run github:LargeModGames/spotatui`
          apps = {
            default = {
              type = "app";
              program = "${self.packages.${system}.default}/bin/spotatui";
            };
          };

          # Devtools for nix develop
          devShells.default = pkgs.mkShell {
            buildInputs = with pkgs; [
              rustc
              cargo
              rust-analyzer
              rustfmt
              clippy
            ];
          };
        };
      }
    );
}
