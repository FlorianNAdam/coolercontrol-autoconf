{
  inputs = {
    flake-utils.url = "github:numtide/flake-utils";
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    naersk.url = "github:nix-community/naersk";
  };

  outputs =
    {
      self,
      flake-utils,
      nixpkgs,
      naersk,
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = (import nixpkgs) {
          inherit system;
        };

        naersk-lib = pkgs.callPackage naersk { };

        coolercontrol-autoconf = naersk-lib.buildPackage {
          pname = "coolercontrol-autoconf";
          src = ./.;
        };
      in
      {
        packages = {
          inherit coolercontrol-autoconf;
          default = coolercontrol-autoconf;
        };

        devShell = pkgs.mkShell {
          buildInputs = with pkgs; [
            cargo
            rustc
            openssl
          ];

          nativeBuildInputs = with pkgs; [
            pkg-config
          ];

          packages = with pkgs; [
            rust-analyzer
          ];
        };
      }
    )
    // {
      nixosModules.coolercontrol-autoconf =
        {
          lib,
          pkgs,
          ...
        }:
        {
          imports = [ ./nix/module.nix ];
          services.coolercontrol-autoconf.package =
            lib.mkDefault
              self.packages.${pkgs.stdenv.hostPlatform.system}.default;
        };

      nixosModules.stylix =
        { ... }:
        {
          imports = [ ./nix/stylix.nix ];
        };

      nixosModules.default = self.nixosModules.coolercontrol-autoconf;
    };
}
