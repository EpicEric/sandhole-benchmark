let
  sources = import ./npins;
  pkgs = import sources.nixpkgs {
    overlays = [ (import sources.rust-overlay) ];
  };
in
pkgs.mkShell {
  packages = [
    pkgs.rust-bin.stable.latest.default
    pkgs.sandhole
    pkgs.sish
  ];
}
