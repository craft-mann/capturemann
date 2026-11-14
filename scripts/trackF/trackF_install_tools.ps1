# Installs the build tools for a one-off cm-desktop build on the lab laptop (Track F, Nov 14, 2026).
# Run from an elevated session (the ssh user is an administrator) and keep the ssh session open.
# Writes C:\lab\trackF-install.log. Tools: CMake, rustup, Visual Studio Build Tools (C++), LLVM (libclang for bindgen).
param([string[]]$Only = @('cmake','rustup','vsbuildtools','llvm'))
$log = 'C:\lab\trackF-install.log'
"start $(Get-Date)" | Out-File $log -Append
function Step($name, $block) { if ($Only -notcontains $name) { return }; "== $name $(Get-Date)" | Out-File $log -Append; & $block 2>&1 | Out-File $log -Append; "exit $LASTEXITCODE" | Out-File $log -Append }
Step 'cmake' { winget install --id Kitware.CMake -e --silent --accept-package-agreements --accept-source-agreements }
Step 'rustup' { winget install --id Rustlang.Rustup -e --silent --accept-package-agreements --accept-source-agreements }
Step 'vsbuildtools' { winget install --id Microsoft.VisualStudio.2022.BuildTools -e --silent --accept-package-agreements --accept-source-agreements --override "--quiet --wait --norestart --nocache --add Microsoft.VisualStudio.Workload.VCTools --add Microsoft.VisualStudio.Component.Windows11SDK.22621 --includeRecommended" }
Step 'llvm' { winget install --id LLVM.LLVM -e --silent --accept-package-agreements --accept-source-agreements }
"done $(Get-Date)" | Out-File $log -Append
