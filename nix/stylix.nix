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

  options.stylix.targets.coolercontrol.enable =
    lib.mkEnableOption "CoolerControl Stylix theme integration";

  config = lib.mkIf cfg.enable {
    services.coolercontrol-autoconf = {
      enable = true;
      settings.theme = {
        accent = "#${colors.base0D}";
        bgOne = "#${colors.base00}";
        bgTwo = "#${colors.base01}";
        borderOne = "#${colors.base03}";
        textColor = "#${colors.base05}";
        textColorSecondary = "#${colors.base04}";
      };
    };
  };
}
