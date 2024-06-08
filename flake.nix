{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    crane = {
      url = "github:ipetkov/crane";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    pre-commit-hooks.url = "github:cachix/pre-commit-hooks.nix";
  };

  outputs = inputs @ {
    self,
    nixpkgs,
    flake-utils,
    fenix,
    crane,
    ...
  }:
    flake-utils.lib.eachDefaultSystem (
      system: let
        inherit name;
        pkgs = nixpkgs.legacyPackages."${system}";
        rust = fenix.packages.${system}.stable;
        craneLib = (crane.mkLib nixpkgs.legacyPackages."${system}").overrideToolchain rust.toolchain;
        buildInputs = with pkgs; [
          alsaLib
          udev
          xorg.libXcursor
          xorg.libXi
          xorg.libXrandr
          libxkbcommon
          vulkan-loader
          wayland
        ];
        nativeBuildInputs = with pkgs; [
          mold
          pkg-config
        ];
        wasmPkg = {profile ? "release"}: let
          target = "wasm32-unknown-unknown";
          toolchainWasm = with fenix.packages.${system};
            combine [
              stable.rustc
              stable.cargo
              targets.${target}.stable.rust-std
            ];
          craneWasm = (crane.mkLib nixpkgs.legacyPackages.${system}).overrideToolchain toolchainWasm;
        in
          craneWasm.buildPackage {
            src = craneLib.cleanCargoSource ./.;
            CARGO_BUILD_TARGET = target;
            CARGO_PROFILE = profile;
            inherit nativeBuildInputs;
            doCheck = false;
          };

        webPkg = {
          package,
          assets,
        }:
          pkgs.stdenv.mkDerivation {
            inherit name;
            src = ./wasm;
            nativeBuildInputs = [
              pkgs.wasm-bindgen-cli
              pkgs.binaryen
            ];
            phases = ["unpackPhase" "installPhase"];
            installPhase = ''
              mkdir -p $out
              wasm-bindgen --out-dir $out --out-name ${name} --target web ${package}/bin/${name}.wasm
              mv $out/${name}_bg.wasm .
              wasm-opt -Oz -o $out/${name}_bg.wasm ${name}_bg.wasm
              cp * $out/
              ln -s ${assets}/assets $out/assets
            '';
          };
      in {
        packages = {
          asteroids-bin = craneLib.buildPackage {
            name = "asteroids-bin";
            src = craneLib.cleanCargoSource ./.;
            inherit buildInputs;
            inherit nativeBuildInputs;
          };

          asteroids-assets = pkgs.stdenv.mkDerivation {
            name = "asteroids-assets";
            src = ./assets;
            phases = ["unpackPhase" "installPhase"];
            installPhase = ''
              mkdir -p $out
              cp -r $src $out/assets
            '';
          };

          asteroids = pkgs.stdenv.mkDerivation {
            name = "asteroids";
            phases = ["installPhase"];
            installPhase = ''
              mkdir -p $out
              ln -s ${self.packages.${system}.asteroids-assets}/assets $out/assets
              cp ${self.packages.${system}.asteroids-bin}/bin/asteroids $out/asteroids
            '';
          };

          asteroids-wasm =
            wasmPkg {
            };
          asteroids-web = webPkg {
            name = "asteroids";
            package = self.packages.${system}.asteroids-wasm;
            assets = self.packages.${system}.asteroids-assets;
          };
          asteroids-web-server = pkgs.writeShellScriptBin "asteroids-web-server" ''
            ${pkgs.simple-http-server}/bin/simple-http-server -i -c=html,wasm,ttf,js -- ${self.packages.${system}.asteroids-web}/
          '';

          asteroids-wasm-debug = wasmPkg {
            profile = "release-debug";
          };
          asteroids-web-debug = webPkg {
            package = self.packages.${system}.asteroids-wasm-debug;
            assets = self.packages.${system}.asteroids-assets;
          };
          asteroids-web-server-debug = pkgs.writeShellScriptBin "asteroids-web-server-debug" ''
            ${pkgs.simple-http-server}/bin/simple-http-server -i -c=html,wasm,ttf,js -- ${self.packages.${system}.asteroids-web-debug}/
          '';
        };

        defaultPackage = self.packages.${system}.asteroids;

        apps.asteroids = flake-utils.lib.mkApp {
          drv = self.packages.${system}.asteroids;
          exePath = "/asteroids";
        };

        apps.asteroids-web-server = flake-utils.lib.mkApp {
          drv = self.packages.${system}.asteroids-web-server;
          exePath = "/bin/asteroids-web-server";
        };

        defaultApp = self.apps.${system}.asteroids;

        checks = {
          pre-commit-check = inputs.pre-commit-hooks.lib.${system}.run {
            src = ./.;
            hooks = {
              alejandra.enable = true;
              statix.enable = true;
              rustfmt.enable = true;
              clippy = {
                enable = false;
                entry = let
                  rust-clippy = rust-clippy.withComponents ["clippy"];
                in
                  pkgs.lib.mkForce "${rust-clippy}/bin/cargo-clippy clippy";
              };
            };
          };
        };

        devShell = pkgs.mkShell {
          shellHook = ''
            export LD_LIBRARY_PATH="$LD_LIBRARY_PATH:${pkgs.lib.makeLibraryPath buildInputs}"
            ${self.checks.${system}.pre-commit-check.shellHook}
          '';
          inherit buildInputs;
          nativeBuildInputs =
            [
              (rust.withComponents ["cargo" "rustc" "rust-src" "rustfmt" "clippy"])
            ]
            ++ nativeBuildInputs;
        };
      }
    );
}
