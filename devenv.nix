{ pkgs, lib, ... }:
{
  packages = [ pkgs.git ];
  languages.rust.enable = true;
  languages.rust.toolchainFile = ./rust-toolchain.toml;

  # Winit loads Wayland and keyboard libraries dynamically on Linux.
  enterShell = lib.optionalString pkgs.stdenv.isLinux ''
    export LD_LIBRARY_PATH="${lib.makeLibraryPath [ pkgs.wayland pkgs.libxkbcommon ]}''${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
  '';
}
