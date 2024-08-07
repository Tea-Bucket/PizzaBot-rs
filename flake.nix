{
  description = "A Rust development environment with Trunk";

  inputs = {
    nixpkgs.url = "nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };
      in
      {
        devShell = with pkgs;
        mkShell {
          buildInputs = [
            pkg-config
            llvmPackages.bintools
            rustc
            cargo
            trunk
          ];

          shellHook = ''
            export CARGO_HOME=$(pwd)/.cargo
            export RUSTUP_HOME=$(pwd)/.rustup
          '';
        };
      });
}
