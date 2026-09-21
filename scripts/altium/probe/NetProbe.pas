{ NetProbe.pas - one-off probe: can a PcbLib region carry a net?

  Creates a net through the object factory, adds it to the library board and
  assigns it to a copper region, checking every object for nil first: an
  access violation inside the script engine cannot be caught. }

const
    OUT_DIR = 'C:\Users\Public\altium_designer_mcp\probe\';

var
    Report : String;

procedure Note(const S : String);
begin
    if Report <> '' then Report := Report + '; ';
    Report := Report + S;
end;

procedure WriteResponse(const Status : String; const Detail : String);
var
    sl : TStringList;
begin
    sl := TStringList.Create;
    try
        sl.Text := '{"status":"' + Status + '","detail":"' + Detail + '"}';
        if not DirectoryExists(OUT_DIR) then ForceDirectories(OUT_DIR);
        sl.SaveToFile(OUT_DIR + 'probe_response.json');
    finally
        sl.Free;
    end;
end;

procedure Run;
var
    Doc   : IServerDocument;
    Lib   : IPCB_Library;
    DefFP : IPCB_LibComponent;
    Comp  : IPCB_LibComponent;
    Rgn   : IPCB_Region;
    Cont  : IPCB_Contour;
    N     : IPCB_Net;
begin
    Report := '';
    Doc := CreateNewDocumentFromDocumentKind('PCBLIB');
    Lib := PCBServer.GetCurrentPCBLibrary;
    DefFP := Lib.CurrentComponent;

    Comp := PCBServer.CreatePCBLibComp;
    Comp.Name := 'REGION_NET';
    Lib.RegisterComponent(Comp);
    PCBServer.PreProcess;

    Rgn := PCBServer.PCBObjectFactory(eRegionObject, eNoDimension, eCreate_Default);
    Rgn.Layer := eTopLayer;
    Cont := Rgn.MainContour.Replicate;
    Cont.Count := 4;
    Cont.X[1] := MilsToCoord(-60);  Cont.Y[1] := MilsToCoord(-60);
    Cont.X[2] := MilsToCoord( 60);  Cont.Y[2] := MilsToCoord(-60);
    Cont.X[3] := MilsToCoord( 60);  Cont.Y[3] := MilsToCoord( 60);
    Cont.X[4] := MilsToCoord(-60);  Cont.Y[4] := MilsToCoord( 60);
    Rgn.SetOutlineContour(Cont);

    N := PCBServer.PCBObjectFactory(eNetObject, eNoDimension, eCreate_Default);
    if N = nil then
        Note('net factory returned nil')
    else
    begin
        Note('net created');
        N.Name := 'GND';
        if Lib.Board = nil then
            Note('library board is nil')
        else
        begin
            Lib.Board.AddPCBObject(N);
            Note('net added to board');
            Rgn.Net := N;
            if Rgn.Net = nil then Note('region net stayed nil')
            else Note('region net=' + Rgn.Net.Name);
        end;
    end;

    Comp.AddPCBObject(Rgn);
    PCBServer.SendMessageToRobots(Comp.I_ObjectAddress, c_Broadcast,
                                  PCBM_BoardRegisteration, Rgn.I_ObjectAddress);
    PCBServer.PostProcess;

    if DefFP <> nil then
    begin
        Lib.DeRegisterComponent(DefFP);
        Lib.RemoveComponent(DefFP);
    end;
    Lib.Board.ViewManager_FullUpdate;
    Doc.SetModified(True);
    Doc.DoSafeChangeFileNameAndSave(OUT_DIR + 'net.PcbLib', 'PCBLIB');
    WriteResponse('ok', Report);
end;
