# Maintainer: Ozan Özdil <ozdil>
pkgname=omarchy-omaprot
pkgver=1.0.0
pkgrel=1
pkgdesc="CachyOS-grade bleeding-edge Proton optimization engine, Linux 7.2+ NTSYNC/BORE orchestrator, and Quickshell widget for Omarchy"
arch=('x86_64')
url="https://github.com/ozdil/omarchy-omaprot"
license=('MIT')
depends=('glibc' 'gcc-libs')
makedepends=('cargo' 'rust' 'git')
source=()
sha256sums=()

build() {
    cd "${startdir}"
    export RUSTUP_TOOLCHAIN=stable
    export CARGO_TARGET_DIR=target
    cargo build --release --locked
}

check() {
    cd "${startdir}"
    export RUSTUP_TOOLCHAIN=stable
    cargo test --locked
}

package() {
    cd "${startdir}"
    install -Dm755 "target/release/omaprot-engine" "${pkgdir}/usr/bin/omaprot-engine"
    install -Dm755 "omaprot-dashboard" "${pkgdir}/usr/share/omarchy/plugins/ozdil.omaprot/omaprot-dashboard"
    install -Dm755 "omaprot-status" "${pkgdir}/usr/share/omarchy/plugins/ozdil.omaprot/omaprot-status"
    install -Dm755 "target/release/omaprot-engine" "${pkgdir}/usr/share/omarchy/plugins/ozdil.omaprot/omaprot-engine"
    install -Dm644 "manifest.json" "${pkgdir}/usr/share/omarchy/plugins/ozdil.omaprot/manifest.json"
    install -Dm644 "Panel.qml" "${pkgdir}/usr/share/omarchy/plugins/ozdil.omaprot/Panel.qml"
    install -Dm644 "qml/shell.qml" "${pkgdir}/usr/share/omarchy/plugins/ozdil.omaprot/qml/shell.qml"
    install -Dm644 "qml/MainWindow.qml" "${pkgdir}/usr/share/omarchy/plugins/ozdil.omaprot/qml/MainWindow.qml"
    install -Dm644 "qml/theme/Theme.qml" "${pkgdir}/usr/share/omarchy/plugins/ozdil.omaprot/qml/theme/Theme.qml"
    install -Dm644 "qml/theme/qmldir" "${pkgdir}/usr/share/omarchy/plugins/ozdil.omaprot/qml/theme/qmldir"
    install -Dm644 "README.md" "${pkgdir}/usr/share/doc/${pkgname}/README.md"
    install -Dm644 "CONTRIBUTING.md" "${pkgdir}/usr/share/doc/${pkgname}/CONTRIBUTING.md"
    install -Dm644 "LICENSE" "${pkgdir}/usr/share/licenses/${pkgname}/LICENSE"
}
