# Construit les installateurs signés et le fichier latest.json lu par la mise à jour automatique.
#
#   .\scripts\release.ps1                      construit seulement
#   .\scripts\release.ps1 -Publish             construit puis crée la release GitHub v<version>
#   .\scripts\release.ps1 -Publish -Notes "…"  avec le texte affiché dans Réglages → Mises à jour
#
# La version vient de src-tauri/tauri.conf.json. La clé privée de signature reste hors du dépôt :
# sans elle, plus aucune mise à jour ne peut être publiée pour les versions déjà installées.

param(
    [switch]$Publish,
    [string]$Notes = "",
    [string]$Repo = "Arkyan/Kiosky",
    [string]$Key = "$HOME\.tauri\kiosky.key"
)

$ErrorActionPreference = "Stop"
Set-Location (Split-Path $PSScriptRoot -Parent)

if (-not (Test-Path $Key)) { throw "Clé de signature introuvable : $Key" }
$version = (Get-Content src-tauri/tauri.conf.json -Raw | ConvertFrom-Json).version
$tag = "v$version"

$env:TAURI_SIGNING_PRIVATE_KEY = Get-Content $Key -Raw
$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = ""
npm run tauri build
if ($LASTEXITCODE -ne 0) { throw "La construction a échoué." }

$bundle = "src-tauri/target/release/bundle"
$nsis = Get-Item "$bundle/nsis/Kiosky_${version}_x64-setup.exe"
$msi = Get-Item "$bundle/msi/Kiosky_${version}_x64_*.msi" | Select-Object -First 1

function Platform($file) {
    @{
        signature = (Get-Content "$($file.FullName).sig" -Raw).Trim()
        url       = "https://github.com/$Repo/releases/download/$tag/$($file.Name)"
    }
}

# « windows-x86_64 » sert aux versions qui ne connaissent pas leur type d'installateur.
$latest = [ordered]@{
    version   = $version
    notes     = $Notes
    pub_date  = (Get-Date).ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ssZ")
    platforms = [ordered]@{
        "windows-x86_64"      = Platform $nsis
        "windows-x86_64-nsis" = Platform $nsis
        "windows-x86_64-msi"  = Platform $msi
    }
}
$json = "$bundle/latest.json"
# Sans BOM : le lecteur JSON de la mise à jour ne l'accepte pas.
[IO.File]::WriteAllText((Join-Path (Get-Location) $json), ($latest | ConvertTo-Json -Depth 4))

$files = @($nsis.FullName, $msi.FullName, $json)
Write-Host "`nKiosky $version :"
$files | ForEach-Object { Write-Host "  $_" }

if ($Publish) {
    gh release create $tag @files --repo $Repo --title "Kiosky $version" --notes $Notes
    if ($LASTEXITCODE -ne 0) { throw "La publication a échoué." }
} else {
    Write-Host "`nRien n'a été publié (ajoute -Publish pour créer la release $tag)."
}
