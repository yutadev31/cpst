{ pkgs, lib, ... }: {
  projectRootFile = "flake.nix";

  programs.nixfmt.enable = true;
  programs.rustfmt.enable = true;

  settings.formatter.tombi = {
    command = lib.getExe pkgs.tombi;
    options = [
      "format"
      "--offline"
    ];
    includes = [ "*.toml" ];
  };

  settings.global.excludes = [
    "result/**"
    "target/**"
  ];
}
