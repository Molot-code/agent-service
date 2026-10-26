#define MyAppName "Parental Control"
#define MyAppVersion "1.0"
#define MyAppPublisher "Your Company"
#define MyAppExeName "ParentalControlSvc.exe"
; Точное системное имя из вашего файла config.toml ("ParentalControlSvc")
#define MyServiceSystemName "ParentalControlSvc"

[Setup]
AppId={{A1B2C3D4-E5F6-7890-ABCD-EF1234567890}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher={#MyAppPublisher}
DefaultDirName={autopf}\ParentalControl
DefaultGroupName={#MyAppName}
OutputDir=.\output
OutputBaseFilename=ParentalControlSetup
Compression=lzma
SolidCompression=yes
WizardStyle=modern
PrivilegesRequired=admin
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
UsePreviousAppDir=yes

[Languages]
Name: russian; MessagesFile: "compiler:Languages\Russian.isl"

[Files]
; 1. Скомпилированные бинарники
Source: "..\target\release\ParentalControlSvc.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\target\release\agent-cli.exe";          DestDir: "{app}"; Flags: ignoreversion
Source: "..\target\release\agent-tray.exe";         DestDir: "{app}"; Flags: ignoreversion

; 2. Конфиг
Source: "..\config.toml"; DestDir: "{app}"; Flags: ignoreversion

; 3. Статика и шаблоны
Source: "..\static\*";    DestDir: "{app}\static";    Flags: ignoreversion recursesubdirs createallsubdirs
Source: "..\templates\*"; DestDir: "{app}\templates"; Flags: recursesubdirs createallsubdirs

[Dirs]
; Создаём папку в ProgramData с полными правами для юзеров
Name: "{commonappdata}\ParentalControl"; Permissions: users-full

[Run]
; --- ОЧИСТКА СТАРОЙ ВЕРСИИ ---
Filename: "sc.exe"; Parameters: "stop ""{#MyServiceSystemName}""";   Flags: runhidden waituntilterminated
Filename: "sc.exe"; Parameters: "delete ""{#MyServiceSystemName}"""; Flags: runhidden waituntilterminated

; --- УСТАНОВКА, НАСТРОЙКА, ЗАПУСК ---
; 1. Брандмауэр
Filename: "netsh"; Parameters: "advfirewall firewall add rule name=""{#MyAppName} API"" dir=in action=allow program=""{app}\{#MyAppExeName}"" enable=yes"; Flags: runhidden waituntilterminated
Filename: "netsh"; Parameters: "advfirewall firewall add rule name=""{#MyAppName} Discovery UDP"" dir=in action=allow protocol=UDP localport=9090 enable=yes"; Flags: runhidden waituntilterminated
Filename: "netsh"; Parameters: "advfirewall firewall add rule name=""{#MyAppName} API TCP"" dir=in action=allow protocol=TCP localport=8765 enable=yes"; Flags: runhidden waituntilterminated

; 2. Регистрация службы
Filename: "{app}\{#MyAppExeName}"; Parameters: "install"; WorkingDir: "{app}"; Flags: runhidden waituntilterminated; StatusMsg: "Регистрация службы в системе..."

; 3. Автозапуск
Filename: "sc.exe"; Parameters: "config ""{#MyServiceSystemName}"" start= auto"; Flags: runhidden waituntilterminated

; 4. Описание
Filename: "sc.exe"; Parameters: "description ""{#MyServiceSystemName}"" ""Служба родительского контроля"""; Flags: runhidden waituntilterminated

; 5. Старт
Filename: "sc.exe"; Parameters: "start ""{#MyServiceSystemName}"""; Flags: runhidden waituntilterminated; StatusMsg: "Запуск службы..."

; 6. Открыть админку
Filename: "http://localhost:8765/"; Flags: shellexec postinstall nowait runasoriginaluser; Description: "Открыть панель управления (админку)"

[UninstallRun]
Filename: "sc.exe"; Parameters: "stop ""{#MyServiceSystemName}"""; Flags: runhidden waituntilterminated; RunOnceId: "StopAgentService"
Filename: "{app}\{#MyAppExeName}"; Parameters: "uninstall"; WorkingDir: "{app}"; Flags: runhidden waituntilterminated; RunOnceId: "UninstallAgentService"
Filename: "netsh"; Parameters: "advfirewall firewall delete rule name=""{#MyAppName} API""";             Flags: runhidden waituntilterminated; RunOnceId: "DeleteFirewallRuleAPI"
Filename: "netsh"; Parameters: "advfirewall firewall delete rule name=""{#MyAppName} Discovery UDP""";   Flags: runhidden waituntilterminated; RunOnceId: "DeleteFirewallRuleDiscovery"
Filename: "netsh"; Parameters: "advfirewall firewall delete rule name=""{#MyAppName} API TCP""";         Flags: runhidden waituntilterminated; RunOnceId: "DeleteFirewallRuleTCP"

[Code]
function InitializeUninstall(): Boolean;
var
  Form: TSetupForm;
  OKButton, CancelButton: TButton;
  PwdEdit: TEdit;
  ResultCode: Integer;
  ExePath: String;
  Attempts: Integer;
begin
  Result := False;
  ExePath := ExpandConstant('{app}\agent-cli.exe');

  if not FileExists(ExePath) then
  begin
    Result := True;
    Exit;
  end;

  for Attempts := 1 to 3 do
  begin
    Form := CreateCustomForm(0, 0, False, False);
    
    try
      Form.ClientWidth := ScaleX(320);
      Form.ClientHeight := ScaleY(140);
      Form.Caption := 'Подтверждение удаления';
      Form.BorderIcons := [biSystemMenu];
      Form.BorderStyle := bsDialog;
      Form.Position := poScreenCenter;

      with TNewStaticText.Create(Form) do
      begin
        Parent := Form;
        Left := ScaleX(10);
        Top := ScaleY(10);
        Width := ScaleX(300);
        Caption := 'Для удаления программы введите PIN-код родителя:';
      end;

      PwdEdit := TEdit.Create(Form);
      PwdEdit.Parent := Form;
      PwdEdit.Width := ScaleX(300);
      PwdEdit.Left := ScaleX(10);
      PwdEdit.Top := ScaleY(40);
      PwdEdit.PasswordChar := '*';

      OKButton := TButton.Create(Form);
      OKButton.Parent := Form;
      OKButton.Width := ScaleX(90);
      OKButton.Height := ScaleY(28);
      OKButton.Left := Form.ClientWidth - ScaleX(200);
      OKButton.Top := Form.ClientHeight - ScaleY(40);
      OKButton.Caption := 'OK';
      OKButton.ModalResult := mrOk;
      OKButton.Default := True;

      CancelButton := TButton.Create(Form);
      CancelButton.Parent := Form;
      CancelButton.Width := ScaleX(90);
      CancelButton.Height := ScaleY(28);
      CancelButton.Left := Form.ClientWidth - ScaleX(100);
      CancelButton.Top := Form.ClientHeight - ScaleY(40);
      CancelButton.Caption := 'Отмена';
      CancelButton.ModalResult := mrCancel;
      CancelButton.Cancel := True;

      Form.ActiveControl := PwdEdit;

      if Form.ShowModal <> mrOk then
      begin
        MsgBox('Удаление отменено.', mbInformation, MB_OK);
        Exit;
      end;

      if Exec(ExePath, 'check-pin ' + PwdEdit.Text, '', SW_HIDE, ewWaitUntilTerminated, ResultCode) then
      begin
        if ResultCode = 0 then
        begin
          Result := True;
          Exit;
        end
        else if ResultCode = 1 then
        begin
          if Attempts < 3 then
            MsgBox('Неверный PIN-код. Осталось попыток: ' + IntToStr(3 - Attempts), mbError, MB_OK)
          else
            MsgBox('Неверный PIN-код. Удаление запрещено.', mbError, MB_OK);
        end
        else
        begin
          MsgBox('Не удалось проверить PIN (код ' + IntToStr(ResultCode) + '). Удаление будет продолжено.', mbInformation, MB_OK);
          Result := True;
          Exit;
        end;
      end
      else
      begin
        MsgBox('Не удалось запустить проверку PIN. Удаление будет продолжено.', mbInformation, MB_OK);
        Result := True;
        Exit;
      end;
    finally
      Form.Free;
    end;
  end;

  Result := False;
end;