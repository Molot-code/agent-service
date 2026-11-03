# check-templates.ps1
$env:Path = "$env:USERPROFILE\.local\bin;$env:Path"

'{}' | Out-File -Encoding utf8 .empty.json

# Проверяем только шаблоны, которые сервер рендерит целиком.
# Партиалы проверяются косвенно — через include из этих файлов.
$roots = @(
    'web\templates\index.html'
    # 'web\templates\base.html'   # раскомментируйте, если base.html рендерится отдельно
)

$fail = $false
foreach ($root in $roots) {
    $out = & minijinja-cli --strict --template-path web\templates $root .empty.json 2>&1
    if ($LASTEXITCODE -ne 0) {
        Write-Host "[FAIL] $root" -ForegroundColor Red
        $out | Where-Object { $_ -match 'error|undefined' } | ForEach-Object {
            Write-Host "   $_" -ForegroundColor Yellow
        }
        $fail = $true
    }
}

Remove-Item .empty.json -ErrorAction SilentlyContinue

if ($fail) { exit 1 }