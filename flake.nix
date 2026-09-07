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
    let
      appModule = import ./nixos-module.nix { inherit self; };
    in
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = nixpkgs.legacyPackages.${system};

        buildToolchain = fenix.packages.${system}.stable.minimalToolchain;
        devToolchain = fenix.packages.${system}.stable.withComponents [
          "cargo"
          "clippy"
          "rust-analyzer"
          "rust-src"
          "rustc"
          "rustfmt"
        ];

        platform = pkgs.makeRustPlatform {
          cargo = buildToolchain;
          rustc = buildToolchain;
        };

        cargoToml = pkgs.lib.importTOML ./Cargo.toml;

        # Fenix's lld doesn't set RPATH; use wrapped lld for native deps.
        # This flag is also needed on macOS, but gated behind -Z unstable-options there.
        rustEnv = {
          RUSTFLAGS =
            pkgs.lib.optionalString pkgs.stdenv.isLinux "-Clink-self-contained=-linker "
            # Avoid runtime references from embedded toolchain source paths.
            + "--remap-path-prefix=${buildToolchain}=/rustc";
          OPENSSL_NO_VENDOR = "1";
        };
      in
      {
        checks = {
          default = self.packages.${system}.default;
        }
        // pkgs.lib.optionalAttrs pkgs.stdenv.isLinux {
          nixos-module-integration = pkgs.testers.runNixOSTest (
            import ./nixos-test.nix { inherit appModule; }
          );
        };

        packages.default = platform.buildRustPackage (
          rustEnv
          // rec {
            pname = cargoToml.package.name;
            version = cargoToml.package.version;
            description = cargoToml.package.description;
            nativeBuildInputs = with pkgs; [ llvmPackages.bintools ];

            # The system TLS verifier needs a trust store even for HTTP tests.
            SSL_CERT_FILE = "${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt";

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
            nativeBuildInputs = [ devToolchain ];
            buildInputs = [ pkgs.nixfmt ];
            RUST_LOG = "debug";
          }
        );
      }
    )
    // {
      nixosModules.default = appModule;

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

            apiKey = lib.mkOption {
              type = lib.types.nullOr lib.types.str;
              default = null;
              description = ''
                Kagi API key to write to the CLI configuration file.

                This value is stored in the Nix store. Prefer
                `programs.kagi.apiKeyFile` for secrets managed outside Nix.
              '';
            };

            apiKeyFile = lib.mkOption {
              type = lib.types.nullOr lib.types.str;
              default = null;
              description = ''
                Path to a file containing the Kagi API key. The path is linked
                without copying the secret into the Nix store.
              '';
            };
          };

          config = lib.mkIf cfg.enable (
            lib.mkMerge [
              {
                assertions = [
                  {
                    assertion = cfg.apiKey == null || cfg.apiKeyFile == null;
                    message = "programs.kagi.apiKey and programs.kagi.apiKeyFile are mutually exclusive.";
                  }
                ];

                home.packages = [ cfg.package ];

                home.file.".pi/agent/extensions/kagi-cli-prompt.ts" = lib.mkIf cfg.enablePiExtension {
                  source = self.piExtensions.default;
                };
              }
              (lib.mkIf (cfg.apiKey != null) {
                home.file.".config/kagi/api-key".text = "${cfg.apiKey}\n";
              })
              (lib.mkIf (cfg.apiKeyFile != null) {
                home.file.".config/kagi/api-key".source = config.lib.file.mkOutOfStoreSymlink cfg.apiKeyFile;
              })
            ]
          );
        };
    };
}
