{
  outputs = { self, nixpkgs }:
    let
      platform = "x86_64-linux";
      pkgs = nixpkgs.legacyPackages.${platform};
    in {
      devShells.${platform}.default = pkgs.mkShell rec {
        buildInputs = with pkgs; [
          libxkbcommon
          libGL

          # WINIT_UNIX_BACKEND=wayland
          wayland
          mold
        ];
        LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath buildInputs;
      };
    };
}
