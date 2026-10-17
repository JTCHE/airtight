# Installs the Airtight virtual speaker. Run as administrator, after test
# signing is on:  Secure Boot off in the UEFI setup, then
#   bcdedit /set testsigning on   and a restart.
$ErrorActionPreference = 'Stop'
$out = Join-Path $PSScriptRoot 'out'
if (-not ((bcdedit /enum '{current}') -match 'testsigning\s+Yes')) {
    Write-Host 'Test signing is off. Turn Secure Boot off, run "bcdedit /set testsigning on", restart, then run this again.'
    exit 1
}
$cer = Join-Path $out 'airtight-test.cer'
Import-Certificate -FilePath $cer -CertStoreLocation Cert:\LocalMachine\Root | Out-Null
Import-Certificate -FilePath $cer -CertStoreLocation Cert:\LocalMachine\TrustedPublisher | Out-Null
$devcon = 'C:\Program Files (x86)\Windows Kits\10\Tools\10.0.26100.0\x64\devcon.exe'
$inf = Join-Path $out 'airtightaudio.inf'
if ((& $devcon hwids 'ROOT\AirtightAudio') -match 'AirtightAudio') {
    & $devcon update $inf 'ROOT\AirtightAudio'
} else {
    & $devcon install $inf 'ROOT\AirtightAudio'
}
exit $LASTEXITCODE
