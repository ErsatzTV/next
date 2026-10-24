# Dev shell for the hardware-accelerated integration tests on NixOS.
#
# The *-sys crates dlopen their libraries at runtime rather than linking them, and a
# cargo-built test binary has no RPATH into the nix store, so those dlopens fail with
# "dlopen failed" unless the libraries are on LD_LIBRARY_PATH. Nothing here is needed
# to *build* the workspace -- only to run the --ignored tests.
#
# The VAAPI drivers themselves (i965_drv_video.so, iHD_drv_video.so, ...) already live
# at the stable /run/opengl-driver/lib/dri, so libva finds them without help.
{ pkgs ? import <nixpkgs> { } }:

pkgs.mkShell {
  # No rust toolchain on purpose: cargo comes from the ambient (home-manager) profile.

  shellHook = ''
    # /run/opengl-driver/lib holds libraries owned by the *running* driver, not by
    # nixpkgs: libcuda.so.1, libnvcuvid.so.1 and libnvidia-encode.so.1 for the cuda
    # tests. They have to match the loaded kernel module, so never swap in a nixpkgs
    # package for these. Vulkan is split across both -- the ICDs (libvulkan_*.so) are
    # here, but the loader (libvulkan.so.1) comes from vulkan-loader below.
    export LD_LIBRARY_PATH="/run/opengl-driver/lib:${pkgs.lib.makeLibraryPath [
      pkgs.libva # libva.so.2 / libva-drm.so.2, for the vaapi tests
      pkgs.vulkan-loader # libvulkan.so.1, for the libplacebo tonemap tests
      pkgs.libdrm
      # Add as needed for other backends, e.g.:
      #   pkgs.ocl-icd  # libOpenCL.so, for the tonemap/pad_opencl paths
      #   pkgs.libvpl   # libvpl.so.2, for the qsv tests
    ]}''${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
  '';
}
