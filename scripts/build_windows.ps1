param(
    [string] $OutputDir = 'dist',
    [string] $AppName = 'VectorCraftSciToolbox'
)
$ErrorActionPreference = 'Stop'
$taskProject = Split-Path -Parent $PSScriptRoot
Set-Location -LiteralPath $taskProject
if (!(Test-Path -LiteralPath '.venv-build/Scripts/python.exe')) {
    python -m venv .venv-build
    if ($LASTEXITCODE -ne 0) { throw 'Unable to create isolated build environment' }
    & '.venv-build/Scripts/python.exe' -m pip install --index-url https://pypi.org/simple -r requirements-build.txt
    if ($LASTEXITCODE -ne 0) { throw 'Unable to install build dependencies' }
}
$taskOutputDir = [System.IO.Path]::GetFullPath($OutputDir, $taskProject)
$taskBuildState = Join-Path $taskProject '.runtime/build-windows'
$taskSpecDir = Join-Path $taskBuildState 'spec'
$taskWorkDir = Join-Path $taskBuildState 'work'
New-Item -ItemType Directory -Force -Path $taskOutputDir,$taskSpecDir,$taskWorkDir | Out-Null
$taskWeb = (Resolve-Path 'src/vectorcraft_sci/web').Path
& '.venv-build/Scripts/python.exe' -m PyInstaller --noconfirm --clean --onefile --windowed --name $AppName --paths src --add-data "$taskWeb;vectorcraft_sci/web" --distpath $taskOutputDir --workpath $taskWorkDir --specpath $taskSpecDir run.py
if ($LASTEXITCODE -ne 0) { throw 'Build failed' }
Write-Output (Join-Path $taskOutputDir "$AppName.exe")
