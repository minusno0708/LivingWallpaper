param(
    [switch]$TestOnly,
    [string]$Toolchain = 'stable',
    [string[]]$AdditionalRustFlags = @()
)
$ErrorActionPreference = 'Stop'
$previousEncodedFlags = $env:CARGO_ENCODED_RUSTFLAGS
Push-Location $PSScriptRoot
try {
    if (-not $previousEncodedFlags -and $env:RUSTFLAGS) {
        throw 'Move RUSTFLAGS into CARGO_ENCODED_RUSTFLAGS or AdditionalRustFlags, then unset RUSTFLAGS. Whitespace parsing could alter custom flags.'
    }
    $flags = [Collections.Generic.List[string]]::new()
    if ($previousEncodedFlags) {
        foreach ($flag in $previousEncodedFlags.Split([char]31)) { $flags.Add($flag) }
    }
    foreach ($flag in $AdditionalRustFlags) { $flags.Add($flag) }

    $profileRoot = [Environment]::GetFolderPath('UserProfile')
    $cargoRoot = if ($env:CARGO_HOME) { [IO.Path]::GetFullPath($env:CARGO_HOME) } else { Join-Path $profileRoot '.cargo' }
    $rustupRoot = if ($env:RUSTUP_HOME) { [IO.Path]::GetFullPath($env:RUSTUP_HOME) } else { Join-Path $profileRoot '.rustup' }
    $targetRoot = if ($env:CARGO_TARGET_DIR) { [IO.Path]::GetFullPath($env:CARGO_TARGET_DIR) } else { Join-Path $PSScriptRoot 'target' }
    $sysroot = & rustc "+$Toolchain" --print sysroot
    if ($LASTEXITCODE -ne 0) { throw 'Cannot locate the selected Rust toolchain' }
    $maps = @(
        @{ Source = $profileRoot; Target = '/build/home' },
        @{ Source = $env:HOME; Target = '/build/home' },
        @{ Source = $cargoRoot; Target = '/build/cargo' },
        @{ Source = $rustupRoot; Target = '/build/rustup' },
        @{ Source = $sysroot.Trim(); Target = '/build/toolchain' },
        @{ Source = $targetRoot; Target = '/build/target' },
        @{ Source = $PSScriptRoot; Target = '/workspace/living-wallpaper' }
    )
    foreach ($map in $maps) {
        if ([string]::IsNullOrWhiteSpace($map.Source)) { continue }
        $source = $map.Source.TrimEnd([char[]]@('\', '/'))
        $variants = @($source, $source.Replace('\', '/'))
        if ($source -match '^[A-Za-z]:\\') { $variants += '\\?\' + $source }
        foreach ($variant in ($variants | Select-Object -Unique)) {
            $flags.Add('--remap-path-prefix=' + $variant + '=' + $map.Target)
        }
    }
    # Encoded flags preserve spaces in Windows paths and custom argument boundaries.
    $env:CARGO_ENCODED_RUSTFLAGS = $flags -join [char]31
    & cargo "+$Toolchain" test --locked
    if ($LASTEXITCODE -ne 0) { throw 'Tests failed' }
    if ($TestOnly) { return }
    & cargo "+$Toolchain" build --release --locked
    if ($LASTEXITCODE -ne 0) { throw 'Build failed' }

    $binary = Join-Path $targetRoot 'release\living-wallpaper.exe'
    if (-not (Test-Path -LiteralPath $binary -PathType Leaf)) { throw 'Release executable not found' }
    # Compiler remapping is best-effort; inspect the bytes before packaging as well.
    $bytes = [IO.File]::ReadAllBytes($binary)
    $byteEncoding = [Text.Encoding]::GetEncoding(28591)
    $raw = $byteEncoding.GetString($bytes)
    foreach ($map in $maps) {
        if ([string]::IsNullOrWhiteSpace($map.Source)) { continue }
        foreach ($candidate in @($map.Source, $map.Source.Replace('\', '/'))) {
            foreach ($encoding in @([Text.Encoding]::UTF8, [Text.Encoding]::Unicode)) {
                $pattern = $byteEncoding.GetString($encoding.GetBytes($candidate))
                if ($raw.IndexOf($pattern, [StringComparison]::OrdinalIgnoreCase) -ge 0) {
                    throw 'Release executable contains a local build path. Distribution was not updated.'
                }
            }
        }
    }
    $dist = Join-Path $PSScriptRoot 'dist'
    # Do not mix a checked release with files left by a previous package.
    if (Test-Path -LiteralPath $dist) { throw 'dist already exists. Archive or remove the old package before creating a new one.' }
    New-Item -ItemType Directory -Path $dist | Out-Null
    Copy-Item -LiteralPath $binary -Destination $dist
    foreach ($name in @('README.md', 'LICENSE', 'THIRD_PARTY_NOTICES.md')) {
        Copy-Item -LiteralPath (Join-Path $PSScriptRoot $name) -Destination $dist
    }
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'third-party-licenses') -Destination $dist -Recurse
    Write-Host 'Built and checked: dist\living-wallpaper.exe'
} finally {
    $env:CARGO_ENCODED_RUSTFLAGS = $previousEncodedFlags
    Pop-Location
}
