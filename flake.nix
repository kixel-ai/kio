{
  description = "kio: poll-native async primitives, deterministic clock, and state machines";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    # The quest CLI, which also serves the quest guide and skills the stubs in
    # .claude/skills call. Bump the rev to upgrade them.
    quest = {
      url = "github:kixel-ai/quest";
      inputs.nixpkgs.follows = "nixpkgs";
      inputs.flake-utils.follows = "flake-utils";
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
      rust-overlay,
      quest,
      ...
    }:
    flake-utils.lib.eachSystem
      [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ]
      (
        system:
        let
          pkgs = import nixpkgs {
            inherit system;
            overlays = [ (import rust-overlay) ];
          };
          toolchain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
          quest-cli = quest.packages.${system}.default;
        in
        {
          packages.quest = quest-cli;
          # lab/pure-machine's Charon, Aeneas, and Lean: `nix develop .#prove`, on Linux only,
          # where Aeneas publishes a release.
          devShells =
            pkgs.lib.optionalAttrs pkgs.stdenv.hostPlatform.isLinux {
              prove = import ./lab/pure-machine/prove.nix { inherit pkgs; };
            }
            // {
              default = pkgs.mkShell {
                packages = with pkgs; [
                  toolchain
                  quest-cli
                  just
                  jq
                  git
                  gh
                  actionlint
                  cargo-shear
                  taplo
                  nixfmt
                ];
              };
            };
          formatter = pkgs.nixfmt;
          checks.quest-tree = pkgs.runCommand "quest-tree" { nativeBuildInputs = [ quest-cli ]; } ''
            quest --root ${self} check
            touch "$out"
          '';
        }
      );
}
