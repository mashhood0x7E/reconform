$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$exeName = if ($IsWindows) {'reconform.exe'} else {'reconform'}
$exe = Join-Path $root "target/debug/$exeName"
$fixtures = Join-Path $root 'crates/reconform-cli/examples/recorded'
function Assert-Schema($json, $schema) {
    if (!(Test-Json -Json $json -SchemaFile (Join-Path $root "docs/schemas/$schema.schema.json") -ErrorAction Stop)) {throw "Invalid $schema document"}
}
foreach ($pair in @(@('reference','bundle'),@('plan','plan'),@('contracts','contracts'))) {
    Assert-Schema (Get-Content (Join-Path $fixtures "$($pair[0]).json") -Raw) $pair[1]
}
$comparison = (& $exe compare (Join-Path $fixtures 'reference.json') (Join-Path $fixtures 'candidate.json') --plan (Join-Path $fixtures 'plan.json') --format json) -join "`n"
if ($LASTEXITCODE -ne 1) {throw 'Expected mismatch'}
Assert-Schema $comparison report
$contract = (& $exe check (Join-Path $fixtures 'candidate.json') --plan (Join-Path $fixtures 'contracts.json') --format json) -join "`n"
if ($LASTEXITCODE -ne 0) {throw 'Expected contract pass'}
Assert-Schema $contract report
$errorReport = (& $exe compare --format json) -join "`n"
if ($LASTEXITCODE -ne 2) {throw 'Expected configuration error'}
Assert-Schema $errorReport report
$invalid = Get-Content (Join-Path $fixtures 'plan.json') -Raw | ConvertFrom-Json -AsHashtable
$invalid.checks[0].policy.typo = $true
if (Test-Json -Json ($invalid | ConvertTo-Json -Depth 20) -SchemaFile (Join-Path $root 'docs/schemas/plan.schema.json') -ErrorAction SilentlyContinue) {throw 'Schema accepted an unknown policy field'}
Write-Output 'Bundle, plan, contract, report and error schemas verified.'
exit 0
