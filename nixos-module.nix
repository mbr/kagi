{ self }:
{
  config,
  lib,
  pkgs,
  utils,
  ...
}:
let
  cfg = config.services.kagi;
  tcpPortMatch = builtins.match "^.*:([0-9]+)$" cfg.listenAddress;
  tcpPort = if tcpPortMatch == null then null else lib.toInt (lib.head tcpPortMatch);
in
{
  options.services.kagi = {
    enable = lib.mkEnableOption "Kagi's Perplexity-compatible search server";

    package = lib.mkOption {
      type = lib.types.package;
      default = self.packages.${pkgs.stdenv.hostPlatform.system}.default;
      defaultText = lib.literalExpression "inputs.kagi.packages.\${pkgs.stdenv.hostPlatform.system}.default";
      description = "Kagi package to run.";
    };

    apiKeyFile = lib.mkOption {
      type = lib.types.str;
      example = "/run/secrets/kagi-api-key";
      description = ''
        Absolute path to a file containing the upstream Kagi API key. Loaded
        through systemd credentials, without copying the secret into the Nix
        store. Compatible with secret files managed by agenix or sops-nix.
      '';
    };

    listenAddress = lib.mkOption {
      type = lib.types.str;
      default = "127.0.0.1:3000";
      example = "[::1]:3000";
      description = ''
        TCP socket address to listen on. The server has no inbound authentication;
        only expose it to trusted clients or an authenticated reverse proxy.
      '';
    };

    baseUrl = lib.mkOption {
      type = lib.types.str;
      default = "https://kagi.com/api/v1";
      description = "Upstream Kagi API base URL.";
    };

    logFilter = lib.mkOption {
      type = lib.types.str;
      default = "info";
      example = "info,kagi=debug";
      description = "Tracing filter for server logs.";
    };

    openFirewall = lib.mkOption {
      type = lib.types.bool;
      default = false;
      description = "Whether to open the unauthenticated search port in the firewall.";
    };

    shutdownTimeout = lib.mkOption {
      type = lib.types.ints.positive;
      default = 65;
      description = "Seconds allowed for active searches to drain during shutdown.";
    };
  };

  config = lib.mkIf cfg.enable {
    assertions = [
      {
        assertion = lib.hasPrefix "/" cfg.apiKeyFile;
        message = "services.kagi.apiKeyFile must be an absolute runtime path.";
      }
      {
        assertion = tcpPort != null && tcpPort >= 1 && tcpPort <= 65535;
        message = "services.kagi.listenAddress must be a TCP socket address with a port between 1 and 65535.";
      }
    ];

    networking.firewall.allowedTCPPorts = lib.mkIf (cfg.openFirewall && tcpPort != null) [
      tcpPort
    ];

    systemd.services.kagi = {
      description = "Kagi Perplexity-compatible search server";
      wantedBy = [ "multi-user.target" ];
      wants = [ "network-online.target" ];
      after = [ "network-online.target" ];
      startLimitIntervalSec = 0;
      environment = {
        KAGI_BASE_URL = cfg.baseUrl;
        KAGI_LISTEN_ADDRESS = cfg.listenAddress;
        RUST_LOG = cfg.logFilter;
      };
      serviceConfig = {
        Type = "exec";
        ExecStart = utils.escapeSystemdExecArgs [
          (lib.getExe cfg.package)
          "serve"
        ];
        Environment = [ "KAGI_API_KEY_FILE=%d/api-key" ];
        LoadCredential = [ "api-key:${cfg.apiKeyFile}" ];
        DynamicUser = true;
        User = "kagi";
        Restart = "on-failure";
        RestartSec = "100ms";
        RestartSteps = 10;
        RestartMaxDelaySec = "2min";
        TimeoutStopSec = cfg.shutdownTimeout;

        CapabilityBoundingSet = "";
        LockPersonality = true;
        MemoryDenyWriteExecute = true;
        NoNewPrivileges = true;
        PrivateDevices = true;
        PrivateTmp = true;
        ProtectClock = true;
        ProtectControlGroups = true;
        ProtectHome = true;
        ProtectHostname = true;
        ProtectKernelLogs = true;
        ProtectKernelModules = true;
        ProtectKernelTunables = true;
        ProtectSystem = "strict";
        RestrictAddressFamilies = [
          "AF_INET"
          "AF_INET6"
          "AF_UNIX"
        ];
        RestrictNamespaces = true;
        RestrictRealtime = true;
        RestrictSUIDSGID = true;
        SystemCallArchitectures = "native";
        SystemCallFilter = [ "@system-service" ];
        UMask = "0077";
      };
    };
  };
}
