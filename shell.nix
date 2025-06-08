{ pkgs ? import <nixpkgs> {} }:

pkgs.mkShell {
  buildInputs = with pkgs; [
    # Rust toolchain
    rustc
    cargo
    rustfmt
    clippy

    # Build dependencies for v8 and openssl
    pkg-config
    openssl
    openssl.dev
    
    # Additional dependencies that might be needed for v8 compilation
    python3
    clang
    llvm
    gn
    ninja
    
    # Common build tools
    gcc
    binutils
    gnumake
    
    # For potential linking issues
    zlib
    zlib.dev
  ];

  # Environment variables needed for linking
  shellHook = ''
    export PKG_CONFIG_PATH="${pkgs.openssl.dev}/lib/pkgconfig:$PKG_CONFIG_PATH"
    export OPENSSL_DIR="${pkgs.openssl.dev}"
    export OPENSSL_LIB_DIR="${pkgs.openssl.out}/lib"
    export OPENSSL_INCLUDE_DIR="${pkgs.openssl.dev}/include"
  '';
} 