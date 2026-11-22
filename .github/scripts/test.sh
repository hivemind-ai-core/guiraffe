#!/usr/bin/env bash
# usage: test.sh <cargo test arguments>
# Runs cargo test and repeats what failed as annotations, which are readable on the run's summary
# and through the API without access to the job log.
set -uo pipefail
log=$(mktemp)
trap 'rm -f "$log"' EXIT
cargo test "$@" 2>&1 | tee "$log"
status=${PIPESTATUS[0]}
if [ "$status" -ne 0 ]; then
  python3 -I - "$log" <<'P'
import re
import sys

text = re.sub(r"\x1b\[[0-9;]*m", "", open(sys.argv[1], errors="replace").read())
lines = text.splitlines()


def annotate(title, found):
    body = "\n".join(dict.fromkeys(found))[:3900]
    if body:
        body = body.replace("%", "%25").replace("\r", "%0D").replace("\n", "%0A")
        print(f"::error title={title}::{body}")


annotate("Failed targets", [line.strip() for line in lines if re.match(r"\s+`-p |error: test failed", line)])
annotate("Failed tests", [line for line in lines if re.match(r"test .*(\.\.\. FAILED|\.\.\. mismatch)", line)])
annotate("Screenshot differences", [line[:420] for line in lines if "differing_pixels" in line])
panics = []
for i, line in enumerate(lines):
    if "panicked at" in line or line.startswith("Error: "):
        panics += [part[:300] for part in lines[i : i + 3]]
annotate("Panics and errors", panics)
annotate("Build errors", [line for line in lines if re.match(r"error(\[E\d+\])?: ", line)])
annotate("End of log", [line[:300] for line in lines[-14:]])
P
fi
exit "$status"
