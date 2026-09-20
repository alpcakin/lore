# Formula for the alpcakin/homebrew-tap repository. Copy it to Formula/lore.rb
# there and fill in the checksums from the release's SHA256SUMS.
class Lore < Formula
  desc "Command library that lives in your shell"
  homepage "https://github.com/alpcakin/lore"
  version "0.3.0"
  license any_of: ["MIT", "Apache-2.0"]

  on_macos do
    on_arm do
      url "https://github.com/alpcakin/lore/releases/download/v#{version}/lore-v#{version}-aarch64-apple-darwin.tar.gz"
      sha256 "a623d9a87c01ed9189fd37d2132bcaf09bc65ec2e4516ab88032693d1dfc3b56"
    end
    on_intel do
      url "https://github.com/alpcakin/lore/releases/download/v#{version}/lore-v#{version}-x86_64-apple-darwin.tar.gz"
      sha256 "2109f5c87cae35ea250ec36c3d160625e4a5bbac3db9a93e7ac3f8f5cb1c18fa"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/alpcakin/lore/releases/download/v#{version}/lore-v#{version}-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "dbdc7a99f640b89912e8d4072bd7e4bc69570305c5df7ddde4cd680f35a3d52a"
    end
    on_intel do
      url "https://github.com/alpcakin/lore/releases/download/v#{version}/lore-v#{version}-x86_64-unknown-linux-musl.tar.gz"
      sha256 "6bffa52f1ce0158b4614774f3d1711069eb0c59c4c03642d5699e065f22c621c"
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
