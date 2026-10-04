# Builds the mocap host app natively on Windows (the equivalent of build.sh which
# cross compiles the Windows binary from Linux).
#
# See pkg/vision/mocap/doc/host_software.md for the tools that must be installed.
#
# Usage:
#   powershell -ExecutionPolicy Bypass -File pkg\vision\mocap\app\build_windows.ps1
#
# Passing -AssetsOnly will only prepare the files that get embedded into the app
# (so that it can then be built/run with 'cargo run --bin mocap_app').

param([switch]$AssetsOnly)

$ErrorActionPreference = 'Stop'

$WorkspaceDir = (Resolve-Path "$PSScriptRoot\..\..\..\..").Path

# Runs a native command and stops the script if it fails.
function Invoke-Native([scriptblock]$Command) {
    & $Command
    if ($LASTEXITCODE -ne 0) { throw "Failed with exit code ${LASTEXITCODE}:$Command" }
}

Push-Location $WorkspaceDir
try {
    # The parsers used at build time only support '\n' line endings.
    if ((Get-Content -Raw Cargo.toml) -match "`r`n") {
        throw "This checkout has CRLF line endings. Re-clone it with 'git clone -c core.autocrlf=false'."
    }

    # Eigen is needed by //pkg/vision/ffi (on Linux it comes from the system packages).
    if (-not (Test-Path ext\eigen3\Eigen)) {
        Invoke-Native { git clone --quiet --depth 1 --branch 3.4.0 https://gitlab.com/libeigen/eigen.git ext\eigen3 }
        if ((git -C ext\eigen3 rev-parse HEAD) -ne '3147391d946bb4b6c68edd901f2add6ac1f31f8c') {
            Remove-Item -Recurse -Force ext\eigen3
            throw 'The downloaded Eigen 3.4.0 has an unexpected commit hash.'
        }
    }

    # Install node.js dependencies (for the UI).
    #
    # NOTE: Install scripts are only needed by native modules that the UI doesn't use.
    if (-not (Test-Path node_modules)) {
        Invoke-Native { npm ci --ignore-scripts }
    }

    # Build the UI code.
    #
    # NOTE: This is the command run by the //pkg/vision/mocap/manager:app rule as
    # the builder only supports Linux.
    Invoke-Native {
        node node_modules\webpack\bin\webpack.js --config pkg\web\webpack.config.js `
            --env "entry=$WorkspaceDir\pkg\vision\mocap\manager\js\index.tsx" `
            --env "output=$WorkspaceDir\built\pkg\vision\mocap\manager\app.js"
    }

    # Generate icons for the app (see build_icons.sh).
    #
    # NOTE: ImageMagick on Windows can directly render SVGs so Inkscape isn't needed.
    $IconDir = 'out\mocap_app\icons'
    New-Item -ItemType Directory -Force $IconDir | Out-Null
    Invoke-Native {
        magick -background none -density 384 pkg\vision\mocap\app\icon.svg -resize 2048x2048 "$IconDir\icon_2k.png"
    }
    Invoke-Native { magick "$IconDir\icon_2k.png" -define icon:auto-resize=256,128,64,48,32,16 "$IconDir\icon.ico" }
    Invoke-Native { magick "$IconDir\icon_2k.png" -resize 256x256 "$IconDir\icon.qoi" }

    if ($AssetsOnly) { return }

    Invoke-Native { cargo build --release --bin mocap_app }

    New-Item -ItemType Directory -Force out\mocap_app\windows, dist\pkg\vision\mocap\app | Out-Null
    Copy-Item -Force target\release\mocap_app.exe out\mocap_app\windows\Mocap.exe
    Compress-Archive -Force out\mocap_app\windows\Mocap.exe dist\pkg\vision\mocap\app\mocap-windows-x64.zip
}
finally {
    Pop-Location
}
