# Maintainer: 1mesles1 <https://github.com/1mesles1>
pkgname=frostbite-sim
pkgver=0.1.0
pkgrel=1
pkgdesc="Interactive native simulator for Frostbite-style graph cores (Interaction Nets + Work-Stealing) with SystemVerilog RTL export"
arch=('x86_64')
url="https://github.com/1mesles1/frostbite-sim"
license=('MIT')
depends=('gcc-libs' 'glibc' 'libxkbcommon' 'wayland' 'libglvnd')
makedepends=('cargo')
optdepends=('iverilog: run gate-level simulation of generated SystemVerilog RTL')
source=("$pkgname-$pkgver.tar.gz::$url/archive/refs/tags/v$pkgver.tar.gz")
sha256sums=('SKIP')

prepare() {
    cd "$pkgname-$pkgver"
    export RUSTUP_TOOLCHAIN=stable
    cargo fetch --locked --target "$(rustc -vV | sed -n 's/host: //p')"
}

build() {
    cd "$pkgname-$pkgver"
    export RUSTUP_TOOLCHAIN=stable
    export CARGO_TARGET_DIR=target
    cargo build --frozen --release
}

package() {
    cd "$pkgname-$pkgver"
    install -Dm755 "target/release/$pkgname" "$pkgdir/usr/bin/$pkgname"
    install -Dm644 LICENSE "$pkgdir/usr/share/licenses/$pkgname/LICENSE"
    install -Dm644 README.md "$pkgdir/usr/share/doc/$pkgname/README.md"
}

