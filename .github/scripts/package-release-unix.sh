#!/usr/bin/env bash
# Package superd + super CLI into super-{version}-{platform}.tar.gz
set -euo pipefail

version="${1:?version required}"
platform="${2:?platform required (e.g. linux-amd64)}"
target="${3:-}"

if [[ -n "$target" ]]; then
  cargo build --release --target "$target" -p superd -p super-cli
  bin_dir="target/${target}/release"
else
  cargo build --release -p superd -p super-cli
  bin_dir="target/release"
fi

root="super-${version}-${platform}"
mkdir -p "${root}/bin" \
  "${root}/contrib/conf.d" \
  "${root}/contrib/systemd" \
  "${root}/contrib/launchd" \
  "${root}/contrib/rc.d"
cp "${bin_dir}/superd" "${bin_dir}/super" "${root}/bin/"
chmod +x "${root}/bin/"*

if [[ -f LICENSE ]]; then
  cp LICENSE "${root}/"
fi

if [[ -d packaging/contrib ]]; then
  cp packaging/contrib/super.toml.default "${root}/contrib/" 2>/dev/null || true
  cp packaging/contrib/README.md "${root}/contrib/" 2>/dev/null || true
  cp packaging/contrib/conf.d/demo.toml.example "${root}/contrib/conf.d/" 2>/dev/null || true
  cp packaging/contrib/systemd/superd.service "${root}/contrib/systemd/" 2>/dev/null || true
  cp packaging/contrib/launchd/com.schiplat.superd.plist "${root}/contrib/launchd/" 2>/dev/null || true
  cp packaging/contrib/rc.d/superd "${root}/contrib/rc.d/" 2>/dev/null || true
  chmod 755 "${root}/contrib/rc.d/superd" 2>/dev/null || true
fi

bash .github/scripts/write-release-readme.sh "${version}" "${platform}" "${root}"

tar -czf "${root}.tar.gz" "${root}"
echo "Created ${root}.tar.gz"
