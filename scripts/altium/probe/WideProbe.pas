{ WideProbe.pas - one-off probe: can a script author text beyond U+00FF?

  Builds strings from character literals (#937) rather than a string literal,
  which reaches Altium as UTF-8 widened through the ANSI page. Each step is
  logged to progress.txt BEFORE it runs, so when the script engine crashes
  (an access violation no try block can catch) the last line names the step.

  Steps, simplest first:
    S1  a BMP character literal in a plain String variable
    S2  the same assigned to a text primitive
    S3  a surrogate pair literal in a String variable
    S4  a body Identifier set to BMP characters
    S5  a body Identifier set to a surrogate pair
    S6  a text primitive set to a surrogate pair }

const
    OUT_DIR = 'C:\Users\Public\altium_designer_mcp\probe\';

var
    Log : TStringList;

procedure Step(const S : String);
begin
    Log.Add(S);
    Log.SaveToFile(OUT_DIR + 'progress.txt');
end;

procedure WriteResponse(const Status : String; const Detail : String);
var
    sl : TStringList;
begin
    sl := TStringList.Create;
    try
        sl.Text := '{"status":"' + Status + '","detail":"' + Detail + '"}';
        sl.SaveToFile(OUT_DIR + 'probe_response.json');
    finally
        sl.Free;
    end;
end;

function NewBody(Comp : IPCB_LibComponent) : IPCB_ComponentBody;
var
    Body : IPCB_ComponentBody;
    Cont : IPCB_Contour;
begin
    Body := PCBServer.PCBObjectFactory(eComponentBodyObject, eNoDimension, eCreate_Default);
    Body.BodyProjection := eBoardSide_Top;
    Body.Layer          := LayerUtils.MechanicalLayer(13);
    Body.StandoffHeight := 0;
    Body.OverallHeight  := MilsToCoord(40);
    Cont := Body.MainContour.Replicate;
    Cont.Count := 4;
    Cont.X[1] := MilsToCoord(-50);  Cont.Y[1] := MilsToCoord(-30);
    Cont.X[2] := MilsToCoord( 50);  Cont.Y[2] := MilsToCoord(-30);
    Cont.X[3] := MilsToCoord( 50);  Cont.Y[3] := MilsToCoord( 30);
    Cont.X[4] := MilsToCoord(-50);  Cont.Y[4] := MilsToCoord( 30);
    Body.SetOutlineContour(Cont);
    Result := Body;
end;

function NewText(Y : Integer) : IPCB_Text;
var
    Txt : IPCB_Text;
begin
    Txt := PCBServer.PCBObjectFactory(eTextObject, eNoDimension, eCreate_Default);
    Txt.XLocation := MilsToCoord(0);
    Txt.YLocation := MilsToCoord(Y);
    Txt.Layer     := eTopOverlay;
    Txt.Size      := MilsToCoord(40);
    Result := Txt;
end;

procedure Place(Comp : IPCB_LibComponent; Obj : IPCB_Primitive);
begin
    Comp.AddPCBObject(Obj);
    PCBServer.SendMessageToRobots(Comp.I_ObjectAddress, c_Broadcast,
                                  PCBM_BoardRegisteration, Obj.I_ObjectAddress);
end;

procedure Run;
var
    Doc   : IServerDocument;
    Lib   : IPCB_Library;
    DefFP : IPCB_LibComponent;
    Comp  : IPCB_LibComponent;
    S     : String;
    Body  : IPCB_ComponentBody;
    Txt   : IPCB_Text;
begin
    if not DirectoryExists(OUT_DIR) then ForceDirectories(OUT_DIR);
    Log := TStringList.Create;
    Step('start');

    Doc := CreateNewDocumentFromDocumentKind('PCBLIB');
    Lib := PCBServer.GetCurrentPCBLibrary;
    DefFP := Lib.CurrentComponent;
    Comp := PCBServer.CreatePCBLibComp;
    Comp.Name := 'WIDE';
    Lib.RegisterComponent(Comp);
    PCBServer.PreProcess;

    Step('S1 BMP literal in a String');
    S := #937;
    Step('S1 ok, length ' + IntToStr(Length(S)) + ', code ' + IntToStr(Ord(S[1])));

    Step('S2 BMP text primitive');
    Txt := NewText(0);
    Txt.Text := #181#937#30005;
    Place(Comp, Txt);
    Step('S2 ok, text length ' + IntToStr(Length(Txt.Text)));

    Step('S3 surrogate pair literal in a String');
    S := #55362#57271;
    Step('S3 ok, length ' + IntToStr(Length(S)));

    Step('S4a body from the factory');
    Body := NewBody(Comp);
    Step('S4b ASCII body Identifier via SetState_Identifier');
    Body.SetState_Identifier('BodyA');
    Step('S4c BMP body Identifier');
    Body.SetState_Identifier(#181#937#30005);
    Step('S4d place the body');
    Place(Comp, Body);
    Step('S4 ok, identifier length ' + IntToStr(Length(Body.GetState_Identifier)));

    Step('S5 surrogate body Identifier');
    Body := NewBody(Comp);
    Body.SetState_Identifier(#55362#57271);
    Place(Comp, Body);
    Step('S5 ok, identifier length ' + IntToStr(Length(Body.GetState_Identifier)));

    Step('S6 surrogate text primitive');
    Txt := NewText(100);
    Txt.Text := #55362#57271;
    Place(Comp, Txt);
    Step('S6 ok, text length ' + IntToStr(Length(Txt.Text)));

    PCBServer.PostProcess;
    Step('save');
    if DefFP <> nil then
    begin
        Lib.DeRegisterComponent(DefFP);
        Lib.RemoveComponent(DefFP);
    end;
    Lib.Board.ViewManager_FullUpdate;
    Doc.SetModified(True);
    Doc.DoSafeChangeFileNameAndSave(OUT_DIR + 'wide.PcbLib', 'PCBLIB');
    Step('saved');
    WriteResponse('ok', 'all steps ran');
end;
