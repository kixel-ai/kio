{
  description = "kio: poll-native async primitives, deterministic clock, and state machines";

  inputs = {
    # quest's nixpkgs, so every repository's dev shell gets the same C compiler and mbx
    # shares build scripts, proc-macros, and the crates built on them across repositories.
    nixpkgs.follows = "quest/nixpkgs";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    # The quest CLI, which also serves the quest guide and skills the stubs in
    # .claude/skills call. Bump the rev to upgrade them.
    quest = {
      url = "github:kixel-ai/quest";
      inputs.flake-utils.follows = "flake-utils";
    };
  };

  outputs =
    {
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
          devPackages = with pkgs; [
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
          verus = pkgs.callPackage ./nix/verus.nix { inherit (pkgs) rust-bin; };
        in
        {
          packages = pkgs.lib.optionalAttrs pkgs.stdenv.hostPlatform.isLinux { inherit verus; };
          devShells = {
            default = pkgs.mkShell { packages = devPackages; };
          }
          # The default shell plus Verus, for `just verify`. Separate so `just ci`'s
          # shell doesn't download Verus's toolchain; Linux only, as Verus's release is.
          // pkgs.lib.optionalAttrs pkgs.stdenv.hostPlatform.isLinux {
            verus = pkgs.mkShell { packages = devPackages ++ [ verus ]; };
            # lab/pure-machine's Charon, Aeneas, and Lean, Linux only as Aeneas's release is.
            prove = import ./lab/pure-machine/prove.nix { inherit pkgs; };
          };
          formatter = pkgs.nixfmt;
        }
      );
}
