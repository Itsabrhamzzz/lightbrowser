# Runs cargo inside the VS Build Tools 2022 environment (the default VS 18 Community install has no C++ libs).
# Usage: .\build.ps1 run | build --release | test
$vc = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat"
$env:Path += ";$env:USERPROFILE\.cargo\bin"
cmd /c "`"$vc`" >nul 2>&1 && cargo $($args -join ' ')"
