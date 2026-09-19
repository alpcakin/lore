# Formula for the alpcakin/homebrew-tap repository. Copy it to Formula/lore.rb
# there and fill in the checksums from the release's SHA256SUMS.
class Lore < Formula
  desc "Command library that lives in your shell"
  homepage "https://github.com/alpcakin/lore"
  version "0.1.2"
  license any_of: ["MIT", "Apache-2.0"]

  on_macos do
    on_arm do
      url "https://github.com/alpcakin/lore/releases/download/v#{version}/lore-v#{version}-aarch64-apple-darwin.tar.gz"
      sha256 "fed0dfed1fd9754ab5468aa6af179b69d47b3319d9d237fa37ebf679d09ae842"
    end
    on_intel do
      url "https://github.com/alpcakin/lore/releases/download/v#{version}/lore-v#{version}-x86_64-apple-darwin.tar.gz"
      sha256 "95fbc892fe67996c39e65f0a576077598b16b20cd5e1a30cf44a1521a5f3bacd"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/alpcakin/lore/releases/download/v#{version}/lore-v#{version}-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "2e6cf4e3091742a3e6a3b9f5ba6af327482a5f9e628f3c68cc446ced1e2a4390"
    end
    on_intel do
      url "https://github.com/alpcakin/lore/releases/download/v#{version}/lore-v#{version}-x86_64-unknown-linux-musl.tar.gz"
      sha256 "6157ce4cd7d88fc6c332dca35e9ff44e7c1d5b0831863d9eb15e3ee0f0707034"
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
