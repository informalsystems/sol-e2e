{
  description = "Nix setup for Solidity contract development";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs?ref=nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay.url = "github:oxalica/rust-overlay";
    ethereumNix.url = "github:nix-community/ethereum.nix";
  };

  outputs =
    inputs:
    inputs.flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import inputs.nixpkgs {
          inherit system;
          overlays = [ inputs.rust-overlay.overlays.default ];
        };
        ethPkgs = inputs.ethereumNix.packages.${system};
      in
      {
        packages = {
          just = pkgs.just;
          cargo-nextest = pkgs.cargo-nextest;
          reth = ethPkgs.reth;
          foundry-bin = ethPkgs.foundry-bin;
        };

        devShell = pkgs.mkShell {
          buildInputs = [
            pkgs.just
            pkgs.cargo-nextest
            ethPkgs.reth
            ethPkgs.foundry-bin
          ];
        };

        formatter = pkgs.nixfmt-rfc-style;
      }
    );
}
