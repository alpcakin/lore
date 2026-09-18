# Formula for the alpcakin/homebrew-tap repository. Copy it to Formula/lore.rb
# there and fill in the checksums from the release's SHA256SUMS.
class Lore < Formula
  desc "Command library that lives in your shell"
  homepage "https://github.com/alpcakin/lore"
  version "0.1.0"
  license any_of: ["MIT", "Apache-2.0"]

  on_macos do
    on_arm do
      url "https://github.com/alpcakin/lore/releases/download/v#{version}/lore-v#{version}-aarch64-apple-darwin.tar.gz"
      sha256 "b13a5dcbbf36da3921d2e29080c47dc041fb1120eb4bacad21b94b7bb4974aec"
    end
    on_intel do
      url "https://github.com/alpcakin/lore/releases/download/v#{version}/lore-v#{version}-x86_64-apple-darwin.tar.gz"
      sha256 "59df8f7f0e80ed11f155eb24fc8cf1d28fa4dee0a2e408b886bf244178b45873"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/alpcakin/lore/releases/download/v#{version}/lore-v#{version}-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "34f21a1b6bde7f2ce3ab961fde7a57ff785038b0e5f30ce7969191a2c9e5b918"
    end
    on_intel do
      url "https://github.com/alpcakin/lore/releases/download/v#{version}/lore-v#{version}-x86_64-unknown-linux-musl.tar.gz"
      sha256 "717c56af3f27b840c1931213179dd0e1589766b9981aa914d780883cbb2125ad"
    end
  end

  def install
    bin.install "lore"
  end

  def caveats
    <<~EOS
      Run 'lore setup' once to install the shell keybinding.
    EOS
  end

  test do
    assert_match "lore", shell_output("#{bin}/lore init bash")
  end
end
