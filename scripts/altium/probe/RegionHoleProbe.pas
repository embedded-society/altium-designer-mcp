{ RegionHoleProbe.pas — one-off probe: can a script author a PcbLib region with a hole?

  Approach B: build the outer contour the proven way (MainContour.Replicate ->
  Count -> X[i]/Y[i] -> SetOutlineContour), then add a second contour to the
  region's GeometricPolygon through AddContourIsHole(Contour, True). Saves a
  throwaway library; never touches the committed samples. }

const
    OUT_DIR = 'C:\Users\Public\altium_designer_mcp\probe\';

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
    Hole  : IPCB_Contour;
    Stage : String;
begin
    Stage := 'start';
    try
        Doc := CreateNewDocumentFromDocumentKind('PCBLIB');
        Lib := PCBServer.GetCurrentPCBLibrary;
        DefFP := Lib.CurrentComponent;

        Comp := PCBServer.CreatePCBLibComp;
        Comp.Name := 'REGION_HOLE';
        Lib.RegisterComponent(Comp);
        PCBServer.PreProcess;

        Stage := 'outline';
        Rgn := PCBServer.PCBObjectFactory(eRegionObject, eNoDimension, eCreate_Default);
        Rgn.Layer := eTopLayer;
        Cont := Rgn.MainContour.Replicate;
        Cont.Count := 4;
        Cont.X[1] := MilsToCoord(-100);  Cont.Y[1] := MilsToCoord(-100);
        Cont.X[2] := MilsToCoord( 100);  Cont.Y[2] := MilsToCoord(-100);
        Cont.X[3] := MilsToCoord( 100);  Cont.Y[3] := MilsToCoord( 100);
        Cont.X[4] := MilsToCoord(-100);  Cont.Y[4] := MilsToCoord( 100);
        Rgn.SetOutlineContour(Cont);

        Stage := 'hole';
        Hole := Rgn.MainContour.Replicate;
        Hole.Count := 4;
        Hole.X[1] := MilsToCoord(-40);  Hole.Y[1] := MilsToCoord(-40);
        Hole.X[2] := MilsToCoord( 40);  Hole.Y[2] := MilsToCoord(-40);
        Hole.X[3] := MilsToCoord( 40);  Hole.Y[3] := MilsToCoord( 40);
        Hole.X[4] := MilsToCoord(-40);  Hole.Y[4] := MilsToCoord( 40);
        Rgn.GeometricPolygon.AddContourIsHole(Hole, True);

        Stage := 'add';
        Comp.AddPCBObject(Rgn);
        PCBServer.SendMessageToRobots(Comp.I_ObjectAddress, c_Broadcast,
                                      PCBM_BoardRegisteration, Rgn.I_ObjectAddress);
        PCBServer.PostProcess;

        Stage := 'save';
        if DefFP <> nil then
        begin
            Lib.DeRegisterComponent(DefFP);
            Lib.RemoveComponent(DefFP);
        end;
        Lib.Board.ViewManager_FullUpdate;
        Doc.SetModified(True);
        Doc.DoSafeChangeFileNameAndSave(OUT_DIR + 'region_hole.PcbLib', 'PCBLIB');
        WriteResponse('ok', 'holes=' + IntToStr(Rgn.HoleCount));
    except
        WriteResponse('error', 'exception at stage ' + Stage);
    end;
end;
