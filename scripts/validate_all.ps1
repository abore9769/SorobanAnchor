# validate_all.ps1 — Windows equivalent of validate_all.sh
# Validates all AnchorKit config files against config_schema.json.
# Converts TOML to JSON via yq/dasel (falls back to Python).
# Validates with ajv-cli if available, otherwise falls back to validate_config_strict.py.
$ErrorActionPreference = "Stop"

$Schema     = "config_schema.json"
$ConfigsDir = "configs"
$ValidatorPy = "validate_config_strict.py"
$Failed     = $false

function Die([string]$msg) { Write-Error "❌ $msg"; exit 1 }

if (-not (Test-Path $Schema)) { Die "Schema not found: $Schema" }

# ── TOML → JSON conversion ────────────────────────────────────────────────────
function ConvertToml-ToJson([string]$tomlFile) {
    $tmp = [System.IO.Path]::GetTempFileName() -replace '\.tmp$', '.json'

    if (Get-Command yq -ErrorAction SilentlyContinue) {
        yq -o=json '.' $tomlFile | Set-Content $tmp -Encoding UTF8
    } elseif (Get-Command dasel -ErrorAction SilentlyContinue) {
        dasel -f $tomlFile -r toml -w json '.' | Set-Content $tmp -Encoding UTF8
    } elseif (Get-Command python -ErrorAction SilentlyContinue) {
        $py = @"
import sys, json, pathlib
p = pathlib.Path(sys.argv[1])
try:
    import tomllib
except ImportError:
    try:
        import tomli as tomllib
    except ImportError:
        import toml as tomllib
        json.dump(tomllib.loads(p.read_text()), open(sys.argv[2], 'w'), indent=2)
        sys.exit(0)
json.dump(tomllib.loads(p.read_bytes()), open(sys.argv[2], 'w'), indent=2)
"@
        python -c $py $tomlFile $tmp
    } else {
        Die "No TOML converter found. Install yq, dasel, or python+toml."
    }
    return $tmp
}

# ── JSON validation ───────────────────────────────────────────────────────────
function Validate-Json([string]$jsonFile, [string]$label) {
    $ok = $false
    if (Get-Command ajv -ErrorAction SilentlyContinue) {
        $out = ajv validate -s $Schema -d $jsonFile --spec=draft7 --errors=text 2>&1
        $ok = ($LASTEXITCODE -eq 0)
        if (-not $ok) { Write-Host $out -ForegroundColor Red }
    } elseif ((Test-Path $ValidatorPy) -and (Get-Command python -ErrorAction SilentlyContinue)) {
        python $ValidatorPy $jsonFile $Schema
        $ok = ($LASTEXITCODE -eq 0)
    } else {
        Die "No validator found. Install ajv-cli (npm i -g ajv-cli) or python+jsonschema."
    }

    if ($ok) {
        Write-Host "  ✅ $label" -ForegroundColor Green
    } else {
        Write-Host "  ❌ $label" -ForegroundColor Red
    }
    return $ok
}

# ── main ──────────────────────────────────────────────────────────────────────
Write-Host "🔍 AnchorKit Config Validation" -ForegroundColor Cyan
Write-Host "Schema: $Schema"
Write-Host ""

$tmpFiles = @()

try {
    Get-ChildItem -Path $ConfigsDir -Include *.json,*.toml -File | ForEach-Object {
        $file  = $_.FullName
        $label = $_.Name

        if ($_.Extension -eq ".toml") {
            $tmp = ConvertToml-ToJson $file
            $tmpFiles += $tmp
            $label = "$label (converted from TOML)"
            $result = Validate-Json $tmp $label
        } else {
            $result = Validate-Json $file $label
        }

        if (-not $result) { $Failed = $true }
    }
} finally {
    $tmpFiles | ForEach-Object { if (Test-Path $_) { Remove-Item $_ -Force } }
}

Write-Host ""
if ($Failed) {
    Write-Host "❌ One or more configs failed validation." -ForegroundColor Red
    exit 1
}
Write-Host "✅ All configs valid." -ForegroundColor Green
