# PowerShell wrapper to execute the Rust-based launcher on Windows.
# - Prefers an existing compiled binary (release or debug)
# - Rebuilds with cargo if sources changed
# - Falls back to Docker cross-build using aifo-coder-rust-builder

$ErrorActionPreference = "Stop"

function Test-CanRun([string]$p) {
  try {
    & $p --version *>$null
    return $true
  } catch {
    return $false
  }
}

function Test-NeedsRebuild([string]$binPath) {
  if (-not (Test-Path -LiteralPath $binPath)) { return $true }
  $binTime = (Get-Item -LiteralPath $binPath).LastWriteTimeUtc

  $cargoToml = Get-Item -LiteralPath "Cargo.toml" -ErrorAction SilentlyContinue
  if ($cargoToml -and $cargoToml.LastWriteTimeUtc -gt $binTime) { return $true }

  $srcDir = Get-Item -LiteralPath "src" -ErrorAction SilentlyContinue
  if ($srcDir) {
    $newer = Get-ChildItem -LiteralPath "src" -File -Recurse | Where-Object { $_.LastWriteTimeUtc -gt $binTime } | Select-Object -First 1
    if ($newer) { return $true }
  }

  return $false
}

# Detect Windows
$onWindows = ($env:OS -eq "Windows_NT") -or [System.Runtime.InteropServices.RuntimeInformation]::IsOSPlatform([System.Runtime.InteropServices.OSPlatform]::Windows)
$exe = if ($onWindows) { ".exe" } else { "" }

# Ensure rustup/cargo in PATH
if ($env:HOME) {
  $env:PATH = "$($env:HOME)\.cargo\bin;$env:PATH"
} elseif ($env:USERPROFILE) {
  $env:PATH = "$($env:USERPROFILE)\.cargo\bin;$env:PATH"
}

# Prefer a user-specified binary, else release; fallback to debug when present
$bin = if ($env:AIFO_CODER_BIN -and -not [string]::IsNullOrWhiteSpace($env:AIFO_CODER_BIN)) {
  $env:AIFO_CODER_BIN
} else {
  Join-Path "." ("target" + [IO.Path]::DirectorySeparatorChar + "release" + [IO.Path]::DirectorySeparatorChar + ("aifo-coder" + $exe))
}
$debugBin = Join-Path "." ("target" + [IO.Path]::DirectorySeparatorChar + "debug" + [IO.Path]::DirectorySeparatorChar + ("aifo-coder" + $exe))
if (-not (Test-Path -LiteralPath $bin) -and (Test-Path -LiteralPath $debugBin)) {
  $bin = $debugBin
}

# If an installed system binary exists (not this script), prefer it
$sysBinCmd = Get-Command aifo-coder -ErrorAction SilentlyContinue
$thisScript = $MyInvocation.MyCommand.Path
$possibleSelf = @($thisScript, (Join-Path (Split-Path -Parent $thisScript) "aifo-coder.cmd"))
if ($sysBinCmd -and -not ($possibleSelf -contains $sysBinCmd.Source) -and (Test-CanRun $sysBinCmd.Source)) {
  & $sysBinCmd.Source @args
  exit $LASTEXITCODE
}

# If cargo is available, rebuild when sources changed
$cargo = Get-Command cargo -ErrorAction SilentlyContinue
if ($cargo) {
  if (Test-NeedsRebuild $bin) {
    Write-Host "Building aifo-coder (release)..."
    & cargo build --release
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    $bin = Join-Path "." ("target" + [IO.Path]::DirectorySeparatorChar + "release" + [IO.Path]::DirectorySeparatorChar + ("aifo-coder" + $exe))
  }
  if ((Test-Path -LiteralPath $bin) -and (Test-CanRun $bin)) {
    & $bin @args
    exit $LASTEXITCODE
  }
}

# Docker fallback: build inside container
$docker = Get-Command docker -ErrorAction SilentlyContinue
if ($docker) {
  Write-Host "Building aifo-coder (release) using docker and rust:1-bookworm ..."
  # Platform hint (best-effort)
  $platform = switch ([System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture) {
    "X64"   { "linux/amd64" }
    "Arm64" { "linux/arm64" }
    default { "" }
  }
  $platArgs = if ($platform) { @("--platform", $platform) } else { @() }

  # Cross compile when host is Windows
  if ($onWindows) {
    $releaseFolder = "x86_64-pc-windows-gnu\"
    $cargoTarget   = @("--target", "x86_64-pc-windows-gnu")
  } else {
    $releaseFolder = ""
    $cargoTarget   = @()
  }

  # Paths for mounts
  $hostPwd   = $pwd.Path
  $targetDir = Join-Path $hostPwd "target"

  $homeDir = if ($env:HOME) { $env:HOME } else { $env:USERPROFILE }
  $registryDir = Join-Path $homeDir ".cargo\registry"
  $gitDir      = Join-Path $homeDir ".cargo\git"

  Write-Host "hostPwd: $hostPwd"
  Write-Host "targetDir: $targetDir"
  Write-Host "homeDir: $homeDir"
  Write-Host "registryDir: $registryDir"
  Write-Host "gitDir: $gitDir"

  $argsDocker = @("run") + $platArgs + @(
    "--rm",
    "-v", "${hostPwd}:/workspace",
    "-v", "${registryDir}:/root/.cargo/registry",
    "-v", "${gitDir}:/root/.cargo/git",
    "-v", "${targetDir}:/workspace/target",
    "-w", "/workspace",
    "aifo-coder-rust-builder",
    "cargo", "build", "--release"
  ) + $cargoTarget

  & docker @argsDocker
  if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

  $bin = if ($releaseFolder -ne "") {
    Join-Path "." ("target\" + $releaseFolder + "release\aifo-coder" + $exe)
  } else {
    Join-Path "." ("target\release\aifo-coder" + $exe)
  }

  if ((Test-Path -LiteralPath $bin) -and (Test-CanRun $bin)) {
    & $bin @args
    exit $LASTEXITCODE
  } else {
    Write-Error "Error: built binary at $bin is not executable on this host (architecture mismatch?)."
    Write-Error "Try: Remove-Item -Recurse -Force .\target; then re-run, or install Rust (https://rustup.rs) to build natively."
    exit 1
  }
}

Write-Error "Error: compiled launcher not found and neither cargo nor Docker is installed."
Write-Error "Please install Rust (https://rustup.rs), or install Docker so the wrapper can build inside a container."
exit 127
