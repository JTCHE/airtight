# Test-signs the driver and its catalog with a self-made certificate in the
# current user's store. Windows loads it only in test-signing mode.
param([string]$Out)
$ErrorActionPreference = 'Stop'
$subject = 'CN=Airtight Test Signing'
$cert = Get-ChildItem Cert:\CurrentUser\My | Where-Object Subject -eq $subject | Select-Object -First 1
if (-not $cert) {
    $cert = New-SelfSignedCertificate -Subject $subject -Type CodeSigningCert -CertStoreLocation Cert:\CurrentUser\My -NotAfter (Get-Date).AddYears(10)
}
Export-Certificate -Cert $cert -FilePath (Join-Path $Out 'airtight-test.cer') | Out-Null
$signtool = 'C:\Program Files (x86)\Windows Kits\10\bin\10.0.26100.0\x64\signtool.exe'
& $signtool sign /q /fd sha256 /sha1 $cert.Thumbprint (Join-Path $Out 'airtightaudio.sys') (Join-Path $Out 'airtightaudio.cat')
if ($LASTEXITCODE) { exit $LASTEXITCODE }
