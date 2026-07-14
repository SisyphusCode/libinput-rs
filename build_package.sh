#!/usr/bin/env bash
set -e

echo "Parsing project specifications..."
VERSION="0.1.1"
ARCH=$(uname -m)

echo "Preparing local build tree structures..."
mkdir -p build_workspace/SOURCES
mkdir -p build_workspace/SPECS
mkdir -p build_workspace/BUILD build_workspace/RPMS build_workspace/SRPMS

echo "Vendoring dependencies for offline Copr build..."
cargo vendor > /dev/null 2>&1
mkdir -p .cargo
cat <<EOF > .cargo/config.toml
[source.crates-io]
replace-with = "vendored-sources"

[source.vendored-sources]
directory = "vendor"
EOF

tar --exclude='build_workspace' --exclude='.git' --exclude='target' -czf build_workspace/SOURCES/libinput-rs-${VERSION}.tar.gz .
cp libinput-rs.spec build_workspace/SPECS/

echo "Executing local RPM build framework..."
rpmbuild --define "_topdir $(pwd)/build_workspace" -ba build_workspace/SPECS/libinput-rs.spec

echo "========================================================"
echo "Build Finished. Packaged binary ready for distribution:"
echo "Location: build_workspace/RPMS/${ARCH}/libinput-rs-${VERSION}-1.${ARCH}.rpm"
echo "Install via: sudo dnf localinstall build_workspace/RPMS/${ARCH}/libinput-rs-*"
echo "========================================================"
