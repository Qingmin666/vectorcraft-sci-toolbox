param(
    [string]$RuntimeRoot = (Join-Path $env:LOCALAPPDATA 'VectorCraftSciBuild'),
    [string]$ToolchainRoot,
    [switch]$Test,
    [switch]$LocalPreview
)
$ErrorActionPreference = 'Stop'
$project = Split-Path $PSScriptRoot -Parent
$identity = Get-Content -Raw (Join-Path $project 'native/upstream.json') | ConvertFrom-Json
$RuntimeRoot = [IO.Path]::GetFullPath($RuntimeRoot)
if ($RuntimeRoot -match '[^\x00-\x7F]') { throw 'RuntimeRoot must use an ASCII path for Windows GNU tools.' }
New-Item -ItemType Directory -Force -Path $RuntimeRoot | Out-Null
$source = Join-Path $RuntimeRoot 'source'
if (-not (Test-Path (Join-Path $source '.git'))) {
    if (Test-Path $source) { throw 'Source directory exists but is not a Git checkout. Choose a fresh RuntimeRoot.' }
    git clone --depth 1 --branch $identity.tag $identity.repository $source
    if ($LASTEXITCODE -ne 0) { throw 'Upstream clone failed.' }
}
$actual = (git -C $source rev-parse HEAD).Trim()
if ($LASTEXITCODE -ne 0 -or $actual -ne $identity.commit) { throw 'Source commit differs from upstream.json. Existing files were preserved.' }
$patch = Join-Path $project 'native/patches/host-integration.patch'
$previousPreference = $ErrorActionPreference
$ErrorActionPreference = 'Continue'
git -C $source apply --reverse --check $patch 2>$null
$alreadyApplied = $LASTEXITCODE -eq 0
$ErrorActionPreference = $previousPreference
if (-not $alreadyApplied) {
    git -C $source apply --check $patch
    if ($LASTEXITCODE -ne 0) { throw 'Host integration patch conflicts. Existing source was preserved.' }
    git -C $source apply $patch
    if ($LASTEXITCODE -ne 0) { throw 'Host integration patch failed.' }
}
$overlay = Join-Path $project 'native/overlay'
Get-ChildItem $overlay -File -Recurse | ForEach-Object {
    $relative = $_.FullName.Substring($overlay.TrimEnd('\').Length + 1)
    $destination = Join-Path $source $relative
    New-Item -ItemType Directory -Force -Path (Split-Path $destination) | Out-Null
    Copy-Item -LiteralPath $_.FullName -Destination $destination -Force
}
if (-not $ToolchainRoot) { $ToolchainRoot = Join-Path $RuntimeRoot 'toolchain' }
if (Test-Path (Join-Path $ToolchainRoot 'cargo-home/bin/cargo.exe')) {
    $env:CARGO_HOME = Join-Path $ToolchainRoot 'cargo-home'
    $env:RUSTUP_HOME = Join-Path $ToolchainRoot 'rustup-home'
    $env:PATH = (Join-Path $env:CARGO_HOME 'bin') + ';' + $env:PATH
}
$gcc = Join-Path $ToolchainRoot 'mingw64/bin/gcc.exe'
if (Test-Path $gcc) {
    $env:CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER = $gcc
    $env:PATH = (Split-Path $gcc) + ';' + $env:PATH
}
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) { throw 'Rust/Cargo is required. This script does not install system software.' }
$env:CARGO_TARGET_DIR = Join-Path $RuntimeRoot 'target'
if (-not $LocalPreview) {
    # Remove local source/cache paths from paths recorded by rustc in distributable builds.
    $flags = @("--remap-path-prefix=$RuntimeRoot=/vectorcraft-sci-build")
    if ($env:CARGO_HOME) { $flags += "--remap-path-prefix=$($env:CARGO_HOME)=/cargo" }
    if ($env:RUSTUP_HOME) { $flags += "--remap-path-prefix=$($env:RUSTUP_HOME)=/rustup" }
    $env:CARGO_ENCODED_RUSTFLAGS = $flags -join [char]31
}
Push-Location $source
try {
    if ($Test) {
        cargo test --release --locked -p vectorcraft-ui-egui scientific::tests -- --test-threads=1
    } else {
        cargo build --release --locked -p vectorcraft
    }
    if ($LASTEXITCODE -ne 0) { throw 'Native build/check failed.' }
} finally { Pop-Location }
if (-not $Test) { Write-Output (Join-Path $env:CARGO_TARGET_DIR 'release/vectorcraft.exe') }
