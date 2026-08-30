import sys
import subprocess

# Filter out '-flavor' and 'gnu' which ld.lld complains about if invoked as ld.lld directly
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

zig_path = r"C:\Users\Admin\AppData\Local\Microsoft\WinGet\Packages\zig.zig_Microsoft.Winget.Source_8wekyb3d8bbwe\zig-x86_64-windows-0.16.0\zig.exe"
cmd = [zig_path, "ld.lld", "-m", "armelf_linux_eabi"] + args
res = subprocess.run(cmd)
sys.exit(res.returncode)
