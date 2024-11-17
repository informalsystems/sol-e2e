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
          packages = with pkgs; {
            inherit
              just
              cargo-nextest
              reth
              foundry-bin
              ;
            default = pkgs.symlinkJoin {
              name = "combined-default";
              paths = [
                just
                cargo-nextest
                reth
                foundry-bin
              ];
            };
          };

          devShells.default = pkgs.mkShell {
            buildInputs = with pkgs; [
              just
              cargo-nextest
              reth
              foundry-bin
            ];
          };

          formatter = pkgs.nixfmt-rfc-style;
        }
      );
}
