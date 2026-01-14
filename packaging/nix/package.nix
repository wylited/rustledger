{
  lib,
  rustPlatform,
  fetchFromGitHub,
}:

rustPlatform.buildRustPackage rec {
  pname = "rustledger";
  version = "1.0.0-rc.18";

  src = fetchFromGitHub {
    owner = "rustledger";
    repo = "rustledger";
    rev = "v${version}";
    hash = "sha256-RlJKQot8UXJjyF9NrR7rC2J0n3sIxb2KhixmgbhFua8=";
  };

  cargoHash = "sha256-RukZTMQpCPrut/DouVLJqW+65Zygupa5eB/wmQ2nU6c=";

  # Skip tests that require network or specific fixtures
  checkFlags = [
    "--skip=integration"
  ];

  meta = with lib; {
    description = "Fast, pure Rust implementation of Beancount double-entry accounting";
    homepage = "https://rustledger.github.io";
    changelog = "https://github.com/rustledger/rustledger/releases/tag/v${version}";
    license = licenses.gpl3Only;
    maintainers = with maintainers; [ ]; # Add your nixpkgs maintainer name
    mainProgram = "rledger-check";
  };
}
