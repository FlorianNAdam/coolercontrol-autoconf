{
  config,
  lib,
  ...
}:

let
  cfg = config.stylix.targets.coolercontrol;
  colors = config.lib.stylix.colors;
in
{
  imports = [ ./module.nix ];

  options.stylix.targets.coolercontrol.enable = config.lib.stylix.mkEnableTarget "CoolerControl" true;

  config = lib.mkIf cfg.enable {
    services.coolercontrol-autoconf = {
      enable = true;
      settings.theme = {
        accent = "#${colors.base0B}";
        bgOne = "#${colors.base00}";
        bgTwo = "#${colors.base01}";
        borderOne = "#${colors.base02}";
        textColor = "#${colors.base07}";
        textColorSecondary = "#${colors.base0C}";
      };
    };
  };
}
