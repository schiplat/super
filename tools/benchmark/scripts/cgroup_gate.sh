#!/usr/bin/env bash
# STB-2-PRO requires cgroup v2 with a writable memory controller.
set -euo pipefail
if [[ "$(uname -s)" != "Linux" ]]; then
  echo "cgroup gate: not Linux — STB-2-PRO not applicable"
  exit 3
fi
if [[ ! -f /sys/fs/cgroup/cgroup.controllers ]]; then
  echo "cgroup gate FAIL: cgroup v2 not mounted at /sys/fs/cgroup" >&2
  exit 1
fi
if ! grep -qw memory /sys/fs/cgroup/cgroup.controllers; then
  echo "cgroup gate FAIL: memory controller not in cgroup.controllers" >&2
  echo "controllers: $(cat /sys/fs/cgroup/cgroup.controllers)" >&2
  exit 1
fi
if [[ ! -w /sys/fs/cgroup ]]; then
  echo "cgroup gate FAIL: /sys/fs/cgroup not writable" >&2
  exit 1
fi
echo "cgroup gate PASS: $(cat /sys/fs/cgroup/cgroup.controllers)"
