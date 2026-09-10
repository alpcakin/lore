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
      sha256 "replace-with-the-sha256-from-SHA256SUMS"
    end
    on_intel do
      url "https://github.com/alpcakin/lore/releases/download/v#{version}/lore-v#{version}-x86_64-apple-darwin.tar.gz"
      sha256 "replace-with-the-sha256-from-SHA256SUMS"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/alpcakin/lore/releases/download/v#{version}/lore-v#{version}-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "replace-with-the-sha256-from-SHA256SUMS"
    end
    on_intel do
      url "https://github.com/alpcakin/lore/releases/download/v#{version}/lore-v#{version}-x86_64-unknown-linux-musl.tar.gz"
      sha256 "replace-with-the-sha256-from-SHA256SUMS"
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
