# Maintainer: FLX <flx@evait.de>

pkgname=penv-git
pkgver=r1.0000000
pkgrel=1
pkgdesc="Pentester Environment - manage network and customer-specific environment variables across shell sessions"
arch=('x86_64')
url="https://github.com/evait-security/penv"
license=('MIT')
depends=('gcc-libs')
makedepends=('rust' 'cargo' 'git')
provides=('penv')
conflicts=('penv')
source=("$pkgname::git+https://github.com/evait-security/penv.git")
sha256sums=('SKIP')

pkgver() {
    cd "$pkgname"
    printf "r%s.%s" "$(git rev-list --count HEAD)" "$(git rev-parse --short HEAD)"
}

prepare() {
    cd "$pkgname"
    cargo fetch --target "$(rustc -vV | sed -n 's/host: //p')"
}

build() {
    cd "$pkgname"
    export RUSTUP_TOOLCHAIN=stable
    export CARGO_TARGET_DIR=target
    cargo build --release --all-features
}

check() {
    cd "$pkgname"
    export RUSTUP_TOOLCHAIN=stable
    cargo test --all-features
}

package() {
    cd "$pkgname"
    install -Dm755 "target/release/penv" "$pkgdir/usr/bin/penv"
    install -Dm644 LICENSE "$pkgdir/usr/share/licenses/$pkgname/LICENSE"
    install -Dm644 README.md "$pkgdir/usr/share/doc/$pkgname/README.md"
}
