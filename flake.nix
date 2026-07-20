{
  description = "Development environment for Rust project with v8";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ (import rust-overlay) ];
        };

        # The toolchain is read straight out of ./rust-toolchain so that nix,
        # rustup and CI all resolve to the same rustc. That file also requests
        # the x86_64-unknown-linux-musl target, for which nixpkgs' own `rustc`
        # ships no rust-std -- without it a musl build fails with
        # "can't find crate for `core`".
        # rust-src is added here rather than in ./rust-toolchain so that CI and
        # rustup users are not made to download it; it only feeds
        # RUST_SRC_PATH for rust-analyzer.
        rustToolchain = (pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain).override {
          extensions = [ "rust-src" ];
        };

        # `ring` (pulled in by rustls) builds C and assembly, so the static
        # musl build needs a C cross toolchain. This provides
        # x86_64-unknown-linux-musl-{gcc,ar,...}, which .cargo/config.toml
        # refers to by bare name.
        muslCC = pkgs.pkgsCross.musl64.stdenv.cc;
      in
      {
        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            # Rust toolchain, pinned by ./rust-toolchain and musl-capable.
            rustToolchain
            rust-analyzer

            # Generic build tooling. TLS is handled by rustls (pure Rust), so
            # no system OpenSSL is required.
            pkg-config
          ] ++ lib.optionals stdenv.hostPlatform.isLinux [
            muslCC
          ];

          shellHook = ''
            export RUST_SRC_PATH="${rustToolchain}/lib/rustlib/src/rust/library";
          '';
        };
      });
}
