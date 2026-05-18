{
  inputs = {
    flake-parts.url = "github:hercules-ci/flake-parts";
    naersk.url = "github:nix-community/naersk";
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
  };

  outputs = inputs @{ flake-parts, ... }:
    flake-parts.lib.mkFlake {inherit inputs;}
		{
			imports = [ flake-parts.flakeModules.easyOverlay ];
			systems = ["x86_64-linux" "x86_64-darwin"];
			perSystem = {pkgs,config,...}: 
			let
				naersk' = pkgs.callPackage inputs.naersk {};
			in{
				overlayAttrs = {
					inherit (config.packages) bullen-server;
				};

        packages.bullen-server = naersk'.buildPackage {
          src = ./.;
        };

        devShells.default = pkgs.mkShell {
          nativeBuildInputs = with pkgs; [ rustc cargo rust-analyzer rustfmt ];
        };
			};
		};
}
