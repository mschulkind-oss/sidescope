{
  description = "Sidescope native UI and headless Wayland test environment";
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  outputs = { self, nixpkgs }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" ];
      forAllSystems = fn: nixpkgs.lib.genAttrs systems (system: fn nixpkgs.legacyPackages.${system});
    in {
      packages = forAllSystems (pkgs: {
        default = pkgs.rustPlatform.buildRustPackage {
          pname = "sidescope";
          version = "0.1.1";
          src = ./.;
          cargoLock.lockFile = ./Cargo.lock;
          nativeBuildInputs = with pkgs; [ pkg-config wrapGAppsHook4 ];
          buildInputs = with pkgs; [ gtk4 libadwaita ];
          nativeCheckInputs = [ pkgs.curl ];
          gappsWrapperArgs = [ "--prefix" "PATH" ":" (pkgs.lib.makeBinPath [ pkgs.curl ]) ];
          env.SIDESCOPE_REVISION = self.shortRev or self.dirtyShortRev or "nix";
          meta = with pkgs.lib; {
            description = "Native Wayland status column for AI usage, system telemetry, and weather";
            homepage = "https://github.com/mschulkind-oss/sidescope";
            license = licenses.asl20;
            mainProgram = "sidescope";
            platforms = platforms.linux;
          };
        };
      });

      apps = forAllSystems (pkgs: {
        default = {
          type = "app";
          program = "${self.packages.${pkgs.stdenv.hostPlatform.system}.default}/bin/sidescope";
        };
      });

      devShells = forAllSystems (pkgs: {
        default = pkgs.mkShell {
          nativeBuildInputs = with pkgs; [ pkg-config rustc cargo clippy rustfmt clang ];
          buildInputs = with pkgs; [ gtk4 libadwaita ];
          packages = with pkgs; [ sway-unwrapped grim wtype dbus file jq just ];
          shellHook = ''
            export GDK_BACKEND=wayland
            # Isolate Nix-built artifacts from a host/mise Rust toolchain.
            export CARGO_TARGET_DIR="$PWD/target/nix"
          '';
        };
      });
    };
}
