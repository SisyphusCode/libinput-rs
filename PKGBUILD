pkgname=libinput-rs
pkgver=1.0.0
pkgrel=1
pkgdesc="Rust bindings and utilities for libinput"
arch=('x86_64')
url="https://github.com/SisyphusCode/libinput-rs"
license=('GPL3')
depends=('libinput')
makedepends=('cargo')
source=()

build() {
  cargo build --release --locked
}

package() {
  install -Dm644 target/release/liblibinput_rs.so "$pkgdir/usr/lib/liblibinput_rs.so"
}
