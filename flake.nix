{
  inputs = {
    nixpkgs.url = "nixpkgs/nixos-26.05";
    fenix = {
      url = "fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    flake-utils.url = "flake-utils";
  };

  outputs =
    {
      self,
      nixpkgs,
      fenix,
      flake-utils,
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = nixpkgs.legacyPackages.${system};

        toolchain = fenix.packages.${system}.stable.withComponents [
          "cargo"
          "clippy"
          "rust-analyzer"
          "rust-src"
          "rustc"
          "rustfmt"
        ];

        platform = pkgs.makeRustPlatform {
          cargo = toolchain;
          rustc = toolchain;
        };

        cargoToml = pkgs.lib.importTOML ./Cargo.toml;

        # Fenix's lld doesn't set RPATH; use wrapped lld for native deps.
        # This flag is also needed on macOS, but gated behind -Z unstable-options there.
        rustEnv = {
          RUSTFLAGS = pkgs.lib.optionalString pkgs.stdenv.isLinux "-Clink-self-contained=-linker";
          OPENSSL_NO_VENDOR = "1";
        };
      in
      {
        packages.default = platform.buildRustPackage (
          rustEnv
          // rec {
            pname = cargoToml.package.name;
            version = cargoToml.package.version;
            description = cargoToml.package.description;
            nativeBuildInputs = with pkgs; [ llvmPackages.bintools ];

            src = pkgs.lib.cleanSource ./.;

            cargoLock = {
              lockFile = ./Cargo.lock;
            };

            meta.mainProgram = pname;
          }
        );

        devShells.default = pkgs.mkShell (
          rustEnv
          // {
            inputsFrom = [ self.packages.${system}.default ];
            buildInputs = [ pkgs.nixfmt ];
            RUST_LOG = "debug";
          }
        );

        packages.docker = pkgs.dockerTools.buildImage {
          name = cargoToml.package.name;
          tag = cargoToml.package.version;

          config = {
            Cmd = [ (pkgs.lib.getExe self.packages.${system}.default) ];
          };
        };
      }
    )
    // {
      piExtensions.default = "${self.outPath}/extensions/kagi-cli-prompt.ts";

      homeManagerModules.default =
        {
          config,
          lib,
          pkgs,
          ...
        }:
        let
          cfg = config.programs.kagi;
          system = pkgs.stdenv.hostPlatform.system;
        in
        {
          options.programs.kagi = {
            enable = lib.mkEnableOption "Kagi command-line client";

            package = lib.mkOption {
              type = lib.types.package;
              default = self.packages.${system}.default;
              defaultText = lib.literalExpression "inputs.kagi.packages.\${pkgs.stdenv.hostPlatform.system}.default";
              description = "Kagi CLI package to install.";
            };

            enablePiExtension = lib.mkEnableOption "the Kagi Pi prompt extension";
          };

          config = lib.mkIf cfg.enable {
            home.packages = [ cfg.package ];

            home.file.".pi/agent/extensions/kagi-cli-prompt.ts" = lib.mkIf cfg.enablePiExtension {
              source = self.piExtensions.default;
            };
          };
        };
    };
}
