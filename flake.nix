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
    inputs.flake-utils.lib.eachSystem
      [
        "x86_64-linux"
        "aarch64-linux"
      ]
      (
        system:
        let
          pkgs = import inputs.nixpkgs {
            inherit system;
            overlays = [
              inputs.rust-overlay.overlays.default
              inputs.ethereumNix.overlays.default
            ];
          };
        in
        {
          packages.default = pkgs.symlinkJoin {
            name = "combined-default";
            paths = with pkgs; [
              just
              cargo-nextest
              reth
              lighthouse
              foundry-bin
            ];
          };

          devShells.default = pkgs.mkShell {
            buildInputs = with pkgs; [
              just
              cargo-nextest
              reth
              lighthouse
              foundry-bin
            ];
          };

          formatter = pkgs.nixfmt-rfc-style;
        }
      );
}
