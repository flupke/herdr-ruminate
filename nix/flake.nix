{
  inputs = {
    cccc.url = "github:moznion/cccc/v1.6.0";
    cccc.flake = false;
    # The pinned Herdr, herdr-app and herdr-core (`hc`) that the tests and the app sandbox run,
    # shared with the other apps on Herdr. To try a local change to it:
    # `--override-input herdr-common path:../herdr-common`.
    herdr-common.url = "github:flupke/herdr-common";
    herdr-common.inputs.nixpkgs.follows = "nixpkgs";
    herdr-common.inputs.rust-overlay.follows = "rust-overlay";
    herdr-common.inputs.cccc.follows = "cccc";
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    rust-overlay.inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs =
    { cccc, herdr-common, nixpkgs, rust-overlay, ... }:
    let
      # The systems herdr-common pins Herdr for.
      systems = [
        "aarch64-darwin"
        "aarch64-linux"
        "x86_64-linux"
      ];
    in
    {
      devShells = nixpkgs.lib.genAttrs systems (
        system:
        let
          pkgs = import nixpkgs {
            inherit system;
            overlays = [ (import rust-overlay) ];
          };
          pinned = herdr-common.packages.${system};
          isLinux = pkgs.stdenv.hostPlatform.isLinux;
          latestRustPlatform = pkgs.makeRustPlatform {
            cargo = pkgs.rust-bin.stable.latest.default;
            rustc = pkgs.rust-bin.stable.latest.default;
          };
          rustComplexityAnalyzer = latestRustPlatform.buildRustPackage {
            pname = "cccc-cli";
            version = "1.6.0";
            src = cccc;
            cargoLock = {
              lockFile = "${cccc}/Cargo.lock";
              outputHashes."tree-sitter-kotlin-0.4.0" =
                "sha256-O9zCo8G8321cpqVp4z8USwyoLtnJReNhWs1FpYa9IVQ=";
            };
            cargoBuildFlags = [ "--package=cccc-cli" ];
            cargoTestFlags = [ "--package=cccc-cli" ];
            buildInputs = pkgs.lib.optionals pkgs.stdenv.hostPlatform.isLinux [ pkgs.glibc.dev ];
            "BINDGEN_EXTRA_CLANG_ARGS_${pkgs.stdenv.hostPlatform.rust.rustcTarget}" =
              pkgs.lib.optionalString pkgs.stdenv.hostPlatform.isLinux (
                "-isystem ${pkgs.glibc.dev}/include"
              );
            nativeBuildInputs = [
              pkgs.git
              pkgs.rustPlatform.bindgenHook
            ];
          };
        in
        {
          default = pkgs.mkShell ({
            packages = [
              pkgs.rust-bin.stable."1.90.0".default
              pkgs.cargo-nextest
              pkgs.cargo-mutants
              pkgs.jq
              rustComplexityAnalyzer
              # The e2e tests of the Explore page (tests/explore-page) need Node 22.12 or later.
              pkgs.nodejs_22
              # The type check of the Explore page's client in `make lint`: TypeScript 7, the
              # native compiler.
              pkgs.typescript
            ]
            # The app sandbox (herdr-common's app-sandbox) runs bubblewrap, enters it with
            # nsenter, and relays each connection to a port it forwards through socat inside.
            ++ pkgs.lib.optionals isLinux [
              pkgs.bubblewrap
              pkgs.util-linux
              pkgs.socat
            ];
            # Not on PATH: `herdr` there stays the installed Herdr, which
            # `make install` and the live server use.
            TEST_HERDR_BIN_PATH = "${pinned.herdr}/bin/herdr";
            # e2e sends usage data unless this is set.
            E2E_TELEMETRY_DISABLED = "1";
            # The browser the Explore page e2e tests attach to over CDP, instead of the build
            # Playwright downloads, which does not run on NixOS. Playwright's headless shell,
            # patched by nixpkgs: unlike the full Chromium, it does not call Google services.
            E2E_CHROMIUM = pkgs.lib.optionalString isLinux "${pkgs.playwright-driver.components.chromium-headless-shell}/chrome-headless-shell-linux64/chrome-headless-shell";
          }
          # What the app sandbox runs (herdr-common's `app_sandbox::Programs::pinned`), Linux
          # only, off PATH as Herdr is: `herdr-app` and `hc` there stay the installed ones.
          // pkgs.lib.optionalAttrs isLinux {
            TEST_HERDR_APP_BIN_PATH = "${pinned.herdr-app}/bin/herdr-app";
            TEST_HC_BIN_PATH = "${pinned.hc}/bin/hc";
            # The app sandbox's display: a headless Weston, which starts its Xwayland.
            TEST_WESTON_BIN_PATH = "${pkgs.weston}/bin/weston";
            # Mesa's software Vulkan driver: the sandbox's herdr-app draws on the CPU, and never
            # opens the user's GPU.
            TEST_HERDR_APP_VULKAN_DRIVER = "${pkgs.mesa}/share/vulkan/icd.d/lvp_icd.${pkgs.stdenv.hostPlatform.uname.processor}.json";
          });
        }
      );
    };
}
