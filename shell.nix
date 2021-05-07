let
  sources = import ./nix/sources.nix;
  rust = import ./nix/rust.nix { inherit sources; };
  pkgs = import sources.nixpkgs { };
in
  pkgs.mkShell {

  shellHook = ''
    alias cb="cargo build"
    alias cbuild="cargo build"
     '';

  buildInputs = [
    rust
    pkgs.libudev
    pkgs.hidapi
    pkgs.pkg-config
    pkgs.openssl
  ];
}
