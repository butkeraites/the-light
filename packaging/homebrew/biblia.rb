# Fórmula Homebrew para a Bíblia CLI.
#
# Instala o binário `biblia` pré-compilado das GitHub Releases. A cada release,
# atualize `version` e os quatro `sha256` (use os arquivos `*.sha256` anexados
# à release). Publique em um tap, ex.: `butkeraites/homebrew-tap`, e instale com:
#
#   brew install butkeraites/tap/biblia
#
# Alternativa sem fórmula: `cargo install --git https://github.com/butkeraites/the-light biblia-cli` ou baixar o binário direto.
class Biblia < Formula
  desc "Leitor de Bíblia hackeável para terminal (CLI + TUI), bilíngue PT/EN"
  homepage "https://github.com/butkeraites/the-light"
  version "1.0.0"
  license any_of: ["MIT", "Apache-2.0"]

  on_macos do
    on_arm do
      url "https://github.com/butkeraites/the-light/releases/download/v#{version}/biblia-v#{version}-aarch64-apple-darwin.tar.gz"
      sha256 "REPLACE_WITH_AARCH64_APPLE_DARWIN_SHA256"
    end
    on_intel do
      url "https://github.com/butkeraites/the-light/releases/download/v#{version}/biblia-v#{version}-x86_64-apple-darwin.tar.gz"
      sha256 "REPLACE_WITH_X86_64_APPLE_DARWIN_SHA256"
    end
  end

  on_linux do
    on_arm do
      odie "binários Linux ARM ainda não são publicados; use `cargo install --git https://github.com/butkeraites/the-light biblia-cli`"
    end
    on_intel do
      url "https://github.com/butkeraites/the-light/releases/download/v#{version}/biblia-v#{version}-x86_64-unknown-linux-musl.tar.gz"
      sha256 "REPLACE_WITH_X86_64_LINUX_MUSL_SHA256"
    end
  end

  def install
    # O tarball contém um diretório `biblia-vX.Y.Z-<target>/` com o binário.
    bin.install Dir["**/biblia"].first => "biblia"
  end

  test do
    assert_match "biblia", shell_output("#{bin}/biblia --version")
  end
end
