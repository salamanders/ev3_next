import sys
import subprocess
import shutil
import glob
import os

# Filter out '-flavor' and 'gnu' which ld.lld complains about if invoked directly
args = []
skip_next = False
for arg in sys.argv[1:]:
    if skip_next:
        skip_next = False
        continue
    if arg == '-flavor':
        skip_next = True
        continue
    args.append(arg)

# Locate zig executable dynamically
zig_path = shutil.which("zig")
if not zig_path:
    # Search common winget package location
    pattern = os.path.expandvars(r"%LOCALAPPDATA%\Microsoft\WinGet\Packages\*zig*\zig.exe")
    matches = glob.glob(pattern, recursive=True)
    if matches:
        zig_path = matches[0]
    else:
        # Fallback to default
        zig_path = "zig"

cmd = [zig_path, "ld.lld", "-m", "armelf_linux_eabi"] + args
res = subprocess.run(cmd)
sys.exit(res.returncode)
