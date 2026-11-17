#!/usr/bin/env bash
# GUIraffe vs Electron + React: measurement harness (plans/ELECTRON-COMPARISON.md).
#
#   bench/electron/run.sh            # full run: 20 runs per scenario per app (~1.5 h)
#   bench/electron/run.sh --smoke    # 2 runs, short idle; results marked INVALID
#   bench/electron/run.sh --runs 5 --scenarios startup,sort --apps guiraffe
#   bench/electron/run.sh --no-build # reuse existing builds
#
# A real measurement needs a quiet machine with a GPU: load average < 1.0, no Xvfb, no
# software rendering (llvmpipe/lavapipe/softpipe), `performance` CPU governor, AC power.
# Without --smoke the harness refuses to run otherwise (override: --force, which still
# marks the results INVALID).
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "$here/../.." && pwd)"
gapp="$repo/examples/compare/guiraffe-app"
eapp="$repo/examples/compare/electron-app"

smoke=0 force=0 build=1 pass=()
for a in "$@"; do
  case "$a" in
    --smoke) smoke=1 ;;
    --force) force=1 ;;
    --no-build) build=0 ;;
    -h|--help) sed -n '2,13p' "$0"; exit 0 ;;
    *) pass+=("$a") ;;
  esac
done

# Never build or write temp files on /tmp (small RAM disk on dev machines).
export TMPDIR="${TMPDIR_OVERRIDE:-$HOME/.cache/guiraffe-compare-tmp}"
mkdir -p "$TMPDIR"

# ------------------------------------------------------------------ validity checks
problems=()
load1=$(cut -d' ' -f1 /proc/loadavg)
if awk -v l="$load1" 'BEGIN { exit !(l > 1.0) }'; then
  problems+=("load average is $load1 (> 1.0): the machine is not quiet")
fi
disp="${DISPLAY:-}"
xvfb=0
[[ "$disp" =~ ^:9[0-9] ]] && xvfb=1
if [[ -n "$disp" ]] && pgrep -a Xvfb 2>/dev/null | grep -qE -- " ${disp%%.*}( |$)"; then xvfb=1; fi
((xvfb)) && problems+=("DISPLAY=$disp is an Xvfb server (software rendering)")
if [[ -z "$disp" && -z "${WAYLAND_DISPLAY:-}" ]]; then
  problems+=("no DISPLAY or WAYLAND_DISPLAY: no real display")
fi
if [[ -n "${LIBGL_ALWAYS_SOFTWARE:-}" ]]; then
  problems+=("LIBGL_ALWAYS_SOFTWARE is set")
fi
gpu_info=""
command -v glxinfo >/dev/null && gpu_info+="$(glxinfo -B 2>/dev/null || true)"
command -v vulkaninfo >/dev/null && gpu_info+="$(vulkaninfo --summary 2>/dev/null || true)"
if grep -qiE 'llvmpipe|lavapipe|softpipe|swrast|SwiftShader' <<<"$gpu_info"; then
  problems+=("software renderer in GPU info (llvmpipe/lavapipe/softpipe)")
fi
if [[ -z "$gpu_info" ]]; then
  problems+=("could not read GPU info (install mesa-utils/glxinfo or vulkan-tools); cannot confirm a hardware GPU")
fi
gov=$(cat /sys/devices/system/cpu/cpu0/cpufreq/scaling_governor 2>/dev/null || echo unknown)
[[ "$gov" == performance ]] || problems+=("CPU governor is '$gov' (want 'performance')")

invalid=0
if ((${#problems[@]})); then
  invalid=1
  echo "################################################################################" >&2
  echo "#  WARNING: RESULTS FROM THIS MACHINE STATE ARE NOT VALID MEASUREMENTS" >&2
  for p in "${problems[@]}"; do echo "#   - $p" >&2; done
  echo "################################################################################" >&2
  if ((!smoke && !force)); then
    echo "Refusing to run. Fix the above, or use --smoke (pipeline check) / --force." >&2
    exit 2
  fi
fi

# ------------------------------------------------------------------ build
if ((build)); then
  echo "== building guiraffe-app (release, lto=thin, gpui-fast)" >&2
  (cd "$gapp" && cargo build --release --locked)
  echo "== building electron-app (vite build + electron-builder --dir)" >&2
  (cd "$eapp" && pnpm install --frozen-lockfile && pnpm package)
fi

args=(--repo "$repo")
((smoke)) && args+=(--smoke)
((invalid)) && args+=(--invalid "$(IFS='; '; echo "${problems[*]}")")
exec python3 -I "$here/harness.py" "${args[@]}" "${pass[@]}"
