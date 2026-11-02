# 1. Проверка прав администратора
$isAdmin = ([Security.Principal.WindowsPrincipal] [Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $isAdmin) {
    Write-Host "ОШИБКА: Скрипт запущен без прав администратора!" -ForegroundColor Red
    Write-Host "Нажмите любую клавишу для выхода..."
    $null = $Host.UI.RawUI.ReadKey("NoEcho,IncludeKeyDown")
    exit 1
}

# 2. Переход в папку проекта
Set-Location -Path "C:\Users\molot\Desktop\agent-service"

# 3. Запуск задачи и обработка результата
Write-Host "Запуск task dev-watch..." -ForegroundColor Cyan
try {
    # Запускаем task
    task dev-watch
    
    # Если task завершился сам (что странно для watch), пишем об этом
    Write-Host "`n[ВНИМАНИЕ] task dev-watch завершился сам по себе." -ForegroundColor Yellow
} catch {
    Write-Host "`n[ОШИБКА] Не удалось запустить task: $_" -ForegroundColor Red
}

# 4. Пауза в конце, чтобы окно не закрылось
Write-Host "`nНажмите любую клавишу для выхода..."
$null = $Host.UI.RawUI.ReadKey("NoEcho,IncludeKeyDown")