{
  description = "Sidescope native UI and headless Wayland test environment";
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  outputs = { self, nixpkgs }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" ];
      forAllSystems = fn: nixpkgs.lib.genAttrs systems (system: fn nixpkgs.legacyPackages.${system});
    in {
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
