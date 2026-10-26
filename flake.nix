{
  description = "nvmetoy, a toy userspace NVMe driver";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  # nixpkgs has no xnvme package; build libxnvme from the release the
  # xnvme-sys crate was generated against.
  inputs.xnvme-src = {
    url = "github:xnvme/xnvme/v0.7.5";
    flake = false;
  };

  outputs = { self, nixpkgs, xnvme-src }:
    let
      system = "x86_64-linux";
      pkgs = nixpkgs.legacyPackages.${system};

      xnvme = pkgs.stdenv.mkDerivation {
        pname = "xnvme";
        version = "0.7.5";
        src = xnvme-src;
        nativeBuildInputs = [ pkgs.meson pkgs.ninja pkgs.pkg-config ];
        buildInputs = [ pkgs.libaio pkgs.liburing ];
        mesonFlags = [
          "-Dwith-spdk=disabled"
          "-Dwith-libvfn=disabled"
          "-Dwith-isal=disabled"
          "-Dbuild_subprojects=false"
          "-Dexamples=false"
          "-Dtests=false"
        ];
      };

      nvmetoy = pkgs.rustPlatform.buildRustPackage {
        pname = "nvmetoy";
        version = "0.1.0";
        src = self;
        cargoLock.lockFile = ./Cargo.lock;
        buildFeatures = [ "xnvme" ];
        nativeBuildInputs = [ pkgs.pkg-config pkgs.rustPlatform.bindgenHook ];
        buildInputs = [ xnvme ];
      };
    in {
      packages.${system} = {
        inherit xnvme nvmetoy;
        default = nvmetoy;
      };

      devShells.${system}.default = pkgs.mkShell {
        inputsFrom = [ nvmetoy ];
        packages = [ pkgs.clippy pkgs.rustfmt pkgs.rust-analyzer ];
      };
    };
}
