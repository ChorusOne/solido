{ system ? builtins.currentSystem }:

let
  sources = import ./nix/sources.nix;
  pkgs = import sources.nixpkgs { };
  build = import ./build.nix { inherit sources pkgs; };

  name = "chorusone/solido";
  tag = "latest";

in pkgs.dockerTools.buildLayeredImage {
  inherit name tag;
  contents = [ build ];

  config = {
    Cmd = [];  #TODO Add correct command line
    Env = [];  #TODO Add correct env variables
    WorkingDir = "/";
  };
}
