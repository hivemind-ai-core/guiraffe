"""Gate the mounted fixtures up to their production sizes (docs/performance.md)."""
import subprocess
from pathlib import Path

# SOAK_ITEMS counts keys per channel, clips per track, branches, or property groups
# for the corresponding fixtures. Each branch/group represents ten visible records.
MAXIMUM = {
    "grid": 10_000_000, "sheet": 100_000, "code": 10_000_000,
    "flow": 50_000, "timeline": 1_000, "gantt": 100_000,
    "gantt-chain": 100_000, "gantt-fan-out": 100_000, "tracks": 1_000,
    "chart-line": 10_000_000, "chart-scatter": 1_000_000, "chart-stream": 10_000,
    "flamegraph": 10_000_000, "tree": 100_000, "property-grid": 100_000,
    "markdown": 50_000, "mermaid": 500, "chat": 100_000,
    "bar-editor": 100_000, "curve-editor": 100_000,
}

# KNOWN_ISSUES.md#performance. Match the metric and the affected fixture size.
EXPECTED = {
    ("flamegraph", "p99"): {65_536, 131_072, 262_144},
    ("tracks", "p99"): {64, 128},
    ("timeline", "p99"): {1_000},
    ("mermaid", "mount"): {64},
    ("sheet", "RSS"): {100_000},
    ("code", "RSS"): {10_000_000},
}


def main():
    directory = Path("target/size-sweep")
    directory.mkdir(parents=True, exist_ok=True)
    failed = False
    for component, maximum in MAXIMUM.items():
        size = 16
        while True:
            report = directory / f"{component}-{size}.md"
            subprocess.run(
                ["target/release/examples/limits", component, "--start", str(size),
                 "--max", str(size), "--output", report], check=True,
            )
            row, = [line for line in report.read_text().splitlines()
                    if line.startswith(f"| {component} |")]
            _, _, passing, failing, reason, *_ = map(str.strip, row.strip("|").split("|"))
            if reason != "size cap" or passing != str(size):
                if failing != str(size):
                    raise ValueError(f"Invalid probe result: {row}")
                for metric in reason.split(", "):
                    if size in EXPECTED.get((component, metric), ()):
                        print(f"::notice::{component} {size} {metric}: KNOWN_ISSUES.md#performance")
                    else:
                        print(f"::error::{component} {size} exceeded {metric} budget")
                        failed = True
            if size == maximum:
                break
            size = min(size * 2, maximum)
    return int(failed)


if __name__ == "__main__":
    raise SystemExit(main())
