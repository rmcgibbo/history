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
        history = naersk-lib.buildPackage {
          root = ./.;
          buildInputs = with pkgs; [ git ];
        };
      in
      {
        defaultPackage = history;
        devShell = pkgs.mkShell rec {
          inputsFrom = [ history ];
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
