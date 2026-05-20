# build_export.ps1
# Script d'export du visualiseur de cyclisme

# Force l'encodage de la console en UTF-8 pour les accents
$OutputEncoding = [System.Text.Encoding]::UTF8
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8

Write-Host "1. Compilation de l'application en mode release..." -ForegroundColor Cyan
cargo build --release
if ($LASTEXITCODE -ne 0) {
    Write-Error "Erreur lors de la compilation avec cargo. Annulation de l'export."
    exit $LASTEXITCODE
}

# Génération du nom du dossier cible avec la date et l'heure courante (format : AAAAMMJJ.HH.MM)
$date_str = Get-Date -Format "yyyyMMdd.HH.mm"
$export_dir_name = "export_$date_str"
$export_path = Join-Path "exports" $export_dir_name

Write-Host "2. Création du dossier d'export : $export_path" -ForegroundColor Cyan
if (!(Test-Path $export_path)) {
    New-Item -ItemType Directory -Force -Path $export_path | Out-Null
}

Write-Host "3. Copie de l'exécutable..." -ForegroundColor Cyan
$exe_src = "target/release/cycling-visualizer.exe"
if (Test-Path $exe_src) {
    Copy-Item -Path $exe_src -Destination $export_path -Force
} else {
    Write-Error "L'exécutable cycling-visualizer.exe n'a pas été trouvé dans target/release/."
    exit 1
}

Write-Host "4. Copie des données de courses (data/races)..." -ForegroundColor Cyan
$races_src = "data/races"
$races_dest = Join-Path $export_path "data/races"
if (Test-Path $races_src) {
    New-Item -ItemType Directory -Force -Path $races_dest | Out-Null
    Copy-Item -Path "$races_src\*" -Destination $races_dest -Recurse -Force
} else {
    Write-Warning "Le dossier data/races n'a pas été trouvé. Veuillez vous assurer d'avoir généré les données."
}

Write-Host "Export terminé avec succès dans le dossier : $export_path" -ForegroundColor Green
