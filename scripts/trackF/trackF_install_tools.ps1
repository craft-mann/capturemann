# Installs the build tools for a one-off cm-desktop build on the lab laptop (Track F, Nov 14, 2026).
# Run detached: writes C:\lab\trackF-install.log. Needs an elevated session (the ssh user is an administrator).
$log = 'C:\lab\trackF-install.log'
"start $(Get-Date)" | Out-File $log
function Step($name, $block) { "== $name $(Get-Date)" | Out-File $log -Append; & $block 2>&1 | Out-File $log -Append; "exit $LASTEXITCODE" | Out-File $log -Append }
Step 'cmake' { winget install --id Kitware.CMake -e --silent --accept-package-agreements --accept-source-agreements }
Step 'rustup' { winget install --id Rustlang.Rustup -e --silent --accept-package-agreements --accept-source-agreements }
Step 'vsbuildtools' { winget install --id Microsoft.VisualStudio.2022.BuildTools -e --silent --accept-package-agreements --accept-source-agreements --override "--quiet --wait --norestart --nocache --add Microsoft.VisualStudio.Workload.VCTools --add Microsoft.VisualStudio.Component.Windows11SDK.22621 --includeRecommended" }
"done $(Get-Date)" | Out-File $log -Append
