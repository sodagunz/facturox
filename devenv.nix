{ pkgs, ... }:
{
  packages = [ pkgs.git ];
  languages.rust.enable = true;
  languages.rust.toolchainFile = ./rust-toolchain.toml;
}
