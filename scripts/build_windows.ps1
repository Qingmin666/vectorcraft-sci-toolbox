$ErrorActionPreference = 'Stop'
$taskProject = Split-Path -Parent $PSScriptRoot
Set-Location -LiteralPath $taskProject
if (!(Test-Path -LiteralPath '.venv-build/Scripts/python.exe')) {
    python -m venv .venv-build
    if ($LASTEXITCODE -ne 0) { throw 'Unable to create isolated build environment' }
    & '.venv-build/Scripts/python.exe' -m pip install --index-url https://pypi.org/simple -r requirements-build.txt
    if ($LASTEXITCODE -ne 0) { throw 'Unable to install build dependencies' }
}
& '.venv-build/Scripts/python.exe' -m PyInstaller --noconfirm --clean --onefile --windowed --name VectorCraftSciToolbox --paths src --add-data 'src/vectorcraft_sci/web:vectorcraft_sci/web' run.py
if ($LASTEXITCODE -ne 0) { throw 'Build failed' }
Write-Output (Join-Path $taskProject 'dist/VectorCraftSciToolbox.exe')
