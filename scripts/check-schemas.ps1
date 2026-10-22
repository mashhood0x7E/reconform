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
$reportPath = Join-Path $root ('target/schema-report-' + [guid]::NewGuid().ToString('N') + '.json')
[IO.File]::WriteAllText($reportPath,$comparison)
$junit = (& $exe report $reportPath --format junit) -join "`n"
if ($LASTEXITCODE -ne 1) {throw 'Expected saved mismatch exit'}
$xml = [xml]$junit
if ([int]$xml.testsuite.failures -ne 1 -or [int]$xml.testsuite.tests -ne 1) {throw 'Unexpected JUnit counts'}
$contract = (& $exe check (Join-Path $fixtures 'candidate.json') --plan (Join-Path $fixtures 'contracts.json') --format json) -join "`n"
if ($LASTEXITCODE -ne 0) {throw 'Expected contract pass'}
Assert-Schema $contract report
$errorReport = (& $exe compare --format json) -join "`n"
if ($LASTEXITCODE -ne 2) {throw 'Expected configuration error'}
Assert-Schema $errorReport report
$invalid = Get-Content (Join-Path $fixtures 'plan.json') -Raw | ConvertFrom-Json -AsHashtable
$invalid.checks[0].policy.typo = $true
if (Test-Json -Json ($invalid | ConvertTo-Json -Depth 20) -SchemaFile (Join-Path $root 'docs/schemas/plan.schema.json') -ErrorAction SilentlyContinue) {throw 'Schema accepted an unknown policy field'}
$nativeRoot = Join-Path $root ('target/schema-capture-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $nativeRoot | Out-Null
$manifest = @{schema_version='1.0';scenario='schema';subject='cli';steps=@(@{checkpoint='version';executable=$exe;args=@('version');cwd=$nativeRoot;env=@{};stdin='';timeout_ms=5000;max_output_bytes=4096})} | ConvertTo-Json -Depth 20
Assert-Schema $manifest capture
$manifestPath = Join-Path $nativeRoot 'capture.json'
[System.IO.File]::WriteAllText($manifestPath, $manifest)
$bundlePath = Join-Path $nativeRoot 'bundle.json'
$capture = (& $exe capture $manifestPath --allow-exec (Join-Path $root 'target') --output $bundlePath --format json) -join "`n"
if ($LASTEXITCODE -ne 0) {throw "Schema capture failed: $capture"}
Assert-Schema (Get-Content $bundlePath -Raw) bundle
Assert-Schema '{"schema_version":"1.0","tolerance":0,"max_mismatch_fraction":0,"region":null,"assume_srgb":false}' png-policy
Assert-Schema '{"schema_version":"1.0","candidate":"candidate.json","reference":null,"plan":"contracts.json","evidence_dir":"C:/fresh/run"}' scenario
Write-Output 'Bundle, plan, contract, report, error, capture and PNG policy schemas verified.'
exit 0
