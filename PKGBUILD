pkgname=libinput-rs
pkgver=0.1.1
pkgrel=2
pkgdesc="Rust bindings and utilities for libinput"
arch=('x86_64')
url="https://github.com/SisyphusAeolides/libinput-rs"
license=('GPL3')
depends=('libinput')
makedepends=('cargo')
source=()

build() {
  cd "$srcdir/.."
  cargo build --release --locked
}

package() {
  cd "$srcdir/.."
  install -Dm755 target/release/libinput-rs "$pkgdir/usr/bin/libinput-rs"
}
