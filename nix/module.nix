{
  config,
  lib,
  pkgs,
  ...
}:

let
  cfg = config.services.coolercontrol-autoconf;
  settings = cfg.settings;
  settingsFile = pkgs.writeText "coolercontrol-autoconf.json" (
    builtins.toJSON (
      {
        inherit (settings) theme;
      }
      // lib.optionalAttrs (settings.eyeCandy != null) { inherit (settings) eyeCandy; }
      // lib.optionalAttrs (settings.showOnboarding != null) { inherit (settings) showOnboarding; }
      // lib.optionalAttrs (settings.collapsedMainMenu != null) { inherit (settings) collapsedMainMenu; }
      // lib.optionalAttrs (settings.hideMenuCollapseIcon != null) {
        inherit (settings) hideMenuCollapseIcon;
      }
      // lib.optionalAttrs (settings.mainMenuWidthRem != null) { inherit (settings) mainMenuWidthRem; }
      // lib.optionalAttrs (settings.frequencyPrecision != null) {
        inherit (settings) frequencyPrecision;
      }
      // lib.optionalAttrs (settings.chartLineScale != null) { inherit (settings) chartLineScale; }
      // lib.optionalAttrs (settings.time24 != null) { inherit (settings) time24; }
    )
  );
  passwordCredential = "coolercontrol-password";
in
{
  options.services.coolercontrol-autoconf = {
    enable = lib.mkEnableOption "declarative CoolerControl UI settings";

    package = lib.mkOption {
      type = lib.types.package;
      description = "Package providing the coolercontrol-autoconf executable.";
    };

    coolercontroldPackage = lib.mkOption {
      type = lib.types.package;
      default = pkgs.coolercontrol.coolercontrold;
      defaultText = lib.literalExpression "pkgs.coolercontrol.coolercontrold";
      description = "Package providing the coolercontrold executable.";
    };

    url = lib.mkOption {
      type = lib.types.str;
      default = "http://localhost:11987";
      description = "CoolerControl daemon API base URL.";
    };

    coolercontrold = lib.mkOption {
      type = lib.types.str;
      default = lib.getExe cfg.coolercontroldPackage;
      defaultText = lib.literalExpression ''
        lib.getExe config.services.coolercontrol-autoconf.coolercontroldPackage
      '';
      description = "Path to the coolercontrold executable used by setPassword.";
    };

    passwordFile = lib.mkOption {
      type = lib.types.path;
      description = ''
        File containing the CoolerControl admin password. It is loaded through
        systemd credentials and is not passed directly on the command line.
      '';
    };

    setPassword = lib.mkOption {
      type = lib.types.bool;
      default = false;
      description = ''
        Set the CoolerControl admin password from passwordFile before applying settings.
        If that password does not already work, coolercontrol-autoconf runs
        `coolercontrold --reset-password` and then changes the default password to
        passwordFile's value.
      '';
    };

    wait = lib.mkOption {
      type = lib.types.bool;
      default = true;
      description = ''
        Wait for the CoolerControl daemon API to respond before applying settings.
      '';
    };

    waitInterval = lib.mkOption {
      type = lib.types.str;
      default = "1s";
      description = ''
        Interval between daemon readiness checks, such as "500ms", "1s", or "5s".
      '';
    };

    settings = {
      theme = lib.mkOption {
        type = lib.types.oneOf [
          (lib.types.enum [
            "system"
            "light"
            "dark"
            "high-contrast-dark"
            "high-contrast-light"
          ])
          (lib.types.submodule {
            options = {
              accent = lib.mkOption { type = lib.types.str; };
              bgOne = lib.mkOption { type = lib.types.str; };
              bgTwo = lib.mkOption { type = lib.types.str; };
              borderOne = lib.mkOption { type = lib.types.str; };
              textColor = lib.mkOption { type = lib.types.str; };
              textColorSecondary = lib.mkOption { type = lib.types.str; };
            };
          })
        ];
        description = ''
          CoolerControl theme. Use "system", "light", "dark",
          "high-contrast-dark", or "high-contrast-light" for a built-in theme,
          or provide a custom theme attribute set. Custom theme colors may be hex
          colors like "#568af2" or CoolerControl's persisted RGB strings like
          "86 138 242".
        '';
      };

      eyeCandy = lib.mkOption {
        type = lib.types.nullOr lib.types.bool;
        default = null;
        description = "Whether to enable CoolerControl eye candy UI effects.";
      };

      showOnboarding = lib.mkOption {
        type = lib.types.nullOr lib.types.bool;
        default = null;
        description = "Whether CoolerControl should show onboarding.";
      };

      collapsedMainMenu = lib.mkOption {
        type = lib.types.nullOr lib.types.bool;
        default = null;
        description = "Whether the CoolerControl main menu starts collapsed.";
      };

      hideMenuCollapseIcon = lib.mkOption {
        type = lib.types.nullOr lib.types.bool;
        default = null;
        description = "Whether to hide the CoolerControl main menu collapse icon.";
      };

      mainMenuWidthRem = lib.mkOption {
        type = lib.types.nullOr lib.types.number;
        default = null;
        description = "CoolerControl main menu width in rem.";
      };

      frequencyPrecision = lib.mkOption {
        type = lib.types.nullOr lib.types.number;
        default = null;
        description = "Number of decimal places used for frequency values.";
      };

      chartLineScale = lib.mkOption {
        type = lib.types.nullOr lib.types.number;
        default = null;
        description = "CoolerControl chart line scale.";
      };

      time24 = lib.mkOption {
        type = lib.types.nullOr lib.types.bool;
        default = null;
        description = "Whether CoolerControl should use 24-hour time.";
      };
    };
  };

  config = lib.mkIf cfg.enable {
    systemd.services.coolercontrol-autoconf = {
      description = "Apply declarative CoolerControl UI settings";
      wantedBy = [ "multi-user.target" ];
      wants = [
        "coolercontrold.service"
        "network-online.target"
      ];
      after = [
        "coolercontrold.service"
        "network-online.target"
      ];
      restartTriggers = [
        settingsFile
        cfg.package
      ];

      serviceConfig = {
        Type = "oneshot";
        LoadCredential = [ "${passwordCredential}:${cfg.passwordFile}" ];
        ExecStart = lib.escapeShellArgs (
          [
            (lib.getExe' cfg.package "coolercontrol-autoconf")
            "--url"
            cfg.url
            "--password-file"
            "%d/${passwordCredential}"
          ]
          ++ lib.optionals cfg.setPassword [
            "--set-password"
            "--coolercontrold"
            cfg.coolercontrold
          ]
          ++ lib.optionals cfg.wait [
            "--wait"
            "--wait-interval"
            cfg.waitInterval
          ]
          ++ [ settingsFile ]
        );
      };
    };
  };
}
