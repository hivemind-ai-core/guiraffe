"""Native first-click regression. Run under Xvfb at 1400x900 after building showcase.
Usage: python3 first_click.py /absolute/path/to/guiraffe-showcase /tmp/evidence
Requires xdotool and ImageMagick (magick). Uses only its own app process group.
"""
import os
from pathlib import Path
import signal
import subprocess
import sys
import time

out = Path(sys.argv[2])
out.mkdir(parents=True, exist_ok=True)
env = dict(os.environ, WAYLAND_DISPLAY="")
with (out / "app.log").open("w") as log:
    proc = subprocess.Popen([sys.argv[1], "SegmentedControl"], env=env,
                            stdout=log, stderr=log, start_new_session=True)
    try:
        time.sleep(3)
        if proc.poll() is not None:
            raise RuntimeError("showcase exited before the check")
        subprocess.run(["xdotool", "mousemove", "445", "275", "click", "1"], check=True)
        time.sleep(2)
        shot = out / "after-one-click.png"
        subprocess.run(["import", "-window", "root", str(shot)], check=True)
        contrast = float(subprocess.check_output([
            "magick", str(shot), "-format", "%[fx:p{440,265}.r-p{330,265}.r]", "info:"
        ], text=True))
        assert contrast > 0.02, f"indicator stayed on Preview after selecting Code: {contrast}"
        print("PASS: first click moved the painted indicator")
    finally:
        if proc.poll() is None:
            os.killpg(proc.pid, signal.SIGTERM)
        try:
            proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            os.killpg(proc.pid, signal.SIGKILL)
            proc.wait()
