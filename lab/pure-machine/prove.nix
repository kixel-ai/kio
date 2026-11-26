# The shell `prove.sh` runs in: Charon and Aeneas from Aeneas's nightly release, which
# ships the two built together, and elan for the Lean toolchain `proofs/lean-toolchain`
# names, whose bundled `leantar` reads Mathlib's cache. Linux only, as Aeneas releases.
# Bump the tag, both hashes, and `rust-charon` with `proofs/lakefile.toml`'s Aeneas
# revision.
{ pkgs }:
let
  release = "nightly-2026.11.03-ab34bd2";
  tarball =
    {
      x86_64-linux = {
        arch = "x86_64";
        hash = "sha256-7Q4zbFBtrTfk4cU/bMTW1ZWdfjr6sv1Vrq0oB3VlH14=";
      };
      aarch64-linux = {
        arch = "aarch64";
        hash = "sha256-010M/V6/mqFYvTEvLPOoB2ppHyewhgX2Zk39twWtF14=";
      };
    }
    .${pkgs.stdenv.hostPlatform.system} or (throw "Aeneas releases only for Linux");

  # The nightly Charon's driver links against: its `rust-toolchain`.
  rust-charon = pkgs.rust-bin.nightly."2026-09-17".minimal.override {
    extensions = [
      "rustc-dev"
      "rust-src"
      "llvm-tools"
      "miri"
    ];
  };

  # The standard library with full MIR, which Charon reads, as Charon's own flake builds it.
  charon-sysroots = pkgs.runCommand "charon-sysroots" { nativeBuildInputs = [ rust-charon ]; } ''
    export HOME=$NIX_BUILD_TOP/home CARGO_HOME=$NIX_BUILD_TOP/cargo CARGO_NET_OFFLINE=true
    mkdir -p $HOME $CARGO_HOME
    cat > $CARGO_HOME/config.toml <<END
    [source.crates-io]
    replace-with = "vendored-sources"
    [source.vendored-sources]
    directory = "$(rustc --print sysroot)/lib/rustlib/src/rust/library/vendor"
    END
    sysroot=$(cargo miri setup --print-sysroot)
    mkdir -p $out
    cp -a $sysroot/. $out/
  '';

  aeneas = pkgs.stdenv.mkDerivation {
    pname = "aeneas";
    version = release;
    src = pkgs.fetchurl {
      url = "https://github.com/AeneasVerif/aeneas/releases/download/${release}/aeneas-linux-${tarball.arch}.tar.gz";
      inherit (tarball) hash;
    };
    sourceRoot = ".";
    unpackCmd = "tar xzf $curSrc aeneas charon charon-driver";
    nativeBuildInputs = [
      pkgs.autoPatchelfHook
      pkgs.makeWrapper
    ];
    buildInputs = [
      rust-charon
      pkgs.stdenv.cc.cc.lib
    ];
    installPhase = ''
      install -Dm755 -t $out/bin aeneas charon charon-driver
      wrapProgram $out/bin/charon \
        --set CHARON_TOOLCHAIN_IS_IN_PATH 1 \
        --set CHARON_MIRI_SYSROOTS ${charon-sysroots} \
        --prefix PATH : ${rust-charon}/bin
    '';
  };
in
pkgs.mkShell {
  packages = [
    aeneas
    pkgs.elan
    pkgs.git
    pkgs.curl
  ];
}
