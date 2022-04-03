{
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  inputs.utils.url = "github:numtide/flake-utils";
  inputs.naersk = {
    url = "github:nmattia/naersk";
    inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs = { self, nixpkgs, utils, naersk }:
    utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };
        naersk-lib = pkgs.callPackage naersk { };
        histdb = naersk-lib.buildPackage {
          root = ./.;
          buildInputs = with pkgs; [ sqlite ];
        };
      in
      {
        defaultPackage = histdb;
        devShell = pkgs.mkShell rec {
          inputsFrom = [ histdb ];
          buildInputs = with pkgs; [
            cargo-udeps  # RUSTC_BOOTSTRAP=1 cargo udeps
            sqlite-interactive
            rustc
            clippy
            rustfmt
            rust-analyzer
          ];
        };
      });
}
