# Formula for the alpcakin/homebrew-tap repository. Copy it to Formula/lore.rb
# there and fill in the checksums from the release's SHA256SUMS.
class Lore < Formula
  desc "Command library that lives in your shell"
  homepage "https://github.com/alpcakin/lore"
  version "0.1.1"
  license any_of: ["MIT", "Apache-2.0"]

  on_macos do
    on_arm do
      url "https://github.com/alpcakin/lore/releases/download/v#{version}/lore-v#{version}-aarch64-apple-darwin.tar.gz"
      sha256 "1949d5afab4e9fd72e64c5da23d5db298ec05887921335c5b4784c4801c45277"
    end
    on_intel do
      url "https://github.com/alpcakin/lore/releases/download/v#{version}/lore-v#{version}-x86_64-apple-darwin.tar.gz"
      sha256 "cc9f698a4521fa696ecaa9a2252ff957e8daeebbb9f3180345f715edb5cd76ee"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/alpcakin/lore/releases/download/v#{version}/lore-v#{version}-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "6ba6392b8b5c8150e11b057db0a58535f784d637b3154dc62bcfb940124ea4ac"
    end
    on_intel do
      url "https://github.com/alpcakin/lore/releases/download/v#{version}/lore-v#{version}-x86_64-unknown-linux-musl.tar.gz"
      sha256 "aaffc98b27999a9d072e8442e5cd36293595766ad5d2a3a4ebe8c3c1caa96804"
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
