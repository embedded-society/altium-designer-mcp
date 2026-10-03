{ GenerateSamples.pas — on-site sample-library authoring for altium-designer-mcp.

  Drives a real Altium Designer to AUTHOR reference .PcbLib / .SchLib libraries with a
  known set of primitives, then saves them to the bridge directory. The Rust reader and
  round-trip tests validate against these genuine-Altium files (the ground truth that
  the pyaltiumlib oracle only approximates). Run via the ..\..\Generate-Samples.ps1
  wrapper, which launches this through Altium's RunScript CLI and then moves the saved
  libraries into scripts/samples/.

  The RunScript launch mechanism and the file-based response bridge are adapted from
  coffeenmusic/altium-mcp (MIT) — https://github.com/coffeenmusic/altium-mcp

  On-site only: needs Altium Designer installed (developed against AD24). NEVER CI.

  ITERATIVE BY DESIGN: the primitive set below is a SEED. The intended loop is
  generate -> read the sample back with the Rust tests -> add the next feature /
  fix the placement -> regenerate, until coverage is complete. The Altium scripting
  API calls here are a first pass (v0) and are expected to need adjustment against a
  live AD24 — that is the point of running it on-site. Keep one library per feature
  area (mirroring AltiumSharp's TestData layout) so a failing read pinpoints the
  feature. }

const
    OUT_DIR = 'C:\Users\Public\altium_designer_mcp\samples\';

// Writes a one-line JSON status the wrapper polls for.
procedure WriteResponse(const Status : String; const Detail : String);
var
    sl : TStringList;
begin
    sl := TStringList.Create;
    try
        sl.Text := '{"status":"' + Status + '","detail":"' + Detail + '"}';
        if not DirectoryExists(OUT_DIR) then ForceDirectories(OUT_DIR);
        sl.SaveToFile(OUT_DIR + 'generate_response.json');
    finally
        sl.Free;
    end;
end;

{ Adds one SMD pad to a footprint at (X, 0) mils with the given TShape, size and name.
  Mode := ePadMode_Simple + HoleSize := 0 make it a true single-layer SMD pad — the v0
  left the factory's default hole, so it read back as a through-hole pad. Mirrors
  UltraLibrarian's verified pad flow incl. the board-registration broadcast. }
procedure AddPad(Comp : IPCB_LibComponent; X : Integer; PadShape : TShape;
                 W : Integer; H : Integer; Nm : String);
var
    Pad : IPCB_Pad;
begin
    Pad := PCBServer.PCBObjectFactory(ePadObject, eNoDimension, eCreate_Default);
    if Pad = nil then Exit;
    Pad.Name     := Nm;
    Pad.X        := MilsToCoord(X);
    Pad.Y        := MilsToCoord(0);
    Pad.Mode     := ePadMode_Simple;   // single-layer SMD pad (empty size/shape block)
    Pad.Layer    := eTopLayer;
    Pad.HoleSize := 0;                 // true SMD: no hole
    Pad.TopShape := PadShape;
    Pad.TopXSize := MilsToCoord(W);
    Pad.TopYSize := MilsToCoord(H);
    Comp.AddPCBObject(Pad);
    // Altium's own constant is spelled PCBM_BoardRegisteration (the typo is real).
    PCBServer.SendMessageToRobots(Comp.I_ObjectAddress, c_Broadcast,
                                  PCBM_BoardRegisteration, Pad.I_ObjectAddress);
end;

{ Like AddPad but with explicit X, Y, rotation and shape — for boundary-case fixtures the
  clean MAIN samples don't reach (rotated pad, negative/large coords). Pad.Rotation is in
  DEGREES (a plain number, NO MilsToCoord — same as the arc angles in AddArc). }
procedure AddPadFull(Comp : IPCB_LibComponent; X : Integer; Y : Integer; Rot : Integer;
                     PadShape : TShape; W : Integer; H : Integer; Nm : String);
var
    Pad : IPCB_Pad;
begin
    Pad := PCBServer.PCBObjectFactory(ePadObject, eNoDimension, eCreate_Default);
    if Pad = nil then Exit;
    Pad.Name     := Nm;
    Pad.X        := MilsToCoord(X);
    Pad.Y        := MilsToCoord(Y);
    Pad.Rotation := Rot;
    Pad.Mode     := ePadMode_Simple;
    Pad.Layer    := eTopLayer;
    Pad.HoleSize := 0;
    Pad.TopShape := PadShape;
    Pad.TopXSize := MilsToCoord(W);
    Pad.TopYSize := MilsToCoord(H);
    Comp.AddPCBObject(Pad);
    PCBServer.SendMessageToRobots(Comp.I_ObjectAddress, c_Broadcast,
                                  PCBM_BoardRegisteration, Pad.I_ObjectAddress);
end;

{ Adds one through-hole pad at (X, 0) mils with the given hole shape. TH pads sit on
  eMultiLayer with HoleSize > 0; a non-round hole (square/slot) makes Altium emit the
  651-byte size/shape block. Slots also take a HoleWidth (the secondary dimension). }
procedure AddThPad(Comp : IPCB_LibComponent; X : Integer; Hole : THoleType;
                   HoleLen : Integer; HoleWid : Integer; Nm : String);
var
    Pad : IPCB_Pad;
begin
    Pad := PCBServer.PCBObjectFactory(ePadObject, eNoDimension, eCreate_Default);
    if Pad = nil then Exit;
    Pad.Name     := Nm;
    Pad.X        := MilsToCoord(X);
    Pad.Y        := MilsToCoord(0);
    Pad.Mode     := ePadMode_Simple;   // same shape on all layers
    Pad.Layer    := eMultiLayer;       // through-hole: spans all copper
    Pad.TopShape := eRounded;
    Pad.TopXSize := MilsToCoord(70);
    Pad.TopYSize := MilsToCoord(70);
    Pad.HoleType := Hole;              // eRoundHole / eSquareHole / eSlotHole
    Pad.HoleSize := MilsToCoord(HoleLen);
    if Hole = eSlotHole then
    begin
        Pad.HoleWidth    := MilsToCoord(HoleWid);
        Pad.HoleRotation := 0;
    end;
    Comp.AddPCBObject(Pad);
    PCBServer.SendMessageToRobots(Comp.I_ObjectAddress, c_Broadcast,
                                  PCBM_BoardRegisteration, Pad.I_ObjectAddress);
end;

{ A multi-layer (LocalStack) through-hole pad: top/mid/bottom shapes+sizes differ.
  ePadMode_LocalStack unlocks the Top/Mid/Bot triplet (the single mid applies to all
  internal layers). Verified via CreatePCBObjects.PAS PlaceATopMidBotStackPad. }
procedure AddThStackPad(Comp : IPCB_LibComponent; X : Integer; Nm : String);
var
    Pad : IPCB_Pad;
begin
    Pad := PCBServer.PCBObjectFactory(ePadObject, eNoDimension, eCreate_Default);
    if Pad = nil then Exit;
    Pad.Name     := Nm;
    Pad.X        := MilsToCoord(X);
    Pad.Y        := MilsToCoord(0);
    Pad.Layer    := eMultiLayer;          // through-hole: spans all copper
    Pad.HoleSize := MilsToCoord(30);      // round hole (HoleType left default)
    Pad.Mode     := ePadMode_LocalStack;  // top / mid / bottom independent
    Pad.TopShape := eRounded;      Pad.TopXSize := MilsToCoord(70);  Pad.TopYSize := MilsToCoord(70);
    Pad.MidShape := eRounded;      Pad.MidXSize := MilsToCoord(60);  Pad.MidYSize := MilsToCoord(60);
    Pad.BotShape := eRectangular;  Pad.BotXSize := MilsToCoord(50);  Pad.BotYSize := MilsToCoord(50);
    Comp.AddPCBObject(Pad);
    PCBServer.SendMessageToRobots(Comp.I_ObjectAddress, c_Broadcast,
                                  PCBM_BoardRegisteration, Pad.I_ObjectAddress);
end;

{ Adds one track (X1,Y1)->(X2,Y2) mils, width (mils), on Lay. Verified via UL FP_AddLine. }
procedure AddTrack(Comp : IPCB_LibComponent; X1 : Integer; Y1 : Integer;
                   X2 : Integer; Y2 : Integer; W : Integer; Lay : TLayer);
var
    Trk : IPCB_Track;
begin
    Trk := PCBServer.PCBObjectFactory(eTrackObject, eNoDimension, eCreate_Default);
    if Trk = nil then Exit;
    Trk.X1    := MilsToCoord(X1);
    Trk.Y1    := MilsToCoord(Y1);
    Trk.X2    := MilsToCoord(X2);
    Trk.Y2    := MilsToCoord(Y2);
    Trk.Width := MilsToCoord(W);
    Trk.Layer := Lay;
    Comp.AddPCBObject(Trk);
    PCBServer.SendMessageToRobots(Comp.I_ObjectAddress, c_Broadcast,
                                  PCBM_BoardRegisteration, Trk.I_ObjectAddress);
end;

{ Adds one arc centred (XC,YC) mils, radius/width (mils), start/end angles in degrees
  (CCW from +X; full circle = 0..360). Verified via UL FP_AddArc: the width property is
  LineWidth (NOT Width), and the angles take NO MilsToCoord wrapper. }
procedure AddArc(Comp : IPCB_LibComponent; XC : Integer; YC : Integer; Radius : Integer;
                 StartAngle : Double; EndAngle : Double; W : Integer; Lay : TLayer);
var
    Arc : IPCB_Arc;
begin
    Arc := PCBServer.PCBObjectFactory(eArcObject, eNoDimension, eCreate_Default);
    if Arc = nil then Exit;
    Arc.XCenter    := MilsToCoord(XC);
    Arc.YCenter    := MilsToCoord(YC);
    Arc.Radius     := MilsToCoord(Radius);
    Arc.LineWidth  := MilsToCoord(W);
    Arc.StartAngle := StartAngle;
    Arc.EndAngle   := EndAngle;
    Arc.Layer      := Lay;
    Comp.AddPCBObject(Arc);
    PCBServer.SendMessageToRobots(Comp.I_ObjectAddress, c_Broadcast,
                                  PCBM_BoardRegisteration, Arc.I_ObjectAddress);
end;

{ Adds a filled rectangular region with corners (X1,Y1)-(X2,Y2) in mils, on Lyr.
  Contour API verbatim from UL FP_AddPoly: MainContour.Replicate -> Count -> 1-based
  X[i]/Y[i] -> SetOutlineContour (Altium auto-closes). A 4-vertex box keeps the
  authoring free of array literals (unverified in DelphiScript); polygons come later. }
procedure AddRegionBox(Comp : IPCB_LibComponent; X1 : Integer; Y1 : Integer;
                       X2 : Integer; Y2 : Integer; Lyr : TLayer);
var
    Rgn  : IPCB_Region;
    Cont : IPCB_Contour;
begin
    Rgn := PCBServer.PCBObjectFactory(eRegionObject, eNoDimension, eCreate_Default);
    if Rgn = nil then Exit;
    Cont := Rgn.MainContour.Replicate;
    Rgn.Layer := Lyr;
    Cont.Count := 4;
    Cont.X[1] := MilsToCoord(X1);  Cont.Y[1] := MilsToCoord(Y1);
    Cont.X[2] := MilsToCoord(X2);  Cont.Y[2] := MilsToCoord(Y1);
    Cont.X[3] := MilsToCoord(X2);  Cont.Y[3] := MilsToCoord(Y2);
    Cont.X[4] := MilsToCoord(X1);  Cont.Y[4] := MilsToCoord(Y2);
    Rgn.SetOutlineContour(Cont);
    Comp.AddPCBObject(Rgn);
    PCBServer.SendMessageToRobots(Comp.I_ObjectAddress, c_Broadcast,
                                  PCBM_BoardRegisteration, Rgn.I_ObjectAddress);
end;

{ Adds one stroke-font text. X,Y,Height in mils; Rot in degrees; Content is Windows-1252.
  Factory default = stroke font. Verified via UL FP_AddText: .Size IS the text height. }
procedure AddText(Comp : IPCB_LibComponent; X : Integer; Y : Integer; Content : String;
                  Height : Integer; Rot : Double; Lyr : TLayer);
var
    Txt : IPCB_Text;
begin
    Txt := PCBServer.PCBObjectFactory(eTextObject, eNoDimension, eCreate_Default);
    if Txt = nil then Exit;
    Txt.XLocation := MilsToCoord(X);
    Txt.YLocation := MilsToCoord(Y);
    Txt.Layer     := Lyr;
    Txt.Size      := MilsToCoord(Height);
    Txt.Rotation  := Rot;
    Txt.Text      := Content;
    Comp.AddPCBObject(Txt);
    PCBServer.SendMessageToRobots(Comp.I_ObjectAddress, c_Broadcast,
                                  PCBM_BoardRegisteration, Txt.I_ObjectAddress);
end;

{ Adds one simple through-via at (X,Y) mils: PadDia is the outer pad diameter, HoleDia the
  drill, both mils. LowLayer/HighLayer span Top->Bottom (a plain through-via). Verified
  against the COM type library: factory eViaObject; X/Y/Size/HoleSize/LowLayer/HighLayer/
  Mode; ePadMode_Simple is the same proven constant AddPad uses. }
procedure AddVia(Comp : IPCB_LibComponent; X : Integer; Y : Integer; PadDia : Integer; HoleDia : Integer);
var
    Via : IPCB_Via;
begin
    Via := PCBServer.PCBObjectFactory(eViaObject, eNoDimension, eCreate_Default);
    if Via = nil then Exit;
    Via.X         := MilsToCoord(X);
    Via.Y         := MilsToCoord(Y);
    Via.Size      := MilsToCoord(PadDia);
    Via.HoleSize  := MilsToCoord(HoleDia);
    Via.LowLayer  := eTopLayer;
    Via.HighLayer := eBottomLayer;
    Via.Mode      := ePadMode_Simple;
    Comp.AddPCBObject(Via);
    PCBServer.SendMessageToRobots(Comp.I_ObjectAddress, c_Broadcast,
                                  PCBM_BoardRegisteration, Via.I_ObjectAddress);
end;

{ Adds one solid copper fill (filled rectangle) with corners (X1,Y1)-(X2,Y2) mils on ALayer,
  rotated Rot degrees about its centre. Verified against the COM type library: factory
  eFillObject; the corner props are X1Location/Y1Location/X2Location/Y2Location (NOT
  X1/Y1/X2/Y2); Layer; Rotation is a number in degrees. }
procedure AddFill(Comp : IPCB_LibComponent; X1 : Integer; Y1 : Integer; X2 : Integer; Y2 : Integer;
                  ALayer : TLayer; Rot : Integer);
var
    Fill : IPCB_Fill;
begin
    Fill := PCBServer.PCBObjectFactory(eFillObject, eNoDimension, eCreate_Default);
    if Fill = nil then Exit;
    Fill.X1Location := MilsToCoord(X1);
    Fill.Y1Location := MilsToCoord(Y1);
    Fill.X2Location := MilsToCoord(X2);
    Fill.Y2Location := MilsToCoord(Y2);
    Fill.Layer      := ALayer;
    Fill.Rotation   := Rot;
    Comp.AddPCBObject(Fill);
    PCBServer.SendMessageToRobots(Comp.I_ObjectAddress, c_Broadcast,
                                  PCBM_BoardRegisteration, Fill.I_ObjectAddress);
end;

{ A simple extruded 3D ComponentBody: a rectangular WMils x HMils outline centred at
  (CX,CY) mils, extruded from the board (standoff 0) to OverallMils height. Outline via
  ShapeSegments + UpdateContourFromShape (the from-scratch route proven in MakeRegionShapes
  AddExtrudedBody2). A body lives on a MECHANICAL layer, never copper. }
procedure AddExtrudedBox(Comp : IPCB_LibComponent; CX : Integer; CY : Integer;
                         WMils : Integer; HMils : Integer; OverallMils : Integer);
var
    Body  : IPCB_ComponentBody;
    Cont  : IPCB_Contour;
    HalfW : Integer;
    HalfH : Integer;
begin
    HalfW := WMils div 2;
    HalfH := HMils div 2;
    Body := PCBServer.PCBObjectFactory(eComponentBodyObject, eNoDimension, eCreate_Default);
    if Body = nil then Exit;
    Body.BodyProjection := eBoardSide_Top;
    Body.Layer          := LayerUtils.MechanicalLayer(13);
    Body.StandoffHeight := 0;
    Body.OverallHeight  := MilsToCoord(OverallMils);
    // Outline via the IPCB_Contour vertex API (1-based) — the same proven path AddRegionBox
    // uses; avoids ShapeSegments/TPolySegment (TPolySegment.Kind is undeclared in AD24).
    Cont := Body.MainContour.Replicate;
    Cont.Count := 4;
    Cont.X[1] := MilsToCoord(CX - HalfW);  Cont.Y[1] := MilsToCoord(CY - HalfH);
    Cont.X[2] := MilsToCoord(CX + HalfW);  Cont.Y[2] := MilsToCoord(CY - HalfH);
    Cont.X[3] := MilsToCoord(CX + HalfW);  Cont.Y[3] := MilsToCoord(CY + HalfH);
    Cont.X[4] := MilsToCoord(CX - HalfW);  Cont.Y[4] := MilsToCoord(CY + HalfH);
    Body.SetOutlineContour(Cont);
    Comp.AddPCBObject(Body);
    PCBServer.SendMessageToRobots(Comp.I_ObjectAddress, c_Broadcast,
                                  PCBM_BoardRegisteration, Body.I_ObjectAddress);
end;

{ Extruded body whose outline is OFF-GRID by raw internal units (1 unit =
  0.0001 mil): AD24 keeps the exact units through save (2026-09-01 enrich
  probe), so the golden pins the reader's full outline precision. Same proven
  contour route as AddExtrudedBox. }
procedure AddExtrudedBoxOffGrid(Comp : IPCB_LibComponent);
var
    Body : IPCB_ComponentBody;
    Cont : IPCB_Contour;
begin
    Body := PCBServer.PCBObjectFactory(eComponentBodyObject, eNoDimension, eCreate_Default);
    if Body = nil then Exit;
    Body.BodyProjection := eBoardSide_Top;
    Body.Layer          := LayerUtils.MechanicalLayer(13);
    Body.StandoffHeight := 0;
    Body.OverallHeight  := MilsToCoord(40);
    Cont := Body.MainContour.Replicate;
    Cont.Count := 4;
    Cont.X[1] := MilsToCoord(-50) + 1;   Cont.Y[1] := MilsToCoord(-25) + 3;
    Cont.X[2] := MilsToCoord(50) + 7;    Cont.Y[2] := MilsToCoord(-25) - 1;
    Cont.X[3] := MilsToCoord(50) - 3;    Cont.Y[3] := MilsToCoord(25) + 9;
    Cont.X[4] := MilsToCoord(-50) - 9;   Cont.Y[4] := MilsToCoord(25) - 7;
    Body.SetOutlineContour(Cont);
    Comp.AddPCBObject(Body);
    PCBServer.SendMessageToRobots(Comp.I_ObjectAddress, c_Broadcast,
                                  PCBM_BoardRegisteration, Body.I_ObjectAddress);
end;

{ ==== PcbLib COVERAGE-ENRICHMENT HELPERS (verified AD24 names) ============== }

{ SMD pad on an explicit layer (batch 5): the only pad helper whose layer is a
  parameter, for a pad on a mechanical layer past the legacy sixteen. }
procedure AddPadOnLayer(Comp : IPCB_LibComponent; X : Integer; Y : Integer;
                        Nm : String; Lay : TLayer);
var
    Pad : IPCB_Pad;
begin
    Pad := PCBServer.PCBObjectFactory(ePadObject, eNoDimension, eCreate_Default);
    if Pad = nil then Exit;
    Pad.Name     := Nm;
    Pad.X        := MilsToCoord(X);
    Pad.Y        := MilsToCoord(Y);
    Pad.Mode     := ePadMode_Simple;
    Pad.Layer    := Lay;
    Pad.HoleSize := 0;
    Pad.TopShape := eRectangular;
    Pad.TopXSize := MilsToCoord(40);
    Pad.TopYSize := MilsToCoord(40);
    Comp.AddPCBObject(Pad);
    PCBServer.SendMessageToRobots(Comp.I_ObjectAddress, c_Broadcast,
                                  PCBM_BoardRegisteration, Pad.I_ObjectAddress);
end;

{ Extruded body on an explicit layer (batch 5), otherwise AddExtrudedBox. }
procedure AddExtrudedBoxOnLayer(Comp : IPCB_LibComponent; CX : Integer; CY : Integer;
                                WMils : Integer; HMils : Integer; OverallMils : Integer;
                                Lay : TLayer);
var
    Body  : IPCB_ComponentBody;
    Cont  : IPCB_Contour;
    HalfW : Integer;
    HalfH : Integer;
begin
    HalfW := WMils div 2;
    HalfH := HMils div 2;
    Body := PCBServer.PCBObjectFactory(eComponentBodyObject, eNoDimension, eCreate_Default);
    if Body = nil then Exit;
    Body.BodyProjection := eBoardSide_Top;
    Body.Layer          := Lay;
    Body.StandoffHeight := 0;
    Body.OverallHeight  := MilsToCoord(OverallMils);
    Cont := Body.MainContour.Replicate;
    Cont.Count := 4;
    Cont.X[1] := MilsToCoord(CX - HalfW);  Cont.Y[1] := MilsToCoord(CY - HalfH);
    Cont.X[2] := MilsToCoord(CX + HalfW);  Cont.Y[2] := MilsToCoord(CY - HalfH);
    Cont.X[3] := MilsToCoord(CX + HalfW);  Cont.Y[3] := MilsToCoord(CY + HalfH);
    Cont.X[4] := MilsToCoord(CX - HalfW);  Cont.Y[4] := MilsToCoord(CY + HalfH);
    Body.SetOutlineContour(Cont);
    Comp.AddPCBObject(Body);
    PCBServer.SendMessageToRobots(Comp.I_ObjectAddress, c_Broadcast,
                                  PCBM_BoardRegisteration, Body.I_ObjectAddress);
end;

{ Body whose STEP model is REFERENCED, not embedded (batch 5): IPCB_Model.Embed
  (a documented Boolean member) cleared before the model is attached, so the
  golden pins the form a UI-authored library showed for a non-embedded model
  (empty MODELID, MODEL.EMBED=FALSE, MODEL.NAME carrying the path). }
procedure AddBodyStepRef(Comp : IPCB_LibComponent; AFilePath : String);
var
    Body  : IPCB_ComponentBody;
    Model : IPCB_Model;
begin
    Body := PCBServer.PCBObjectFactory(eComponentBodyObject, eNoDimension, eCreate_Default);
    if Body = nil then Exit;
    Model := Body.ModelFactory_FromFilename(AFilePath, False);
    if Model = nil then Exit;
    Model.Embed := False;
    Body.SetState_FromModel;
    Body.Model := Model;
    Comp.AddPCBObject(Body);
    PCBServer.SendMessageToRobots(Comp.I_ObjectAddress, c_Broadcast,
                                  PCBM_BoardRegisteration, Body.I_ObjectAddress);
end;

{ TrueType text with Bold + Italic + Mirror. VERIFIED IPCB_Text members:
  UseTTFonts (True=TrueType), FontName, Bold, Italic, MirrorFlag. }
procedure AddTextStyled(Comp : IPCB_LibComponent; X : Integer; Y : Integer;
                        Content : String; Height : Integer; Lyr : TLayer);
var Txt : IPCB_Text;
begin
    Txt := PCBServer.PCBObjectFactory(eTextObject, eNoDimension, eCreate_Default);
    if Txt = nil then Exit;
    Txt.XLocation := MilsToCoord(X);
    Txt.YLocation := MilsToCoord(Y);
    Txt.Layer     := Lyr;
    Txt.Size      := MilsToCoord(Height);
    Txt.Rotation  := 0.0;
    Txt.Text      := Content;
    Txt.UseTTFonts := True;
    Txt.FontName   := 'Arial';
    Txt.Bold       := True;
    Txt.Italic     := True;
    Txt.MirrorFlag := True;
    Comp.AddPCBObject(Txt);
    PCBServer.SendMessageToRobots(Comp.I_ObjectAddress, c_Broadcast,
                                  PCBM_BoardRegisteration, Txt.I_ObjectAddress);
end;

{ A pad in a jumper group. VERIFIED: JumperID is present in Advpcb.dll (the native
  Delphi engine), which is the test that decides whether a name resolves in
  DelphiScript — a Set* counterpart is not required and its absence proves nothing. }
procedure AddPadJumper(Comp : IPCB_LibComponent; X : Integer; Y : Integer;
                       Nm : String; Jumper : Integer);
var Pad : IPCB_Pad;
begin
    Pad := PCBServer.PCBObjectFactory(ePadObject, eNoDimension, eCreate_Default);
    if Pad = nil then Exit;
    Pad.Name     := Nm;
    Pad.X        := MilsToCoord(X);
    Pad.Y        := MilsToCoord(Y);
    Pad.TopXSize := MilsToCoord(60);
    Pad.TopYSize := MilsToCoord(60);
    Pad.TopShape := eRounded;
    Pad.HoleSize := MilsToCoord(30);
    Pad.Layer    := eMultiLayer;
    Pad.JumperID := Jumper;
    Comp.AddPCBObject(Pad);
    PCBServer.SendMessageToRobots(Comp.I_ObjectAddress, c_Broadcast,
                                  PCBM_BoardRegisteration, Pad.I_ObjectAddress);
end;

{ A board-cutout region. VERIFIED: IPCB_Region.Kind : TRegionKind, direct
  assignment, constant eRegionKind_BoardCutout. }
procedure AddRegionCutout(Comp : IPCB_LibComponent; X1 : Integer; Y1 : Integer;
                          X2 : Integer; Y2 : Integer);
var
    Rgn  : IPCB_Region;
    Cont : IPCB_Contour;
begin
    Rgn := PCBServer.PCBObjectFactory(eRegionObject, eNoDimension, eCreate_Default);
    if Rgn = nil then Exit;
    Cont := Rgn.MainContour.Replicate;
    Rgn.Layer := eTopLayer;
    Rgn.Kind  := eRegionKind_BoardCutout;
    Cont.Count := 4;
    Cont.X[1] := MilsToCoord(X1);  Cont.Y[1] := MilsToCoord(Y1);
    Cont.X[2] := MilsToCoord(X2);  Cont.Y[2] := MilsToCoord(Y1);
    Cont.X[3] := MilsToCoord(X2);  Cont.Y[3] := MilsToCoord(Y2);
    Cont.X[4] := MilsToCoord(X1);  Cont.Y[4] := MilsToCoord(Y2);
    Rgn.SetOutlineContour(Cont);
    Comp.AddPCBObject(Rgn);
    PCBServer.SendMessageToRobots(Comp.I_ObjectAddress, c_Broadcast,
                                  PCBM_BoardRegisteration, Rgn.I_ObjectAddress);
end;

{ Through-hole pad with NON-DEFAULT thermal-relief / power-plane connection (the
  PR-6 format fields). Names VERIFIED: PowerPlaneConnectStyle/Relief*/
  PowerPlaneClearance from the AD24 IDE dump.
  DOCUMENTED NEGATIVE (AD24, batch 4a): the pad-cache paste-mask pattern
  (Padcache := Pad.GetState_Cache; Padcache.PasteMaskExpansionValid :=
  eCacheManual; ...; Pad.SetState_Cache := Padcache) — although verified in
  shipping scripts that operate on EXISTING board pads (SolderPasteGrid.pas,
  SPI_Cleanup_LPW_Footprint.pas) — causes a native ACCESS VIOLATION in
  ScriptingSystem.DLL ("Read of address 0x38" + runtime error 217) on a
  freshly factory-created LIBRARY pad, the same unallocated-structure class
  as the CRPercentage crash. Do not retry on a fresh pad; a mask-expansion
  golden needs a different init sequence (or an existing-pad source). }
procedure AddThPadThermal(Comp : IPCB_LibComponent; X : Integer; Nm : String);
var
    Pad : IPCB_Pad;
begin
    Pad := PCBServer.PCBObjectFactory(ePadObject, eNoDimension, eCreate_Default);
    if Pad = nil then Exit;
    Pad.Name     := Nm;
    Pad.X        := MilsToCoord(X);
    Pad.Y        := MilsToCoord(0);
    Pad.Mode     := ePadMode_Simple;
    Pad.Layer    := eMultiLayer;
    Pad.TopShape := eRounded;
    Pad.TopXSize := MilsToCoord(70);
    Pad.TopYSize := MilsToCoord(70);
    Pad.HoleType := eRoundHole;
    Pad.HoleSize := MilsToCoord(35);
    Pad.PowerPlaneConnectStyle := eDirectConnectToPlane;   { default is Relief }
    Pad.ReliefConductorWidth   := MilsToCoord(15);
    Pad.ReliefEntries          := 2;                       { default is 4 }
    Pad.ReliefAirGap           := MilsToCoord(12);
    Pad.PowerPlaneClearance    := MilsToCoord(25);
    Comp.AddPCBObject(Pad);
    PCBServer.SendMessageToRobots(Comp.I_ObjectAddress, c_Broadcast,
                                  PCBM_BoardRegisteration, Pad.I_ObjectAddress);
end;

{ Batch-4b RETRY of the thermal pad: create + register a PLAIN pad first, and only
  then set the thermal-relief / power-plane properties — the 4a crash hit a pad
  that was not yet part of a component, so the cache-backed setters may work once
  the pad is registered (same theory as the CRPercentage stack-init hypothesis). }
procedure AddThPadThermalPost(Comp : IPCB_LibComponent; X : Integer; Nm : String);
var
    Pad : IPCB_Pad;
begin
    Pad := PCBServer.PCBObjectFactory(ePadObject, eNoDimension, eCreate_Default);
    if Pad = nil then Exit;
    Pad.Name     := Nm;
    Pad.X        := MilsToCoord(X);
    Pad.Y        := MilsToCoord(0);
    Pad.Mode     := ePadMode_Simple;
    Pad.Layer    := eMultiLayer;
    Pad.TopShape := eRounded;
    Pad.TopXSize := MilsToCoord(70);
    Pad.TopYSize := MilsToCoord(70);
    Pad.HoleType := eRoundHole;
    Pad.HoleSize := MilsToCoord(35);
    Comp.AddPCBObject(Pad);
    PCBServer.SendMessageToRobots(Comp.I_ObjectAddress, c_Broadcast,
                                  PCBM_BoardRegisteration, Pad.I_ObjectAddress);
    { Properties AFTER registration. }
    Pad.PowerPlaneConnectStyle := eDirectConnectToPlane;   { default is Relief }
    Pad.ReliefConductorWidth   := MilsToCoord(15);
    Pad.ReliefEntries          := 2;                       { default is 4 }
    Pad.ReliefAirGap           := MilsToCoord(12);
    Pad.PowerPlaneClearance    := MilsToCoord(25);
end;

{ Barcode text (TextKind = eText_BarCode). Names VERIFIED from the AD24 IDE dump:
  TextKind/BarCodeKind (eBarCode128) + BarCodeFullWidth/FullHeight/XMargin/
  MinWidth/FontName/BarCodeInverted. }
procedure AddTextBarcode(Comp : IPCB_LibComponent; X : Integer; Y : Integer;
                         Content : String);
var Txt : IPCB_Text;
begin
    Txt := PCBServer.PCBObjectFactory(eTextObject, eNoDimension, eCreate_Default);
    if Txt = nil then Exit;
    Txt.XLocation := MilsToCoord(X);
    Txt.YLocation := MilsToCoord(Y);
    Txt.Layer     := eTopOverlay;
    Txt.Size      := MilsToCoord(60);
    Txt.Rotation  := 0.0;
    Txt.Text      := Content;
    Txt.TextKind          := eText_BarCode;
    Txt.BarCodeKind       := eBarCode128;
    Txt.BarCodeFullWidth  := MilsToCoord(400);
    Txt.BarCodeFullHeight := MilsToCoord(100);
    Comp.AddPCBObject(Txt);
    PCBServer.SendMessageToRobots(Comp.I_ObjectAddress, c_Broadcast,
                                  PCBM_BoardRegisteration, Txt.I_ObjectAddress);
end;

{ A second barcode whose every sizing field differs from AddTextBarcode's, so
  diffing the two records isolates each field's offset by its authored value.
  All ten BarCode* names are present in Advpcb.dll (the native Delphi engine),
  which is the check that matters — a name found only in the Altium.*.dll .NET
  assemblies (TextJustification, for one) does NOT resolve in DelphiScript. }
procedure AddTextBarcode2(Comp : IPCB_LibComponent; X : Integer; Y : Integer;
                          Content : String);
var Txt : IPCB_Text;
begin
    Txt := PCBServer.PCBObjectFactory(eTextObject, eNoDimension, eCreate_Default);
    if Txt = nil then Exit;
    Txt.XLocation := MilsToCoord(X);
    Txt.YLocation := MilsToCoord(Y);
    Txt.Layer     := eTopOverlay;
    Txt.Size      := MilsToCoord(60);
    Txt.Rotation  := 0.0;
    Txt.Text      := Content;
    Txt.TextKind          := eText_BarCode;
    Txt.BarCodeKind       := eBarCode128;
    Txt.BarCodeFullWidth  := MilsToCoord(600);
    Txt.BarCodeFullHeight := MilsToCoord(150);
    Txt.BarCodeXMargin    := MilsToCoord(30);
    Txt.BarCodeYMargin    := MilsToCoord(40);
    Txt.BarCodeMinWidth   := MilsToCoord(5);
    Txt.BarCodeInverted   := True;
    Txt.BarCodeShowText   := True;
    Txt.BarCodeFontName   := 'Courier New';
    Comp.AddPCBObject(Txt);
    PCBServer.SendMessageToRobots(Comp.I_ObjectAddress, c_Broadcast,
                                  PCBM_BoardRegisteration, Txt.I_ObjectAddress);
end;

{ Barcode variants that differ from AddTextBarcode2 in exactly ONE field each, so
  the remaining offsets can be isolated: BC3 turns Inverted off (@159), BC4 turns
  ShowText off (@225). Everything else is held identical on purpose.

  DOCUMENTED NEGATIVE: BarCodeRenderMode and BarCodeMinWidth are not recoverable this
  way. A BC5 varying only RenderMode moved no byte except @115, which reads 4/3/2/1
  across the barcodes in creation order — an ordinal, not the property. MinWidth@153
  reads 39604/88235 against an authored 5 mil, so Altium computes it from the content
  and width rather than storing what was asked for. }
procedure AddTextBarcodeVariant(Comp : IPCB_LibComponent; X : Integer; Y : Integer;
                                Content : String; Inverted : Boolean;
                                ShowText : Boolean; RenderMode : Integer);
var Txt : IPCB_Text;
begin
    Txt := PCBServer.PCBObjectFactory(eTextObject, eNoDimension, eCreate_Default);
    if Txt = nil then Exit;
    Txt.XLocation := MilsToCoord(X);
    Txt.YLocation := MilsToCoord(Y);
    Txt.Layer     := eTopOverlay;
    Txt.Size      := MilsToCoord(60);
    Txt.Rotation  := 0.0;
    Txt.Text      := Content;
    Txt.TextKind          := eText_BarCode;
    Txt.BarCodeKind       := eBarCode128;
    Txt.BarCodeFullWidth  := MilsToCoord(600);
    Txt.BarCodeFullHeight := MilsToCoord(150);
    Txt.BarCodeXMargin    := MilsToCoord(30);
    Txt.BarCodeYMargin    := MilsToCoord(40);
    Txt.BarCodeMinWidth   := MilsToCoord(5);
    Txt.BarCodeInverted   := Inverted;
    Txt.BarCodeShowText   := ShowText;
    Txt.BarCodeFontName   := 'Courier New';
    Txt.BarCodeRenderMode := RenderMode;
    Comp.AddPCBObject(Txt);
    PCBServer.SendMessageToRobots(Comp.I_ObjectAddress, c_Broadcast,
                                  PCBM_BoardRegisteration, Txt.I_ObjectAddress);
end;

{ Inverted (knockout) TrueType text in an inverted rectangle. Names VERIFIED from
  the AD24 IDE dump: Inverted, UseInvertedRectangle, InvertedTTTextBorder,
  TTFOffsetFromInvertedRect. }
procedure AddTextInverted(Comp : IPCB_LibComponent; X : Integer; Y : Integer;
                          Content : String);
var Txt : IPCB_Text;
begin
    Txt := PCBServer.PCBObjectFactory(eTextObject, eNoDimension, eCreate_Default);
    if Txt = nil then Exit;
    Txt.XLocation := MilsToCoord(X);
    Txt.YLocation := MilsToCoord(Y);
    Txt.Layer     := eTopOverlay;
    Txt.Size      := MilsToCoord(60);
    Txt.Rotation  := 0.0;
    Txt.Text      := Content;
    Txt.UseTTFonts := True;
    Txt.FontName   := 'Arial';
    Txt.Inverted   := True;
    Txt.UseInvertedRectangle := True;
    Txt.InvertedTTTextBorder := MilsToCoord(10);
    { Explicit inverted-rect size. AD computes it lazily from the rendered
      text extent, so a headless save can catch it at 0 (the 2026-09-01
      regeneration did) — author it deterministically instead. }
    Txt.InvRectWidth  := MilsToCoord(120);
    Txt.InvRectHeight := MilsToCoord(70);
    Comp.AddPCBObject(Txt);
    PCBServer.SendMessageToRobots(Comp.I_ObjectAddress, c_Broadcast,
                                  PCBM_BoardRegisteration, Txt.I_ObjectAddress);
end;

{ EMBEDDED STEP component body. Sequence VERIFIED from shipping scripts
  (AutoSTEPplacer.pas, SPI_Cleanup_LPW_Footprint.pas): body factory ->
  ModelFactory_FromFilename(file, false) -> SetState_FromModel -> .Model :=
  -> AddPCBObject. If Altium's importer rejects the minimal STEP file, Model
  comes back nil and we skip (the missing footprint then shows up in the read
  tests -> iterate with a richer file). }
procedure AddBodyStep(Comp : IPCB_LibComponent; AFilePath : String);
var
    Body  : IPCB_ComponentBody;
    Model : IPCB_Model;
begin
    Body := PCBServer.PCBObjectFactory(eComponentBodyObject, eNoDimension, eCreate_Default);
    if Body = nil then Exit;
    Model := Body.ModelFactory_FromFilename(AFilePath, False);
    if Model = nil then Exit;
    Body.SetState_FromModel;
    Body.Model := Model;
    Comp.AddPCBObject(Body);
    PCBServer.SendMessageToRobots(Comp.I_ObjectAddress, c_Broadcast,
                                  PCBM_BoardRegisteration, Body.I_ObjectAddress);
end;

{ Region carrying the descriptive properties a plain copper pour never sets:
  a name, a union index and an explicit Kind. Every other region in the
  library is an unnamed copper box, so these reader arms have no golden
  coverage, and the numeric value behind each TRegionKind is only pinned by
  authoring one region per kind.
  DOCUMENTED NEGATIVE (AD24): ArcResolution is NOT on IPCB_Region —
  `Rgn.ArcResolution` is a compile error, "Undeclared identifier". The name
  is real in the scripting identifier table, but on another interface. }
procedure AddRegionNamed(Comp : IPCB_LibComponent; X1 : Integer; Y1 : Integer;
                         X2 : Integer; Y2 : Integer; RName : String;
                         K : TRegionKind);
var
    Rgn  : IPCB_Region;
    Cont : IPCB_Contour;
begin
    Rgn := PCBServer.PCBObjectFactory(eRegionObject, eNoDimension, eCreate_Default);
    if Rgn = nil then Exit;
    Rgn.Layer         := eTopLayer;
    Rgn.Name          := RName;
    Rgn.Kind          := K;
    Rgn.UnionIndex    := 7;
    Cont := Rgn.MainContour.Replicate;
    Cont.Count := 4;
    Cont.X[1] := MilsToCoord(X1);  Cont.Y[1] := MilsToCoord(Y1);
    Cont.X[2] := MilsToCoord(X2);  Cont.Y[2] := MilsToCoord(Y1);
    Cont.X[3] := MilsToCoord(X2);  Cont.Y[3] := MilsToCoord(Y2);
    Cont.X[4] := MilsToCoord(X1);  Cont.Y[4] := MilsToCoord(Y2);
    Rgn.SetOutlineContour(Cont);
    Comp.AddPCBObject(Rgn);
    PCBServer.SendMessageToRobots(Comp.I_ObjectAddress, c_Broadcast,
                                  PCBM_BoardRegisteration, Rgn.I_ObjectAddress);
end;

{ Component body with a non-zero standoff, cavity height, colour and opacity.
  The plain AddExtrudedBox sits flat in the default grey, so all four are
  unexercised by any other footprint. }
procedure AddBodyProps(Comp : IPCB_LibComponent; CX : Integer; CY : Integer;
                       WMils : Integer; HMils : Integer);
var
    Body  : IPCB_ComponentBody;
    Cont  : IPCB_Contour;
    HalfW : Integer;
    HalfH : Integer;
begin
    HalfW := WMils div 2;
    HalfH := HMils div 2;
    Body := PCBServer.PCBObjectFactory(eComponentBodyObject, eNoDimension, eCreate_Default);
    if Body = nil then Exit;
    Body.BodyProjection := eBoardSide_Top;
    Body.Layer          := LayerUtils.MechanicalLayer(13);
    Body.StandoffHeight := MilsToCoord(10);
    Body.OverallHeight  := MilsToCoord(50);
    Body.CavityHeight   := MilsToCoord(5);
    Body.BodyColor3D    := $0000FF;           { red, against the grey default }
    Body.BodyOpacity3D  := 0.5;
    Cont := Body.MainContour.Replicate;
    Cont.Count := 4;
    Cont.X[1] := MilsToCoord(CX - HalfW);  Cont.Y[1] := MilsToCoord(CY - HalfH);
    Cont.X[2] := MilsToCoord(CX + HalfW);  Cont.Y[2] := MilsToCoord(CY - HalfH);
    Cont.X[3] := MilsToCoord(CX + HalfW);  Cont.Y[3] := MilsToCoord(CY + HalfH);
    Cont.X[4] := MilsToCoord(CX - HalfW);  Cont.Y[4] := MilsToCoord(CY + HalfH);
    Body.SetOutlineContour(Cont);
    Comp.AddPCBObject(Body);
    PCBServer.SendMessageToRobots(Comp.I_ObjectAddress, c_Broadcast,
                                  PCBM_BoardRegisteration, Body.I_ObjectAddress);
end;

{ Via with manual mask expansions, set through TPadCache (GetState_Cache ->
  SetState_Cache) — the same route the pad mask helper uses.
  DOCUMENTED NEGATIVE (AD24): the DIRECT setters
  Via.SolderMaskExpansionMode / .SolderMaskExpansion / .PasteMaskExpansionMode /
  .PasteMaskExpansion compile, then take AD24 down with a native access
  violation in ScriptingSystem.DLL, exactly like the pad thermal-relief
  setters. Always go through the cache. }
procedure AddViaMask(Comp : IPCB_LibComponent; X : Integer; Y : Integer;
                     PadDia : Integer; HoleDia : Integer);
var
    Via   : IPCB_Via;
    Cache : TPadCache;
begin
    Via := PCBServer.PCBObjectFactory(eViaObject, eNoDimension, eCreate_Default);
    if Via = nil then Exit;
    Via.X         := MilsToCoord(X);
    Via.Y         := MilsToCoord(Y);
    Via.Size      := MilsToCoord(PadDia);
    Via.HoleSize  := MilsToCoord(HoleDia);
    Via.LowLayer  := eTopLayer;
    Via.HighLayer := eBottomLayer;
    Via.Mode      := ePadMode_Simple;
    Cache := Via.GetState_Cache;
    Cache.SolderMaskExpansionValid := eCacheManual;
    Cache.SolderMaskExpansion      := MilsToCoord(7);
    Cache.PasteMaskExpansionValid  := eCacheManual;
    Cache.PasteMaskExpansion       := MilsToCoord(3);
    Via.SetState_Cache             := Cache;
    Comp.AddPCBObject(Via);
    PCBServer.SendMessageToRobots(Comp.I_ObjectAddress, c_Broadcast,
                                  PCBM_BoardRegisteration, Via.I_ObjectAddress);
end;

{ Stroke text selecting a non-default stroke font and a non-default stroke
  width. Every other stroke text in the library leaves both at the factory
  value, so the reader's font-id arm (geometry @25, only surfaced above 1) and
  the stroke-width arm have no golden coverage. FontID 2 = Sans Serif,
  3 = Serif. }
procedure AddTextStrokeFont(Comp : IPCB_LibComponent; X : Integer; Y : Integer;
                            Content : String; Height : Integer; AFontID : Integer;
                            WidthMils : Integer);
var
    Txt : IPCB_Text;
begin
    Txt := PCBServer.PCBObjectFactory(eTextObject, eNoDimension, eCreate_Default);
    if Txt = nil then Exit;
    Txt.XLocation  := MilsToCoord(X);
    Txt.YLocation  := MilsToCoord(Y);
    Txt.Layer      := eTopOverlay;
    Txt.Size       := MilsToCoord(Height);
    Txt.Rotation   := 0;
    Txt.UseTTFonts := False;      { stroke, not TrueType }
    Txt.FontID     := AFontID;
    Txt.Width      := MilsToCoord(WidthMils);
    Txt.Text       := Content;
    Comp.AddPCBObject(Txt);
    PCBServer.SendMessageToRobots(Comp.I_ObjectAddress, c_Broadcast,
                                  PCBM_BoardRegisteration, Txt.I_ObjectAddress);
end;

{ Slotted through-hole pad with a rotated slot and no plating. Every hole in the
  library is unrotated and plated, so HoleRotation and Plated read nothing but
  their defaults. }
procedure AddPadSlotRotated(Comp : IPCB_LibComponent; X : Integer; Nm : String);
var
    Pad : IPCB_Pad;
begin
    Pad := PCBServer.PCBObjectFactory(ePadObject, eNoDimension, eCreate_Default);
    if Pad = nil then Exit;
    Pad.X            := MilsToCoord(X);
    Pad.Y            := 0;
    Pad.Name         := Nm;
    Pad.Layer        := eMultiLayer;
    Pad.TopShape     := eRounded;
    Pad.TopXSize     := MilsToCoord(70);
    Pad.TopYSize     := MilsToCoord(50);
    Pad.HoleType     := eSlotHole;
    Pad.HoleSize     := MilsToCoord(20);
    Pad.HoleWidth    := MilsToCoord(40);
    Pad.HoleRotation := 30;
    Pad.Plated       := False;
    Comp.AddPCBObject(Pad);
    PCBServer.SendMessageToRobots(Comp.I_ObjectAddress, c_Broadcast,
                                  PCBM_BoardRegisteration, Pad.I_ObjectAddress);
end;

{ ---- PcbLib authoring -------------------------------------------------------

  Footprints: PAD_SHAPES, PAD_HOLES, VIAS, TRACKS, ARCS, REGIONS, FILLS, TEXT_STROKE,
  TEXT_WIN1252, TEXT_LONG. Each new footprint is wrapped in try/except so one failing primitive
  doesn't abort the whole script (a missing footprint then shows up as a failed read
  test). Blind/buried vias, stacks and 3D bodies follow in later batches. }
procedure GeneratePcbLib;
var
    Lib      : IPCB_Library;
    DefFP    : IPCB_LibComponent;
    Comp     : IPCB_LibComponent;
    Doc      : IServerDocument;
    LongText : String;
    I        : Integer;
    Pad      : IPCB_Pad;
    Cache    : TPadCache;
    Trk      : IPCB_Track;
    Via      : IPCB_Via;
    Arc      : IPCB_Arc;
    Fill     : IPCB_Fill;
    M20      : Integer;
begin
    // CreateNewDocumentFromDocumentKind creates + focuses a blank doc and returns its
    // IServerDocument (Client.OpenNewDocumentOfKind, used in the v0, does not exist).
    Doc := CreateNewDocumentFromDocumentKind('PCBLIB');
    if Doc = nil then Exit;

    Lib := PCBServer.GetCurrentPCBLibrary;   // the new doc is focused
    if Lib = nil then Exit;

    DefFP := Lib.CurrentComponent;           // capture Altium's auto-created default

    Comp := PCBServer.CreatePCBLibComp;
    Comp.Name := 'PAD_SHAPES';
    Lib.RegisterComponent(Comp);             // register before mutating

    PCBServer.PreProcess;
    AddPad(Comp,   0, eRounded,            60, 40, '1');
    AddPad(Comp, 100, eRectangular,        60, 40, '2');
    AddPad(Comp, 200, eOctagonal,          60, 40, '3');
    AddPad(Comp, 300, eRoundedRectangular, 60, 40, '4');
    PCBServer.PostProcess;

    // PAD_HOLES: through-hole pads, one per hole shape (round / square / slot).
    Comp := PCBServer.CreatePCBLibComp;
    Comp.Name := 'PAD_HOLES';
    Lib.RegisterComponent(Comp);

    PCBServer.PreProcess;
    AddThPad(Comp,   0, eRoundHole,  30,  0, '1');
    AddThPad(Comp, 100, eSquareHole, 30,  0, '2');
    AddThPad(Comp, 200, eSlotHole,   40, 20, '3');
    PCBServer.PostProcess;

    // VIAS: two simple through-vias (Top->Bottom), different pad/hole sizes.
    try
        Comp := PCBServer.CreatePCBLibComp;
        Comp.Name := 'VIAS';
        Lib.RegisterComponent(Comp);
        PCBServer.PreProcess;
        AddVia(Comp,  0, 0, 24, 12);
        AddVia(Comp, 80, 0, 40, 20);
        PCBServer.PostProcess;
    except
    end;

    // PAD_STACK: one multi-layer through-hole pad (top/mid/bottom shapes+sizes differ).
    try
        Comp := PCBServer.CreatePCBLibComp;
        Comp.Name := 'PAD_STACK';
        Lib.RegisterComponent(Comp);
        PCBServer.PreProcess;
        AddThStackPad(Comp, 0, '1');
        PCBServer.PostProcess;
    except
    end;

    // TRACKS: a 4-segment silk box (10 mil) + one wider copper track (20 mil).
    try
        Comp := PCBServer.CreatePCBLibComp;
        Comp.Name := 'TRACKS';
        Lib.RegisterComponent(Comp);
        PCBServer.PreProcess;
        AddTrack(Comp, -100, -100,  100, -100, 10, eTopOverlay);
        AddTrack(Comp,  100, -100,  100,  100, 10, eTopOverlay);
        AddTrack(Comp,  100,  100, -100,  100, 10, eTopOverlay);
        AddTrack(Comp, -100,  100, -100, -100, 10, eTopOverlay);
        AddTrack(Comp, -100,    0,  100,    0, 20, eTopLayer);
        PCBServer.PostProcess;
    except
    end;

    // ARCS: full circle (r=50) + quarter arc (r=40).
    try
        Comp := PCBServer.CreatePCBLibComp;
        Comp.Name := 'ARCS';
        Lib.RegisterComponent(Comp);
        PCBServer.PreProcess;
        AddArc(Comp,   0, 0, 50, 0.0, 360.0,  8, eTopOverlay);
        AddArc(Comp, 200, 0, 40, 0.0,  90.0, 10, eTopOverlay);
        PCBServer.PostProcess;
    except
    end;

    // REGIONS: a copper box + a mechanical box (4-vertex each).
    try
        Comp := PCBServer.CreatePCBLibComp;
        Comp.Name := 'REGIONS';
        Lib.RegisterComponent(Comp);
        PCBServer.PreProcess;
        AddRegionBox(Comp, -50, -50,  50,  50, eTopLayer);
        AddRegionBox(Comp, 150, -40, 250,  40, eMechanical1);
        PCBServer.PostProcess;
    except
    end;

    // FILLS: two copper fills on the top layer — one axis-aligned, one rotated 45 deg.
    try
        Comp := PCBServer.CreatePCBLibComp;
        Comp.Name := 'FILLS';
        Lib.RegisterComponent(Comp);
        PCBServer.PreProcess;
        AddFill(Comp,  0, 0,  40, 20, eTopLayer,  0);
        AddFill(Comp, 60, 0, 100, 20, eTopLayer, 45);
        PCBServer.PostProcess;
    except
    end;

    // BODY3D: a simple extruded 3D component body (100x60 mil outline, ~40 mil tall).
    try
        Comp := PCBServer.CreatePCBLibComp;
        Comp.Name := 'BODY3D';
        Lib.RegisterComponent(Comp);
        PCBServer.PreProcess;
        AddExtrudedBox(Comp, 0, 0, 100, 60, 40);
        PCBServer.PostProcess;
    except
    end;

    // TEXT_STROKE: stroke text incl. a 90-deg rotation. (Win-1252 high chars deferred —
    // DelphiScript did not interpret the #$B5 char literal; needs a Chr()-based approach.)
    try
        Comp := PCBServer.CreatePCBLibComp;
        Comp.Name := 'TEXT_STROKE';
        Lib.RegisterComponent(Comp);
        PCBServer.PreProcess;
        AddText(Comp,   0,   0, 'REF',  60,  0, eTopOverlay);
        AddText(Comp,   0, 100, '10uF', 50,  0, eTopOverlay);
        AddText(Comp, 200,   0, 'VERT', 60, 90, eTopOverlay);
        AddText(Comp, 200, 100, '4u7',  50,  0, eTopOverlay);
        PCBServer.PostProcess;
    except
    end;

    // TEXT_WIN1252: high Windows-1252 chars built with Chr() so the raw byte survives (a
    // literal #$B5 was NOT interpreted). Chr(181)=0xB5=micro (renders 10uF as 10<micro>F),
    // Chr(177)=0xB1=plus-minus (renders +/-5%). Same size/layer as the TEXT_STROKE values.
    try
        Comp := PCBServer.CreatePCBLibComp;
        Comp.Name := 'TEXT_WIN1252';
        Lib.RegisterComponent(Comp);
        PCBServer.PreProcess;
        AddText(Comp, 0,   0, '10' + Chr(181) + 'F', 50, 0, eTopOverlay);
        AddText(Comp, 0, 100, Chr(177) + '5%',       50, 0, eTopOverlay);
        PCBServer.PostProcess;
    except
    end;

    // TEXT_LONG: a text longer than 255 characters. Block 1 of a Text record is a Pascal
    // SHORT string, so anything past 255 bytes cannot be stored inline and Altium has to
    // use the out-of-line /{component}/WideStrings stream. Every other text in these
    // samples is short and Win1252-representable, and Altium duplicates those inline, so
    // the reader's WideStrings path has never been proven against a real Altium file
    // (issue #314). 260 'A's plus a marker tail makes the boundary unambiguous.
    //
    // DOCUMENTED NEGATIVE (do not retry): authoring genuine Unicode via Chr(N) for N > 255
    // does NOT work in AD24 DelphiScript — the codepoint is truncated modulo 256. A first
    // attempt with Chr(937)/Chr(956)/Chr(1050)/Chr(20013) (Greek omega and mu, Cyrillic,
    // CJK) produced bytes 169, 188, 26 and 45 instead: '(c)', '1/4' and two control
    // characters. Non-Win1252 text is therefore not authorable by script here.
    try
        Comp := PCBServer.CreatePCBLibComp;
        Comp.Name := 'TEXT_LONG';
        Lib.RegisterComponent(Comp);
        PCBServer.PreProcess;
        LongText := '';
        for I := 1 to 260 do
            LongText := LongText + 'A';
        AddText(Comp, 0, 0, LongText + '_END', 50, 0, eTopOverlay);
        // A short one alongside it, so the component exercises both paths at once.
        AddText(Comp, 0, 100, 'SHORT', 50, 0, eTopOverlay);
        PCBServer.PostProcess;
    except
    end;

    // UNINAME: a footprint whose NAME is outside Windows-1252 (issue #327). The
    // SchLib side of this is UNINAME's symbol counterpart; both are ground truth for
    // which encoding Altium uses for the storage name, the Library/Data component
    // list and the component's own name block. Literal, because Chr(N) truncates
    // modulo 256 (see TEXT_LONG).
    try
        Comp := PCBServer.CreatePCBLibComp;
        Comp.Name := 'Резистор_0402';
        Lib.RegisterComponent(Comp);
        PCBServer.PreProcess;
        AddPadFull(Comp, -25, 0, 0, eRounded, 60, 40, '1');
        AddPadFull(Comp,  25, 0, 0, eRounded, 60, 40, '2');
        PCBServer.PostProcess;
    except
    end;

    // PADMASK: manual paste / solder-mask expansion on a pad — one of the fields the
    // fixture map lists as exercised only by a self-round-trip.
    //
    // These are NOT direct properties. They live behind the pad's cache record, and
    // the enum is eCacheManual (there is no eMaskExpansion_* identifier). Only names
    // verified against shipping AD24 scripts are used here, because DelphiScript
    // resolves identifiers at COMPILE time: one unknown name aborts the entire
    // script, and a try/except around the assignment does not help.
    //
    // Thermal-relief and power-plane setters stay out permanently — see the Pad row
    // in scripts/samples/COVERAGE.md; they crash AD24's scripting DLL.
    try
        Comp := PCBServer.CreatePCBLibComp;
        Comp.Name := 'PADMASK';
        Lib.RegisterComponent(Comp);
        PCBServer.PreProcess;

        Pad := PCBServer.PCBObjectFactory(ePadObject, eNoDimension, eCreate_Default);
        if Pad <> nil then
        begin
            Pad.Name     := '1';
            Pad.X        := MilsToCoord(-40);
            Pad.Y        := MilsToCoord(0);
            Pad.TopXSize := MilsToCoord(70);
            Pad.TopYSize := MilsToCoord(70);
            Pad.TopShape := eRounded;
            Pad.HoleSize := MilsToCoord(30);
            Pad.Layer    := eMultiLayer;

            Cache := Pad.GetState_Cache;
            Cache.PasteMaskExpansionValid  := eCacheManual;
            Cache.PasteMaskExpansion       := MilsToCoord(3);
            Cache.SolderMaskExpansionValid := eCacheManual;
            Cache.SolderMaskExpansion      := MilsToCoord(7);
            Pad.SetState_Cache             := Cache;

            Comp.AddPCBObject(Pad);
            PCBServer.SendMessageToRobots(Comp.I_ObjectAddress, c_Broadcast,
                                          PCBM_BoardRegisteration, Pad.I_ObjectAddress);
        end;

        // A second, plain SMD pad so the first can be compared against a default.
        AddPadFull(Comp, 40, 0, 0, eRectangular, 60, 40, '2');

        // Pad 3: solder-mask expansion measured from the HOLE edge rather than the pad
        // edge. AltiumSharp reads this as a boolean (via SubRecord-1 @258); the name is
        // in the Advpcb.dll RTTI but with no Set* counterpart, so the pad cache is the
        // candidate route — the same record the mask expansions above go through.
        // One unknown identifier this run.
        Pad := PCBServer.PCBObjectFactory(ePadObject, eNoDimension, eCreate_Default);
        if Pad <> nil then
        begin
            Pad.Name     := '3';
            Pad.X        := MilsToCoord(0);
            Pad.Y        := MilsToCoord(60);
            Pad.TopXSize := MilsToCoord(70);
            Pad.TopYSize := MilsToCoord(70);
            Pad.TopShape := eRounded;
            Pad.HoleSize := MilsToCoord(40);
            Pad.Layer    := eMultiLayer;

            Cache := Pad.GetState_Cache;
            Cache.SolderMaskExpansionValid := eCacheManual;
            Cache.SolderMaskExpansion      := MilsToCoord(5);
            Pad.SetState_Cache             := Cache;
            Pad.SolderMaskExpansionFromHoleEdge := True;

            Comp.AddPCBObject(Pad);
            PCBServer.SendMessageToRobots(Comp.I_ObjectAddress, c_Broadcast,
                                          PCBM_BoardRegisteration, Pad.I_ObjectAddress);
        end;

        PCBServer.PostProcess;
    except
    end;

    // LOCKFLAGS_PCB: the locked / keepout flag word, one of the fields the fixture map
    // lists as self-round-trip only. Both flags live in the shared common-header flag
    // word, so a pad and a track between them cover every primitive that carries it.
    // One field family per run: an unknown identifier aborts the whole script.
    try
        Comp := PCBServer.CreatePCBLibComp;
        Comp.Name := 'LOCKFLAGS_PCB';
        Lib.RegisterComponent(Comp);
        PCBServer.PreProcess;

        Pad := PCBServer.PCBObjectFactory(ePadObject, eNoDimension, eCreate_Default);
        if Pad <> nil then
        begin
            Pad.Name     := '1';
            Pad.X        := MilsToCoord(-40);
            Pad.Y        := MilsToCoord(0);
            Pad.TopXSize := MilsToCoord(60);
            Pad.TopYSize := MilsToCoord(40);
            Pad.TopShape := eRounded;
            Pad.Layer    := eTopLayer;
            Pad.Moveable := False;
            Comp.AddPCBObject(Pad);
            PCBServer.SendMessageToRobots(Comp.I_ObjectAddress, c_Broadcast,
                                          PCBM_BoardRegisteration, Pad.I_ObjectAddress);
        end;

        // An unlocked pad as the control.
        AddPadFull(Comp, 40, 0, 0, eRounded, 60, 40, '2');

        // Pad 4: drill tolerances (extended-tail i32 @162/@166). Both are modelled
        // by the reader, unlike the testpoint and jumper fields, so there is
        // something to assert. One identifier family per run.
        Pad := PCBServer.PCBObjectFactory(ePadObject, eNoDimension, eCreate_Default);
        if Pad <> nil then
        begin
            Pad.Name     := '4';
            Pad.X        := MilsToCoord(0);
            Pad.Y        := MilsToCoord(30);
            Pad.TopXSize := MilsToCoord(60);
            Pad.TopYSize := MilsToCoord(60);
            Pad.TopShape := eRounded;
            Pad.HoleSize := MilsToCoord(30);
            Pad.Layer    := eMultiLayer;
            Pad.HolePositiveTolerance := MilsToCoord(3);
            Pad.HoleNegativeTolerance := MilsToCoord(2);
            Comp.AddPCBObject(Pad);
            PCBServer.SendMessageToRobots(Comp.I_ObjectAddress, c_Broadcast,
                                          PCBM_BoardRegisteration, Pad.I_ObjectAddress);
        end;

        // DOCUMENTED NEGATIVE (do not retry): DrillType is not stored on a library
        // pad. The name IS in Advpcb.dll and `Pad.DrillType := 1` compiles and runs
        // without error — but the saved pad is byte-identical to a plain through-hole
        // pad apart from its coordinates, so AD24 keeps the press-fit/simple
        // classification somewhere other than the library record. The probe pad was
        // removed again rather than left asserting nothing.

        // Pads 7-8: a jumper pair. JumperID is in Advpcb.dll and links pads sharing
        // a non-zero id as a 0-ohm net; pad 2 above keeps the default 0 as the control.
        AddPadJumper(Comp,  60, -60, '7', 4);
        AddPadJumper(Comp, 120, -60, '8', 4);

        // Pad 3 carries the keepout flag, the other bit of the same word.
        Pad := PCBServer.PCBObjectFactory(ePadObject, eNoDimension, eCreate_Default);
        if Pad <> nil then
        begin
            Pad.Name     := '3';
            Pad.X        := MilsToCoord(0);
            Pad.Y        := MilsToCoord(-30);
            Pad.TopXSize := MilsToCoord(60);
            Pad.TopYSize := MilsToCoord(40);
            Pad.TopShape := eRounded;
            Pad.Layer    := eTopLayer;
            Pad.IsKeepout := True;
            Comp.AddPCBObject(Pad);
            PCBServer.SendMessageToRobots(Comp.I_ObjectAddress, c_Broadcast,
                                          PCBM_BoardRegisteration, Pad.I_ObjectAddress);
        end;

        Trk := PCBServer.PCBObjectFactory(eTrackObject, eNoDimension, eCreate_Default);
        if Trk <> nil then
        begin
            Trk.X1 := MilsToCoord(-40); Trk.Y1 := MilsToCoord(30);
            Trk.X2 := MilsToCoord(40);  Trk.Y2 := MilsToCoord(30);
            Trk.Width := MilsToCoord(6);
            Trk.Layer := eTopOverlay;
            Trk.Moveable := False;
            Comp.AddPCBObject(Trk);
            PCBServer.SendMessageToRobots(Comp.I_ObjectAddress, c_Broadcast,
                                          PCBM_BoardRegisteration, Trk.I_ObjectAddress);
        end;

        // Pads 5-8: the fabrication and assembly test-point flags, one per pad so each
        // bit of the shared flag word can be read off in isolation. The AD24 spelling
        // is IsTestPoint_Top / IsAssyTestPoint_Top (capital P), confirmed against the
        // Advpcb.dll RTTI alongside their SetIsTestPoint_* setters.
        //
        // Marking a pad as a test point also clears its unlocked bit, so these pads
        // read back locked as well. That is Altium's own behaviour, not a decode
        // defect — see issue #334.
        //
        // DOCUMENTED NEGATIVE (do not retry): the ASSEMBLY test-point flags are not
        // persisted in a PcbLib. IsAssyTestPoint_Top / _Bottom are valid AD24
        // identifiers and author without error, but the saved pad's flag word comes
        // back as a plain 0x000C — no bit set, unlocked still set — where the
        // fabrication flags above give 0x0080 / 0x0100. Those pads were removed
        // again: a fixture that asserts nothing is not coverage.

        Pad := PCBServer.PCBObjectFactory(ePadObject, eNoDimension, eCreate_Default);
        if Pad <> nil then
        begin
            Pad.Name     := '5';
            Pad.X        := MilsToCoord(0);
            Pad.Y        := MilsToCoord(60);
            Pad.TopXSize := MilsToCoord(60);
            Pad.TopYSize := MilsToCoord(60);
            Pad.TopShape := eRounded;
            Pad.HoleSize := MilsToCoord(30);
            Pad.Layer    := eMultiLayer;
            Pad.IsTestPoint_Top := True;
            Comp.AddPCBObject(Pad);
            PCBServer.SendMessageToRobots(Comp.I_ObjectAddress, c_Broadcast,
                                          PCBM_BoardRegisteration, Pad.I_ObjectAddress);
        end;

        Pad := PCBServer.PCBObjectFactory(ePadObject, eNoDimension, eCreate_Default);
        if Pad <> nil then
        begin
            Pad.Name     := '6';
            Pad.X        := MilsToCoord(0);
            Pad.Y        := MilsToCoord(75);
            Pad.TopXSize := MilsToCoord(60);
            Pad.TopYSize := MilsToCoord(60);
            Pad.TopShape := eRounded;
            Pad.HoleSize := MilsToCoord(30);
            Pad.Layer    := eMultiLayer;
            Pad.IsTestPoint_Bottom := True;
            Comp.AddPCBObject(Pad);
            PCBServer.SendMessageToRobots(Comp.I_ObjectAddress, c_Broadcast,
                                          PCBM_BoardRegisteration, Pad.I_ObjectAddress);
        end;

        // DOCUMENTED NEGATIVE (do not retry): none of the remaining fabrication flags
        // is stored on a library pad.
        //
        // TearDrop and UserRouted are the only two AD24 exposes as settable (each has
        // a Set* counterpart in the Advpcb.dll RTTI). Both author without error — the
        // script compiles and the pads appear — but the saved flag word comes back as
        // a plain 0x000C, exactly like an untouched pad, where the fabrication test
        // points above give 0x0080 / 0x0100. The two pads were removed again.
        //
        // IsBackDrill, IsCounterHole and IsPreRoute have no Set* counterpart at all:
        // they are derived board state (layer-stack backdrills, counter-hole params,
        // routing) rather than per-pad properties, so there is nothing to author.

        // A keepout track, the second bit of the same word on the same primitive.
        Trk := PCBServer.PCBObjectFactory(eTrackObject, eNoDimension, eCreate_Default);
        if Trk <> nil then
        begin
            Trk.X1 := MilsToCoord(-40); Trk.Y1 := MilsToCoord(45);
            Trk.X2 := MilsToCoord(40);  Trk.Y2 := MilsToCoord(45);
            Trk.Width := MilsToCoord(6);
            Trk.Layer := eTopOverlay;
            Trk.IsKeepout := True;
            Comp.AddPCBObject(Trk);
            PCBServer.SendMessageToRobots(Comp.I_ObjectAddress, c_Broadcast,
                                          PCBM_BoardRegisteration, Trk.I_ObjectAddress);
        end;

        // Arc and Fill carry the same shared flag word. Moveable/IsKeepout are the
        // identifiers already proven on the pad and track above, so both primitives
        // stay inside this run's one identifier family. Each is authored twice, once
        // per bit, and the plain arcs and fills in ARCS/FILLS are the controls.
        Arc := PCBServer.PCBObjectFactory(eArcObject, eNoDimension, eCreate_Default);
        if Arc <> nil then
        begin
            Arc.XCenter    := MilsToCoord(-40);
            Arc.YCenter    := MilsToCoord(-60);
            Arc.Radius     := MilsToCoord(20);
            Arc.LineWidth  := MilsToCoord(6);
            Arc.StartAngle := 0;
            Arc.EndAngle   := 360;
            Arc.Layer      := eTopOverlay;
            Arc.Moveable   := False;
            Comp.AddPCBObject(Arc);
            PCBServer.SendMessageToRobots(Comp.I_ObjectAddress, c_Broadcast,
                                          PCBM_BoardRegisteration, Arc.I_ObjectAddress);
        end;

        Arc := PCBServer.PCBObjectFactory(eArcObject, eNoDimension, eCreate_Default);
        if Arc <> nil then
        begin
            Arc.XCenter    := MilsToCoord(40);
            Arc.YCenter    := MilsToCoord(-60);
            Arc.Radius     := MilsToCoord(20);
            Arc.LineWidth  := MilsToCoord(6);
            Arc.StartAngle := 0;
            Arc.EndAngle   := 360;
            Arc.Layer      := eTopOverlay;
            Arc.IsKeepout  := True;
            Comp.AddPCBObject(Arc);
            PCBServer.SendMessageToRobots(Comp.I_ObjectAddress, c_Broadcast,
                                          PCBM_BoardRegisteration, Arc.I_ObjectAddress);
        end;

        Fill := PCBServer.PCBObjectFactory(eFillObject, eNoDimension, eCreate_Default);
        if Fill <> nil then
        begin
            Fill.X1Location := MilsToCoord(-70);
            Fill.Y1Location := MilsToCoord(-100);
            Fill.X2Location := MilsToCoord(-30);
            Fill.Y2Location := MilsToCoord(-80);
            Fill.Layer      := eTopLayer;
            Fill.Rotation   := 0;
            Fill.Moveable   := False;
            Comp.AddPCBObject(Fill);
            PCBServer.SendMessageToRobots(Comp.I_ObjectAddress, c_Broadcast,
                                          PCBM_BoardRegisteration, Fill.I_ObjectAddress);
        end;

        Fill := PCBServer.PCBObjectFactory(eFillObject, eNoDimension, eCreate_Default);
        if Fill <> nil then
        begin
            Fill.X1Location := MilsToCoord(30);
            Fill.Y1Location := MilsToCoord(-100);
            Fill.X2Location := MilsToCoord(70);
            Fill.Y2Location := MilsToCoord(-80);
            Fill.Layer      := eTopLayer;
            Fill.Rotation   := 0;
            Fill.IsKeepout  := True;
            Comp.AddPCBObject(Fill);
            PCBServer.SendMessageToRobots(Comp.I_ObjectAddress, c_Broadcast,
                                          PCBM_BoardRegisteration, Fill.I_ObjectAddress);
        end;

        PCBServer.PostProcess;
    except
    end;

    // DOCUMENTED NEGATIVE (do not retry): via tenting is not persisted in a PcbLib.
    // IsTenting_Top / IsTenting_Bottom are valid AD24 identifiers — a VIAFLAGS
    // footprint setting both compiles and authors the via — but the saved library
    // carries no tenting bits, and the via reads back with an empty flag word. The
    // reader decodes ALT_FLAG_TENTING_TOP/BOTTOM correctly (a text round-trip test
    // covers it), so this is Altium's behaviour: tenting on a library via is not
    // stored per-primitive. Nothing to assert, so no fixture was added.

    // EDGE: boundary-case pads — a 45-deg rotated rectangle, a negative-coord pad, a far-out pad.
    try
        Comp := PCBServer.CreatePCBLibComp;
        Comp.Name := 'EDGE';
        Lib.RegisterComponent(Comp);
        PCBServer.PreProcess;
        AddPadFull(Comp,   0,   0, 45, eRectangular, 80, 40, '1');
        AddPadFull(Comp, -50, -30,  0, eRounded,     60, 60, '2');
        AddPadFull(Comp, 200, 150,  0, eRounded,     60, 60, '3');
        PCBServer.PostProcess;
    except
    end;

    // COVERAGE ENRICHMENT (verified AD24 names). Arc fill was dropped for good:
    // IPCB_Arc has no area/fill colour (arcs are stroked open curves).

    // TEXT_STYLE: a TrueType text with Bold + Italic + Mirror set.
    try
        Comp := PCBServer.CreatePCBLibComp;
        Comp.Name := 'TEXT_STYLE';
        Lib.RegisterComponent(Comp);
        PCBServer.PreProcess;
        AddTextStyled(Comp, 0, 0, 'TTF', 60, eTopOverlay);
        PCBServer.PostProcess;
    except
    end;

    // REGION_CUTOUT: a board-cutout region (KIND != copper).
    try
        Comp := PCBServer.CreatePCBLibComp;
        Comp.Name := 'REGION_CUTOUT';
        Lib.RegisterComponent(Comp);
        PCBServer.PreProcess;
        AddRegionCutout(Comp, -50, -50, 50, 50);
        PCBServer.PostProcess;
    except
    end;

    // FINAL DOCUMENTED NEGATIVE (AD24, batches 4a+4b): PAD_THERMAL cannot be
    // authored by script. The thermal-relief / power-plane setters
    // (PowerPlaneConnectStyle / Relief* / PowerPlaneClearance) crash with a
    // native ScriptingSystem.DLL access violation ("Read of address 0x38" +
    // runtime error 217) on a scripted library pad in EVERY tried sequence:
    // before registration, after AddPCBObject + robot registration
    // (AddThPadThermalPost), with and without the GetState_Cache block. The
    // cache-backed pad structures evidently never exist for a scripted PcbLib
    // pad in AD24. Do NOT retry; a thermal-relief golden would need a manually
    // authored library (or a future AD fix).
    // try
    //     Comp := PCBServer.CreatePCBLibComp;
    //     Comp.Name := 'PAD_THERMAL';
    //     Lib.RegisterComponent(Comp);
    //     PCBServer.PreProcess;
    //     AddThPadThermalPost(Comp, 0, '1');
    //     PCBServer.PostProcess;
    // except
    // end;

    // MULTILAYER (batch 4b): one track per exotic layer so layer_from_id's arms
    // get real golden coverage. All layer constants verified in shipping scripts.
    try
        Comp := PCBServer.CreatePCBLibComp;
        Comp.Name := 'MULTILAYER';
        Lib.RegisterComponent(Comp);
        PCBServer.PreProcess;
        AddTrack(Comp, -50, 0, 50, 0, 10, eMechanical2);
        AddTrack(Comp, -50, 20, 50, 20, 10, eMidLayer5);
        AddTrack(Comp, -50, 40, 50, 40, 10, eDrillGuide);
        AddTrack(Comp, -50, 60, 50, 60, 10, eDrillDrawing);
        AddTrack(Comp, -50, 80, 50, 80, 10, eInternalPlane1);
        AddTrack(Comp, -50, 100, 50, 100, 10, eKeepOutLayer);
        PCBServer.PostProcess;
    except
    end;

    // EMBSTEP (batch 4b): a component body with an embedded STEP model, so the
    // Library/Models embedded-model read path gets real golden coverage. The
    // wrapper writes minimal.step into the bridge dir before launching.
    try
        Comp := PCBServer.CreatePCBLibComp;
        Comp.Name := 'EMBSTEP';
        Lib.RegisterComponent(Comp);
        PCBServer.PreProcess;
        AddBodyStep(Comp, OUT_DIR + 'minimal.step');
        PCBServer.PostProcess;
    except
    end;

    // STEP_REF (batch 5): the same STEP model referenced by path, not embedded.
    try
        Comp := PCBServer.CreatePCBLibComp;
        Comp.Name := 'STEP_REF';
        Lib.RegisterComponent(Comp);
        PCBServer.PreProcess;
        AddBodyStepRef(Comp, OUT_DIR + 'minimal.step');
        PCBServer.PostProcess;
    except
    end;

    // MECH20 (batch 5): one primitive of every layered kind on Mechanical 20, a
    // layer past the sixteen the legacy header byte can name. AD has no
    // eMechanical17..32 constants; LayerUtils.MechanicalLayer(N) (proven in
    // AddExtrudedBox for 13) returns the layer id that IPCB_Primitive.Layer takes.
    // Hand-authored tracks showed byte 72 + V7 id 0x01020014; this settles the
    // pair for pads, arcs, text, fills, regions and bodies too.
    try
        Comp := PCBServer.CreatePCBLibComp;
        Comp.Name := 'MECH20';
        Lib.RegisterComponent(Comp);
        PCBServer.PreProcess;
        M20 := LayerUtils.MechanicalLayer(20);
        AddTrack(Comp, -50, 0, 50, 0, 10, M20);
        AddArc(Comp, 0, 60, 20, 0, 180, 10, M20);
        AddText(Comp, 0, 100, 'M20', 50, 0, M20);
        AddFill(Comp, -50, 140, 50, 160, M20, 0);
        AddRegionBox(Comp, -50, 180, 50, 220, M20);
        AddPadOnLayer(Comp, 100, 0, 'M', M20);
        AddExtrudedBoxOnLayer(Comp, 100, 100, 40, 40, 20, M20);
        PCBServer.PostProcess;
    except
    end;

    // TEXT_WIDE_ONLY (batch 5): a text whose WideStrings carries a code unit
    // the Data stream cannot — the shape a UI-typed Ω has (Data byte '?',
    // ENCODEDTEXT 937) — so a golden pins WideStrings as the authoritative
    // form and ENCODEDTEXT as UTF-16 code units of the text as Altium holds
    // it (AltiumSharp and the Latin-1 10µF of TEXT_WIN1252 were the only
    // witnesses). Chr(N) yields the Unicode character N, not a byte: Chr(148)
    // is U+0094, which every ANSI page narrows to '?' while WideStrings keeps
    // 148. DOCUMENTED NEGATIVES (two runs, 2026-08-23): (1) a source LITERAL
    // beyond Latin-1 in a TEXT reaches Altium as its UTF-8 bytes widened
    // through the machine's page ('10 Ω' -> 49,48,32,206,169), assigned
    // directly or through a String parameter alike, unlike a component NAME;
    // (2) Chr(N) for N > 255 truncates modulo 256 (TEXT_LONG); (3) a control
    // the writing page leaves undefined (Chr(152) on a Windows-1250 machine)
    // narrows to its IDENTITY byte there but to '?' elsewhere — page-dependent,
    // so it is not authored. Text beyond U+00FF needs a hand-authored fixture.
    try
        Comp := PCBServer.CreatePCBLibComp;
        Comp.Name := 'TEXT_WIDE_ONLY';
        Lib.RegisterComponent(Comp);
        PCBServer.PreProcess;
        AddText(Comp, 0,   0, '10 ' + Chr(206) + Chr(169), 50, 0, eTopOverlay);
        AddText(Comp, 0, 100, Chr(148) + 'Q' + Chr(187), 50, 0, eTopOverlay);
        PCBServer.PostProcess;
    except
    end;

    // PRIMPROPS: the descriptive properties the plain primitives never set.
    // Only the named region is authored today; the body and via probes are
    // staged behind comments so a crash or a compile error names one interface
    // rather than leaving the whole run ambiguous.
    try
        Comp := PCBServer.CreatePCBLibComp;
        Comp.Name := 'PRIMPROPS';
        Lib.RegisterComponent(Comp);
        PCBServer.PreProcess;
        AddRegionNamed(Comp, -100, -50,   0,  50, 'NamedPour', eRegionKind_NamedRegion);
        AddPadSlotRotated(Comp, -300, 'S1');
        AddTextStrokeFont(Comp, -100, 120, 'SANS',  40, 2, 12);
        AddTextStrokeFont(Comp,  100, 120, 'SERIF', 40, 3, 12);
        AddRegionNamed(Comp,   20, -50, 120,  50, 'CavityRgn', eRegionKind_Cavity);
        AddRegionNamed(Comp,  140, -50, 240,  50, 'BoardCut',  eRegionKind_BoardCutout);
        AddRegionNamed(Comp,  260, -50, 360,  50, 'PlainCut',  eRegionKind_Cutout);
        AddBodyProps(Comp, 200, 0, 80, 60);
        AddViaMask(Comp, 400, 0, 50, 25);
        PCBServer.PostProcess;
    except
    end;

    // TEXT_SPECIAL: a Code-128 barcode text and an inverted (knockout) TrueType
    // text in an inverted rectangle (batch 4a).
    try
        Comp := PCBServer.CreatePCBLibComp;
        Comp.Name := 'TEXT_SPECIAL';
        Lib.RegisterComponent(Comp);
        PCBServer.PreProcess;
        AddTextBarcode(Comp, 0, 100, 'BC128');
        AddTextBarcode2(Comp, 0, -150, 'BC2');
        AddTextBarcodeVariant(Comp, 0, -300, 'BC3', False, True,  0);
        AddTextBarcodeVariant(Comp, 0, -450, 'BC4', True,  False, 0);
        AddTextInverted(Comp, 0, -100, 'INV');
        PCBServer.PostProcess;
    except
    end;

    // BODYPREC: an extruded body whose outline is off-grid by raw internal
    // units, pinning the reader's full outline precision against AD24's own
    // save (enrich probe, 2026-09-01).
    try
        Comp := PCBServer.CreatePCBLibComp;
        Comp.Name := 'BODYPREC';
        Lib.RegisterComponent(Comp);
        PCBServer.PreProcess;
        AddExtrudedBoxOffGrid(Comp);
        PCBServer.PostProcess;
    except
    end;

    // NOTE: a PAD_ROUNDED footprint using CRPercentage[eTopLayer] was tried but the
    // indexed corner-radius setter on a freshly-created Simple pad causes a native
    // ACCESS VIOLATION in ScriptingSystem.DLL (runtime, not compile — escapes
    // try/except). The per-layer corner-radius array is likely not allocated until
    // the pad has a proper stack; deferred until the correct init sequence is known.

    Lib.CurrentComponent := Comp;

    // Delete Altium's empty auto-created default footprint. Unlike SchLib, the PCB
    // removal works: DeRegisterComponent then RemoveComponent -> exactly one footprint.
    if DefFP <> nil then
    begin
        Lib.DeRegisterComponent(DefFP);
        Lib.RemoveComponent(DefFP);
    end;

    Lib.Board.ViewManager_FullUpdate;
    // IServerDocument has no DoFileSaveAs; DoSafeChangeFileNameAndSave is the
    // documented "Save As to a path" (the second arg is the document kind).
    Doc.SetModified(True);
    Doc.DoSafeChangeFileNameAndSave(OUT_DIR + 'footprints.PcbLib', 'PCBLIB');
end;

{ IEEE symbol (RECORD=3, batch 6): factory eSymbol (TObjectId 34 — the constant
  is not spelled "IEEE"); ISch_Symbol carries Symbol (TIeeeSymbol: eDot=1,
  eRightLeftSignalFlow=2, eClock=3, eActiveLowInput=4, ...), ScaleFactor (a
  TCoord), IsMirrored, Orientation and LineWidth over the graphical base. The
  record this crate read as a "text annotation" — settled by authoring one. }
procedure AddIeeeSymbol(Comp : ISch_Component; X : Integer; Y : Integer;
                        AKind : TIeeeSymbol; AScale : Integer; AMirror : Boolean;
                        ARotate : TRotationBy90; AColor : TColor; Locked : Boolean);
var
    Sym : ISch_Symbol;
begin
    Sym := SchServer.SchObjectFactory(eSymbol, eCreate_Default);
    if Sym = nil then Exit;
    Sym.Location    := Point(MilsToCoord(X), MilsToCoord(Y));
    Sym.Symbol      := AKind;
    Sym.ScaleFactor := MilsToCoord(AScale);
    Sym.IsMirrored  := AMirror;
    Sym.Orientation := ARotate;
    Sym.LineWidth   := eSmall;
    Sym.Color       := AColor;
    Sym.GraphicallyLocked    := Locked;
    Sym.OwnerPartId          := 1;
    Sym.OwnerPartDisplayMode := Comp.DisplayMode;
    Comp.AddSchObject(Sym);
    SchServer.RobotManager.SendMessage(Comp.I_ObjectAddress, c_BroadCast,
                                       SCHM_PrimitiveRegistration, Sym.I_ObjectAddress);
end;

{ Elliptical arc (RECORD=11, batch 5): factory eEllipticalArc; ISch_EllipticalArc
  carries Radius + SecondaryRadius + StartAngle/EndAngle + LineWidth over the
  graphical base. Off-grid when Frac is set (the +5000 idiom of AddRectFrac),
  so the record carries Location/Radius/SecondaryRadius _Frac keys. }
procedure AddEllipticalArc(Comp : ISch_Component; CX : Integer; CY : Integer;
                           RX : Integer; RY : Integer; AStart : Double; AEnd : Double;
                           Frac : Boolean; Locked : Boolean);
var
    EA  : ISch_EllipticalArc;
    Off : Integer;
begin
    EA := SchServer.SchObjectFactory(eEllipticalArc, eCreate_Default);
    if EA = nil then Exit;
    Off := 0;
    if Frac then Off := 5000;
    EA.Location        := Point(MilsToCoord(CX) + Off, MilsToCoord(CY) + Off);
    EA.Radius          := MilsToCoord(RX) + Off;
    EA.SecondaryRadius := MilsToCoord(RY) + Off;
    EA.StartAngle      := AStart;
    EA.EndAngle        := AEnd;
    EA.LineWidth       := eSmall;
    EA.Color           := $000000;
    EA.GraphicallyLocked := Locked;
    EA.OwnerPartId          := 1;
    EA.OwnerPartDisplayMode := Comp.DisplayMode;
    Comp.AddSchObject(EA);
    SchServer.RobotManager.SendMessage(Comp.I_ObjectAddress, c_BroadCast,
                                       SCHM_PrimitiveRegistration, EA.I_ObjectAddress);
end;

{ Footprint model link (RECORD=45 with its 44/46/48 chain, batch 5). The
  component's own AddSchImplementation returns the registered ISch_Implementation;
  AddDataFileLink(EntityName, Location, FileKind) is what gives it DatafileCount=1
  and the ModelDatafile*0 keys. IntegratedModel/DatabaseModel are the two flags a
  UI-authored link carries (`IntegratedModel=T|DatabaseModel=T`). }
procedure AddImplementation(Comp : ISch_Component; AName : String; ADesc : String;
                            APath : String; ACurrent : Boolean; AIntegrated : Boolean);
var
    Impl : ISch_Implementation;
begin
    Impl := Comp.AddSchImplementation;
    if Impl = nil then Exit;
    Impl.ModelName   := AName;
    Impl.ModelType   := 'PCBLIB';
    Impl.Description := ADesc;
    Impl.IsCurrent   := ACurrent;
    if AIntegrated then
    begin
        Impl.IntegratedModel := True;
        Impl.DatabaseModel   := True;
    end;
    if APath <> '' then
        Impl.AddDataFileLink(AName, APath, 'PCBLib');
end;

{ Off-grid shapes of every kind that FRACSHAPES (rect + arc) did not cover, each
  carrying the +5000 internal-unit remainder (0.5 mil) on every coordinate so
  the record emits the corresponding _Frac keys: ellipse, pie, round rectangle,
  line, polyline, polygon, bezier and label. }
procedure AddFracShapes(Comp : ISch_Component);
var
    E   : ISch_Ellipse;
    Pie : ISch_Pie;
    RR  : ISch_RoundRectangle;
    L   : ISch_Line;
    PL  : ISch_Polyline;
    Pol : ISch_Polygon;
    Bez : ISch_Bezier;
    Txt : ISch_Label;
begin
    E := SchServer.SchObjectFactory(eEllipse, eCreate_Default);
    if E <> nil then
    begin
        E.Location        := Point(MilsToCoord(-150) + 5000, MilsToCoord(100) + 5000);
        E.Radius          := MilsToCoord(30) + 5000;
        E.SecondaryRadius := MilsToCoord(20) + 5000;
        E.LineWidth       := eSmall;
        E.Color           := $000000;
        E.AreaColor       := $B0FFFF;
        E.IsSolid         := True;
        E.Transparent     := False;
        E.OwnerPartId     := 1;
        E.OwnerPartDisplayMode := Comp.DisplayMode;
        Comp.AddSchObject(E);
        SchServer.RobotManager.SendMessage(Comp.I_ObjectAddress, c_BroadCast, SCHM_PrimitiveRegistration, E.I_ObjectAddress);
    end;

    Pie := SchServer.SchObjectFactory(ePie, eCreate_Default);
    if Pie <> nil then
    begin
        Pie.Location   := Point(MilsToCoord(-50) + 5000, MilsToCoord(100) + 5000);
        Pie.Radius     := MilsToCoord(30) + 5000;
        Pie.LineWidth  := eSmall;
        Pie.Color      := $000000;
        Pie.StartAngle := 30;
        Pie.EndAngle   := 210;
        Pie.AreaColor  := $00FFFF;
        Pie.IsSolid    := True;
        Pie.OwnerPartId := 1;
        Pie.OwnerPartDisplayMode := Comp.DisplayMode;
        Comp.AddSchObject(Pie);
        SchServer.RobotManager.SendMessage(Comp.I_ObjectAddress, c_BroadCast, SCHM_PrimitiveRegistration, Pie.I_ObjectAddress);
    end;

    RR := SchServer.SchObjectFactory(eRoundRectangle, eCreate_Default);
    if RR <> nil then
    begin
        RR.Location      := Point(MilsToCoord(50) + 5000, MilsToCoord(80) + 5000);
        RR.Corner        := Point(MilsToCoord(150) + 5000, MilsToCoord(130) + 5000);
        RR.CornerXRadius := MilsToCoord(10) + 5000;
        RR.CornerYRadius := MilsToCoord(10) + 5000;
        RR.LineWidth     := eSmall;
        RR.Color         := $000000;
        RR.AreaColor     := $B0FFFF;
        RR.IsSolid       := True;
        RR.Transparent   := False;
        RR.OwnerPartId   := 1;
        RR.OwnerPartDisplayMode := Comp.DisplayMode;
        Comp.AddSchObject(RR);
        SchServer.RobotManager.SendMessage(Comp.I_ObjectAddress, c_BroadCast, SCHM_PrimitiveRegistration, RR.I_ObjectAddress);
    end;

    L := SchServer.SchObjectFactory(eLine, eCreate_Default);
    if L <> nil then
    begin
        L.Location  := Point(MilsToCoord(-150) + 5000, MilsToCoord(0) + 5000);
        L.Corner    := Point(MilsToCoord(-50) + 5000, MilsToCoord(30) + 5000);
        L.LineWidth := eSmall;
        L.Color     := $000000;
        L.OwnerPartId := 1;
        L.OwnerPartDisplayMode := Comp.DisplayMode;
        Comp.AddSchObject(L);
        SchServer.RobotManager.SendMessage(Comp.I_ObjectAddress, c_BroadCast, SCHM_PrimitiveRegistration, L.I_ObjectAddress);
    end;

    PL := SchServer.SchObjectFactory(ePolyline, eCreate_Default);
    if PL <> nil then
    begin
        PL.LineWidth := eSmall;
        PL.Color     := $000000;
        PL.ClearAllVertices;
        PL.InsertVertex(1);  PL.Vertex[1] := Point(MilsToCoord(-30) + 5000, MilsToCoord(0) + 5000);
        PL.InsertVertex(2);  PL.Vertex[2] := Point(MilsToCoord(0) + 5000,   MilsToCoord(30) + 5000);
        PL.InsertVertex(3);  PL.Vertex[3] := Point(MilsToCoord(30) + 5000,  MilsToCoord(0) + 5000);
        PL.OwnerPartId := 1;
        PL.OwnerPartDisplayMode := Comp.DisplayMode;
        Comp.AddSchObject(PL);
        SchServer.RobotManager.SendMessage(Comp.I_ObjectAddress, c_BroadCast, SCHM_PrimitiveRegistration, PL.I_ObjectAddress);
    end;

    Pol := SchServer.SchObjectFactory(ePolygon, eCreate_Default);
    if Pol <> nil then
    begin
        Pol.ClearAllVertices;
        Pol.InsertVertex(1);  Pol.Vertex[1] := Point(MilsToCoord(50) + 5000,  MilsToCoord(0) + 5000);
        Pol.InsertVertex(2);  Pol.Vertex[2] := Point(MilsToCoord(150) + 5000, MilsToCoord(0) + 5000);
        Pol.InsertVertex(3);  Pol.Vertex[3] := Point(MilsToCoord(100) + 5000, MilsToCoord(40) + 5000);
        Pol.LineWidth   := eSmall;
        Pol.Color       := $000000;
        Pol.AreaColor   := $B0FFFF;
        Pol.IsSolid     := True;
        Pol.OwnerPartId := 1;
        Pol.OwnerPartDisplayMode := Comp.DisplayMode;
        Comp.AddSchObject(Pol);
        SchServer.RobotManager.SendMessage(Comp.I_ObjectAddress, c_BroadCast, SCHM_PrimitiveRegistration, Pol.I_ObjectAddress);
    end;

    Bez := SchServer.SchObjectFactory(eBezier, eCreate_Default);
    if Bez <> nil then
    begin
        Bez.LineWidth := eSmall;
        Bez.Color     := $000000;
        Bez.ClearAllVertices;
        Bez.InsertVertex(1);  Bez.SetState_Vertex(1, Point(MilsToCoord(-150) + 5000, MilsToCoord(-80) + 5000));
        Bez.InsertVertex(2);  Bez.SetState_Vertex(2, Point(MilsToCoord(-120) + 5000, MilsToCoord(-40) + 5000));
        Bez.InsertVertex(3);  Bez.SetState_Vertex(3, Point(MilsToCoord(-80) + 5000,  MilsToCoord(-120) + 5000));
        Bez.InsertVertex(4);  Bez.SetState_Vertex(4, Point(MilsToCoord(-50) + 5000,  MilsToCoord(-80) + 5000));
        Bez.OwnerPartId := 1;
        Bez.OwnerPartDisplayMode := Comp.DisplayMode;
        Comp.AddSchObject(Bez);
        SchServer.RobotManager.SendMessage(Comp.I_ObjectAddress, c_BroadCast, SCHM_PrimitiveRegistration, Bez.I_ObjectAddress);
    end;

    Txt := SchServer.SchObjectFactory(eLabel, eCreate_Default);
    if Txt <> nil then
    begin
        Txt.Location      := Point(MilsToCoord(50) + 5000, MilsToCoord(-80) + 5000);
        Txt.Orientation   := eRotate0;
        Txt.FontID        := 1;
        Txt.Justification := eJustify_BottomLeft;
        Txt.Color         := $000000;
        Txt.Text          := 'FRAC';
        Txt.OwnerPartId   := 1;
        Txt.OwnerPartDisplayMode := Comp.DisplayMode;
        Comp.AddSchObject(Txt);
        SchServer.RobotManager.SendMessage(Comp.I_ObjectAddress, c_BroadCast, SCHM_PrimitiveRegistration, Txt.I_ObjectAddress);
    end;
end;

{ Adds one pin to a symbol at (0, Y) mils, pointing left (body to the right), with the
  given electrical type, designator and name. Mirrors UltraLibrarian's verified pin flow:
  factory -> set props -> AddSchObject -> per-primitive SCHM_PrimitiveRegistration. }
procedure AddPin(Comp : ISch_Component; Y : Integer; Elec : TPinElectrical;
                 Desig : String; Nm : String);
var
    Pin : ISch_Pin;
begin
    Pin := SchServer.SchObjectFactory(ePin, eCreate_Default);
    if Pin = nil then Exit;
    Pin.Location             := Point(MilsToCoord(0), MilsToCoord(Y));
    Pin.Orientation          := eRotate180;   // electrical end at left, body to the right
    Pin.PinLength            := MilsToCoord(200);
    Pin.Electrical           := Elec;
    Pin.Designator           := Desig;
    Pin.Name                 := Nm;
    Pin.ShowDesignator       := True;
    Pin.ShowName             := True;
    Pin.OwnerPartId          := 1;
    Pin.OwnerPartDisplayMode := Comp.DisplayMode;
    Comp.AddSchObject(Pin);
    SchServer.RobotManager.SendMessage(Comp.I_ObjectAddress, c_BroadCast,
                                       SCHM_PrimitiveRegistration, Pin.I_ObjectAddress);
end;

{ ===================== TIER A — verified-in-UL_Import helpers ===================== }

{ Pin variant: full control over orientation / show flags / hidden, no decoration.
  Superset of AddPin; all setters are exercised by UL_Import SY_AddPin. }
procedure AddPinEx(Comp : ISch_Component; X : Integer; Y : Integer; Len : Integer;
                   Orient : TRotationBy90; Elec : TPinElectrical;
                   Desig : String; Nm : String;
                   ShowNm : Boolean; ShowDes : Boolean; Hidden : Boolean);
var
    Pin : ISch_Pin;
begin
    Pin := SchServer.SchObjectFactory(ePin, eCreate_Default);
    if Pin = nil then Exit;
    Pin.Location             := Point(MilsToCoord(X), MilsToCoord(Y));
    Pin.Orientation          := Orient;        { eRotate0/90/180/270 }
    Pin.PinLength            := MilsToCoord(Len);
    Pin.Electrical           := Elec;
    Pin.Designator           := Desig;
    Pin.Name                 := Nm;
    Pin.ShowDesignator       := ShowDes;
    Pin.ShowName             := ShowNm;
    Pin.IsHidden             := Hidden;        { UNCERTAIN: IsHidden verified on ISch_Parameter, not exercised on ISch_Pin in UL — but is the documented AD24 property }
    Pin.OwnerPartId          := 1;
    Pin.OwnerPartDisplayMode := Comp.DisplayMode;
    Comp.AddSchObject(Pin);
    SchServer.RobotManager.SendMessage(Comp.I_ObjectAddress, c_BroadCast,
                                       SCHM_PrimitiveRegistration, Pin.I_ObjectAddress);
end;

{ Line (X1,Y1)->(X2,Y2) mils. eLine + Location/Corner — verified SY_AddLine. }
procedure AddLine(Comp : ISch_Component; X1 : Integer; Y1 : Integer;
                  X2 : Integer; Y2 : Integer);
var
    Lin : ISch_Line;
begin
    Lin := SchServer.SchObjectFactory(eLine, eCreate_Default);
    if Lin = nil then Exit;
    Lin.Location             := Point(MilsToCoord(X1), MilsToCoord(Y1));
    Lin.Corner               := Point(MilsToCoord(X2), MilsToCoord(Y2));
    Lin.LineWidth            := eSmall;
    Lin.LineStyle            := eLineStyleSolid;
    Lin.Color                := $000000;
    Lin.OwnerPartId          := 1;
    Lin.OwnerPartDisplayMode := Comp.DisplayMode;
    Comp.AddSchObject(Lin);
    SchServer.RobotManager.SendMessage(Comp.I_ObjectAddress, c_BroadCast,
                                       SCHM_PrimitiveRegistration, Lin.I_ObjectAddress);
end;

{ Arc centred (CX,CY) mils, radius R mils, angles in degrees (CCW, 0=+X).
  Full circle => AStart=0, AEnd=360. Verified SY_AddArc (angles take NO MilsToCoord). }
procedure AddSchArc(Comp : ISch_Component; CX : Integer; CY : Integer; R : Integer;
                    AStart : Double; AEnd : Double);
var
    Arc : ISch_Arc;
begin
    Arc := SchServer.SchObjectFactory(eArc, eCreate_Default);
    if Arc = nil then Exit;
    Arc.Location             := Point(MilsToCoord(CX), MilsToCoord(CY));
    Arc.Radius               := MilsToCoord(R);
    Arc.LineWidth            := eSmall;
    Arc.Color                := $000000;
    Arc.StartAngle           := AStart;
    Arc.EndAngle             := AEnd;
    Arc.OwnerPartId          := 1;
    Arc.OwnerPartDisplayMode := Comp.DisplayMode;
    Comp.AddSchObject(Arc);
    SchServer.RobotManager.SendMessage(Comp.I_ObjectAddress, c_BroadCast,
                                       SCHM_PrimitiveRegistration, Arc.I_ObjectAddress);
end;

{ Pie (filled circular sector, RECORD=9). VERIFIED: factory ePie (=12, NOT the
  record id 9); ISch_Pie inherits ISch_Arc geometry (Location/Radius/Start/End
  angle) and adds IsSolid + AreaColor. It has NO Transparent — see the
  documented negative below. }
procedure AddPie(Comp : ISch_Component; CX : Integer; CY : Integer; R : Integer;
                 AStart : Double; AEnd : Double; FillCol : TColor);
var
    Pie : ISch_Pie;
begin
    Pie := SchServer.SchObjectFactory(ePie, eCreate_Default);
    if Pie = nil then Exit;
    Pie.Location             := Point(MilsToCoord(CX), MilsToCoord(CY));
    Pie.Radius               := MilsToCoord(R);
    Pie.LineWidth            := eSmall;
    Pie.Color                := $000000;
    Pie.StartAngle           := AStart;
    Pie.EndAngle             := AEnd;
    Pie.AreaColor            := FillCol;
    Pie.IsSolid              := True;
    Pie.OwnerPartId          := 1;
    Pie.OwnerPartDisplayMode := Comp.DisplayMode;
    Comp.AddSchObject(Pie);
    SchServer.RobotManager.SendMessage(Comp.I_ObjectAddress, c_BroadCast,
                                       SCHM_PrimitiveRegistration, Pie.I_ObjectAddress);
end;

{ Image (embedded/linked picture, RECORD=30). VERIFIED factory eImage (=11);
  ISch_Image members Location/Corner (bounding box), FileName, EmbedImage,
  KeepAspect, IsSolid, Transparent, LineStyle, LineWidth. A non-embedded image
  (EmbedImage=False) just references FileName and needs no image bytes. }
procedure AddImage(Comp : ISch_Component; X1 : Integer; Y1 : Integer;
                   X2 : Integer; Y2 : Integer; AFileName : String);
var
    Img : ISch_Image;
begin
    Img := SchServer.SchObjectFactory(eImage, eCreate_Default);
    if Img = nil then Exit;
    Img.Location             := Point(MilsToCoord(X1), MilsToCoord(Y1));
    Img.Corner               := Point(MilsToCoord(X2), MilsToCoord(Y2));
    Img.LineWidth            := eSmall;
    Img.Color                := $000000;
    Img.FileName             := AFileName;
    Img.EmbedImage           := False;   { link, not embedded — no bytes needed }
    Img.KeepAspect           := True;
    Img.OwnerPartId          := 1;
    Img.OwnerPartDisplayMode := Comp.DisplayMode;
    Comp.AddSchObject(Img);
    SchServer.RobotManager.SendMessage(Comp.I_ObjectAddress, c_BroadCast,
                                       SCHM_PrimitiveRegistration, Img.I_ObjectAddress);
end;

{ EMBEDDED image (RECORD=30 with EmbedImage=True): Altium loads the file at
  AFilePath and stores its bytes in the library /Storage stream (zlib-compressed,
  0xD0-tagged entries named "0","1",... matched to embedded images in order).
  The wrapper (Generate-Samples.ps1) writes a deterministic 70-byte 2x2 BMP to
  OUT_DIR\embed.bmp before launching, so the fixture bytes are known exactly. }
procedure AddImageEmbedded(Comp : ISch_Component; X1 : Integer; Y1 : Integer;
                           X2 : Integer; Y2 : Integer; AFilePath : String);
var
    Img : ISch_Image;
begin
    Img := SchServer.SchObjectFactory(eImage, eCreate_Default);
    if Img = nil then Exit;
    Img.Location             := Point(MilsToCoord(X1), MilsToCoord(Y1));
    Img.Corner               := Point(MilsToCoord(X2), MilsToCoord(Y2));
    Img.LineWidth            := eSmall;
    Img.Color                := $000000;
    Img.FileName             := AFilePath;
    Img.EmbedImage           := True;    { embed - bytes land in /Storage }
    Img.KeepAspect           := True;
    Img.OwnerPartId          := 1;
    Img.OwnerPartDisplayMode := Comp.DisplayMode;
    Comp.AddSchObject(Img);
    SchServer.RobotManager.SendMessage(Comp.I_ObjectAddress, c_BroadCast,
                                       SCHM_PrimitiveRegistration, Img.I_ObjectAddress);
end;

{ Pin carrying the binary-pin TRAILING-STRING fields: SwapId_Pin / SwapId_Part
  (the swap-group tail strings) and DefaultValue. Names VERIFIED from the AD24
  IDE dump (ISch_Pin: SwapId_Pin/SwapId_Part/SwapId_PartPin/SwapId_Pair :
  WideString; DefaultValue : WideString). }
procedure AddPinSwap(Comp : ISch_Component; Y : Integer; Desig : String; Nm : String);
var
    Pin : ISch_Pin;
begin
    Pin := SchServer.SchObjectFactory(ePin, eCreate_Default);
    if Pin = nil then Exit;
    Pin.Location             := Point(MilsToCoord(0), MilsToCoord(Y));
    Pin.Orientation          := eRotate180;
    Pin.PinLength            := MilsToCoord(200);
    Pin.Electrical           := eElectricInput;
    Pin.Designator           := Desig;
    Pin.Name                 := Nm;
    Pin.SwapId_Pin           := 'A';
    Pin.SwapId_Part          := '1';
    Pin.DefaultValue         := '3V3';
    Pin.OwnerPartId          := 1;
    Pin.OwnerPartDisplayMode := Comp.DisplayMode;
    Comp.AddSchObject(Pin);
    SchServer.RobotManager.SendMessage(Comp.I_ObjectAddress, c_BroadCast,
                                       SCHM_PrimitiveRegistration, Pin.I_ObjectAddress);
end;

{ Rectangle assigned to an EXPLICIT display mode (for a DisplayModeCount=2
  symbol) — exercises a non-default OwnerPartDisplayMode on a graphic shape.
  DisplayModeCount : Integer verified in the AD24 IDE dump (ISch_Component). }
procedure AddRectMode(Comp : ISch_Component; X1 : Integer; Y1 : Integer;
                      X2 : Integer; Y2 : Integer; Mode : Integer);
var R : ISch_Rectangle;
begin
    R := SchServer.SchObjectFactory(eRectangle, eCreate_Default);
    if R = nil then Exit;
    R.Location             := Point(MilsToCoord(X1), MilsToCoord(Y1));
    R.Corner               := Point(MilsToCoord(X2), MilsToCoord(Y2));
    R.LineWidth            := eSmall;
    R.Color                := $000000;
    R.AreaColor            := $B0FFFF;
    R.IsSolid              := True;
    R.OwnerPartId          := 1;
    R.OwnerPartDisplayMode := Mode;
    Comp.AddSchObject(R);
    SchServer.RobotManager.SendMessage(Comp.I_ObjectAddress, c_BroadCast,
                                       SCHM_PrimitiveRegistration, R.I_ObjectAddress);
end;

{ Pin assigned to an EXPLICIT display mode (for the DisplayModeCount=2
  symbol): the pin RECORD's own alternate-view byte, which stayed
  self-round-trip only until authored here. ISch_Pin.OwnerPartDisplayMode is
  the same proven member AddPin sets on every pin; the non-default VALUE is
  what the 2026-09-01 enrich probe settled (persists as byte 1 in the saved
  pin record). }
procedure AddPinMode(Comp : ISch_Component; Y : Integer; Desig : String;
                     Nm : String; Mode : Integer);
var
    Pin : ISch_Pin;
begin
    Pin := SchServer.SchObjectFactory(ePin, eCreate_Default);
    if Pin = nil then Exit;
    Pin.Location             := Point(MilsToCoord(0), MilsToCoord(Y));
    Pin.Orientation          := eRotate180;
    Pin.PinLength            := MilsToCoord(200);
    Pin.Electrical           := eElectricPassive;
    Pin.Designator           := Desig;
    Pin.Name                 := Nm;
    Pin.ShowDesignator       := True;
    Pin.ShowName             := True;
    Pin.OwnerPartId          := 1;
    Pin.OwnerPartDisplayMode := Mode;
    Comp.AddSchObject(Pin);
    SchServer.RobotManager.SendMessage(Comp.I_ObjectAddress, c_BroadCast,
                                       SCHM_PrimitiveRegistration, Pin.I_ObjectAddress);
end;

{ Graphically locked pin. ISch_Pin.GraphicallyLocked proven by the 2026-09-01
  enrich probe; persists as flag bit 0x40 in the saved pin record. }
procedure AddPinLocked(Comp : ISch_Component; Y : Integer; Desig : String;
                       Nm : String);
var
    Pin : ISch_Pin;
begin
    Pin := SchServer.SchObjectFactory(ePin, eCreate_Default);
    if Pin = nil then Exit;
    Pin.Location             := Point(MilsToCoord(0), MilsToCoord(Y));
    Pin.Orientation          := eRotate180;
    Pin.PinLength            := MilsToCoord(200);
    Pin.Electrical           := eElectricPassive;
    Pin.Designator           := Desig;
    Pin.Name                 := Nm;
    Pin.ShowDesignator       := True;
    Pin.ShowName             := True;
    Pin.OwnerPartId          := 1;
    Pin.OwnerPartDisplayMode := Comp.DisplayMode;
    Pin.GraphicallyLocked    := True;
    Comp.AddSchObject(Pin);
    SchServer.RobotManager.SendMessage(Comp.I_ObjectAddress, c_BroadCast,
                                       SCHM_PrimitiveRegistration, Pin.I_ObjectAddress);
end;

{ Rectangle with an explicit LineStyle. Persists as `LineStyleExt` placed
  between Corner.Y and LineWidth (2026-09-01 enrich probe) — unlike the
  round-rect, which accepts the property and drops the key. }
procedure AddRectStyled(Comp : ISch_Component; X1 : Integer; Y1 : Integer;
                        X2 : Integer; Y2 : Integer; Style : TLineStyle);
var R : ISch_Rectangle;
begin
    R := SchServer.SchObjectFactory(eRectangle, eCreate_Default);
    if R = nil then Exit;
    R.Location             := Point(MilsToCoord(X1), MilsToCoord(Y1));
    R.Corner               := Point(MilsToCoord(X2), MilsToCoord(Y2));
    R.LineWidth            := eSmall;
    R.LineStyle            := Style;
    R.Color                := $000000;
    R.AreaColor            := $B0FFFF;
    R.IsSolid              := False;
    R.OwnerPartId          := 1;
    R.OwnerPartDisplayMode := Comp.DisplayMode;
    Comp.AddSchObject(R);
    SchServer.RobotManager.SendMessage(Comp.I_ObjectAddress, c_BroadCast,
                                       SCHM_PrimitiveRegistration, R.I_ObjectAddress);
end;

{ OFF-GRID rectangle: coordinates carry a +5000 internal-unit remainder (0.5 mil)
  so the saved record emits the graphic-shape `*_Frac` keys — the same +5000
  pattern the FRACPINS pins use (proven). }
procedure AddRectFrac(Comp : ISch_Component; X1 : Integer; Y1 : Integer;
                      X2 : Integer; Y2 : Integer);
var R : ISch_Rectangle;
begin
    R := SchServer.SchObjectFactory(eRectangle, eCreate_Default);
    if R = nil then Exit;
    R.Location          := Point(MilsToCoord(X1) + 5000, MilsToCoord(Y1) + 5000);
    R.Corner            := Point(MilsToCoord(X2) + 5000, MilsToCoord(Y2) + 5000);
    R.LineWidth         := eSmall;
    R.Color             := $000000;
    R.AreaColor         := $B0FFFF;
    R.IsSolid           := True;
    R.OwnerPartId       := 1;
    R.OwnerPartDisplayMode := Comp.DisplayMode;
    Comp.AddSchObject(R);
    SchServer.RobotManager.SendMessage(Comp.I_ObjectAddress, c_BroadCast,
                                       SCHM_PrimitiveRegistration, R.I_ObjectAddress);
end;

{ OFF-GRID arc: centre carries a +5000 internal-unit remainder (0.5 mil), so the
  record emits Location.X_Frac / Location.Y_Frac / Radius_Frac. }
procedure AddSchArcFrac(Comp : ISch_Component; CX : Integer; CY : Integer; R : Integer);
var Arc : ISch_Arc;
begin
    Arc := SchServer.SchObjectFactory(eArc, eCreate_Default);
    if Arc = nil then Exit;
    Arc.Location   := Point(MilsToCoord(CX) + 5000, MilsToCoord(CY) + 5000);
    Arc.Radius     := MilsToCoord(R) + 5000;
    Arc.StartAngle := 0;
    Arc.EndAngle   := 270;
    Arc.LineWidth  := eSmall;
    Arc.Color      := $000000;
    Arc.OwnerPartId          := 1;
    Arc.OwnerPartDisplayMode := Comp.DisplayMode;
    Comp.AddSchObject(Arc);
    SchServer.RobotManager.SendMessage(Comp.I_ObjectAddress, c_BroadCast,
                                       SCHM_PrimitiveRegistration, Arc.I_ObjectAddress);
end;

{ Bordered multi-line text frame (RECORD=28). All member names VERIFIED against the
  AD24 IDE object-model dump (ISch_TextFrame: Text, WordWrap, ClipToRect, ShowBorder,
  IsSolid, Transparent, TextMargin, TextColor, LineWidth, LineStyle, FontID, Alignment;
  factory constant eTextFrame). Alignment is left at its default (no verified enum
  constant name for THorizontalAlign values — do not guess one). }
procedure AddTextFrame(Comp : ISch_Component; X1 : Integer; Y1 : Integer;
                       X2 : Integer; Y2 : Integer; AText : String);
var
    Frm : ISch_TextFrame;
begin
    Frm := SchServer.SchObjectFactory(eTextFrame, eCreate_Default);
    if Frm = nil then Exit;
    Frm.Location             := Point(MilsToCoord(X1), MilsToCoord(Y1));
    Frm.Corner               := Point(MilsToCoord(X2), MilsToCoord(Y2));
    Frm.Text                 := AText;
    Frm.FontID               := 1;
    Frm.Color                := $000000;
    Frm.AreaColor            := $B0FFFF;
    Frm.TextColor            := $800000;   { dark blue (BGR) }
    Frm.IsSolid              := True;
    Frm.ShowBorder           := True;
    Frm.WordWrap             := True;
    Frm.ClipToRect           := True;
    Frm.LineWidth            := eSmall;
    Frm.TextMargin           := MilsToCoord(2);
    Frm.OwnerPartId          := 1;
    Frm.OwnerPartDisplayMode := Comp.DisplayMode;
    Comp.AddSchObject(Frm);
    SchServer.RobotManager.SendMessage(Comp.I_ObjectAddress, c_BroadCast,
                                       SCHM_PrimitiveRegistration, Frm.I_ObjectAddress);
end;

{ FILLED polygon from 4 corners (a box). ePolygon + VerticesCount + 1-based Vertex[i] +
  IsSolid — verified SY_AddPoly. NOTE: this is RECORD=7 (parse_polygon), NOT a polyline. }
procedure AddPolygonBox(Comp : ISch_Component; X1 : Integer; Y1 : Integer;
                        X2 : Integer; Y2 : Integer; FillCol : TColor);
var
    Pol : ISch_Polygon;
begin
    Pol := SchServer.SchObjectFactory(ePolygon, eCreate_Default);
    if Pol = nil then Exit;
    Pol.ClearAllVertices;
    // InsertVertex grows the array; do NOT also set VerticesCount (that double-counts).
    Pol.InsertVertex(1);  Pol.Vertex[1] := Point(MilsToCoord(X1), MilsToCoord(Y1));
    Pol.InsertVertex(2);  Pol.Vertex[2] := Point(MilsToCoord(X2), MilsToCoord(Y1));
    Pol.InsertVertex(3);  Pol.Vertex[3] := Point(MilsToCoord(X2), MilsToCoord(Y2));
    Pol.InsertVertex(4);  Pol.Vertex[4] := Point(MilsToCoord(X1), MilsToCoord(Y2));
    Pol.LineWidth            := eSmall;
    Pol.Color                := $000000;
    Pol.AreaColor            := FillCol;
    Pol.IsSolid              := True;
    Pol.OwnerPartId          := 1;
    Pol.OwnerPartDisplayMode := Comp.DisplayMode;
    Comp.AddSchObject(Pol);
    SchServer.RobotManager.SendMessage(Comp.I_ObjectAddress, c_BroadCast,
                                       SCHM_PrimitiveRegistration, Pol.I_ObjectAddress);
end;

{ Rectangle (X1,Y1)-(X2,Y2) mils. eRectangle verified; IsSolid/Transparent/AreaColor verified. }
procedure AddRect(Comp : ISch_Component; X1 : Integer; Y1 : Integer; X2 : Integer; Y2 : Integer;
                  Solid : Boolean; FillCol : TColor);
var R : ISch_Rectangle;
begin
    R := SchServer.SchObjectFactory(eRectangle, eCreate_Default);
    if R = nil then Exit;
    R.Location    := Point(MilsToCoord(X1), MilsToCoord(Y1));
    R.Corner      := Point(MilsToCoord(X2), MilsToCoord(Y2));
    R.LineWidth   := eSmall;
    R.Color       := $000000;
    R.AreaColor   := FillCol;
    R.IsSolid     := Solid;
    R.Transparent := False;
    R.OwnerPartId := 1;
    R.OwnerPartDisplayMode := Comp.DisplayMode;
    Comp.AddSchObject(R);
    SchServer.RobotManager.SendMessage(Comp.I_ObjectAddress, c_BroadCast, SCHM_PrimitiveRegistration, R.I_ObjectAddress);
end;

{ Rounded rectangle. eRoundRectangle + CornerXRadius/CornerYRadius verified. }
procedure AddRoundRect(Comp : ISch_Component; X1 : Integer; Y1 : Integer; X2 : Integer; Y2 : Integer;
                       Rx : Integer; Ry : Integer; Solid : Boolean; FillCol : TColor);
var RR : ISch_RoundRectangle;
begin
    RR := SchServer.SchObjectFactory(eRoundRectangle, eCreate_Default);
    if RR = nil then Exit;
    RR.Location      := Point(MilsToCoord(X1), MilsToCoord(Y1));
    RR.Corner        := Point(MilsToCoord(X2), MilsToCoord(Y2));
    RR.CornerXRadius := MilsToCoord(Rx);
    RR.CornerYRadius := MilsToCoord(Ry);
    RR.LineWidth     := eSmall;
    RR.Color         := $000000;
    RR.AreaColor     := FillCol;
    RR.IsSolid       := Solid;
    RR.Transparent   := False;
    RR.OwnerPartId   := 1;
    RR.OwnerPartDisplayMode := Comp.DisplayMode;
    Comp.AddSchObject(RR);
    SchServer.RobotManager.SendMessage(Comp.I_ObjectAddress, c_BroadCast, SCHM_PrimitiveRegistration, RR.I_ObjectAddress);
end;

{ Ellipse centred (CX,CY), X-radius RX, Y-radius RY (mils). eEllipse + Radius/SecondaryRadius
  verified (NOT RadiusX/RadiusY). A circle => RX=RY. No LineStyle on ellipse. }
procedure AddEllipse(Comp : ISch_Component; CX : Integer; CY : Integer; RX : Integer; RY : Integer;
                     Solid : Boolean; FillCol : TColor);
var E : ISch_Ellipse;
begin
    E := SchServer.SchObjectFactory(eEllipse, eCreate_Default);
    if E = nil then Exit;
    E.Location        := Point(MilsToCoord(CX), MilsToCoord(CY));
    E.Radius          := MilsToCoord(RX);
    E.SecondaryRadius := MilsToCoord(RY);
    E.LineWidth       := eSmall;
    E.Color           := $000000;
    E.AreaColor       := FillCol;
    E.IsSolid         := Solid;
    E.Transparent     := False;
    E.OwnerPartId     := 1;
    E.OwnerPartDisplayMode := Comp.DisplayMode;
    Comp.AddSchObject(E);
    SchServer.RobotManager.SendMessage(Comp.I_ObjectAddress, c_BroadCast, SCHM_PrimitiveRegistration, E.I_ObjectAddress);
end;

{ Coverage: a solid ellipse with Transparent := True (proven ISch_Ellipse member,
  non-default value — the plain AddEllipse always sets False). }
procedure AddEllipseTransparent(Comp : ISch_Component; CX : Integer; CY : Integer; RX : Integer; RY : Integer);
var E : ISch_Ellipse;
begin
    E := SchServer.SchObjectFactory(eEllipse, eCreate_Default);
    if E = nil then Exit;
    E.Location        := Point(MilsToCoord(CX), MilsToCoord(CY));
    E.Radius          := MilsToCoord(RX);
    E.SecondaryRadius := MilsToCoord(RY);
    E.LineWidth       := eSmall;
    E.Color           := $000000;
    E.AreaColor       := $B0FFFF;
    E.IsSolid         := True;
    E.Transparent     := True;
    E.OwnerPartId     := 1;
    E.OwnerPartDisplayMode := Comp.DisplayMode;
    Comp.AddSchObject(E);
    SchServer.RobotManager.SendMessage(Comp.I_ObjectAddress, c_BroadCast, SCHM_PrimitiveRegistration, E.I_ObjectAddress);
end;

{ 3-point polyline (open). ePolyline + the verified InsertVertex-before-assign, 1-based sequence
  (VerticesCount alone yields an empty object). Explicit points avoid array literals. }
procedure AddPolyline3(Comp : ISch_Component; X1 : Integer; Y1 : Integer; X2 : Integer; Y2 : Integer;
                       X3 : Integer; Y3 : Integer);
var PL : ISch_Polyline;
begin
    PL := SchServer.SchObjectFactory(ePolyline, eCreate_Default);
    if PL = nil then Exit;
    PL.LineWidth := eSmall;
    PL.Color     := $000000;
    PL.ClearAllVertices;
    // InsertVertex grows the array; do NOT also set VerticesCount (that double-counts).
    PL.InsertVertex(1);  PL.Vertex[1] := Point(MilsToCoord(X1), MilsToCoord(Y1));
    PL.InsertVertex(2);  PL.Vertex[2] := Point(MilsToCoord(X2), MilsToCoord(Y2));
    PL.InsertVertex(3);  PL.Vertex[3] := Point(MilsToCoord(X3), MilsToCoord(Y3));
    PL.OwnerPartId := 1;
    PL.OwnerPartDisplayMode := Comp.DisplayMode;
    Comp.AddSchObject(PL);
    SchServer.RobotManager.SendMessage(Comp.I_ObjectAddress, c_BroadCast, SCHM_PrimitiveRegistration, PL.I_ObjectAddress);
end;

{ Free-text label. eLabel + Orientation(TRotationBy90) + Justification(TTextJustification)
  + Text. Verified SY_AddText. FontID=1 keeps font_id deterministic for the test. }
procedure AddLabel(Comp : ISch_Component; X : Integer; Y : Integer; AText : String;
                   AJustify : TTextJustification; ARotate : TRotationBy90);
var
    Txt : ISch_Label;
begin
    Txt := SchServer.SchObjectFactory(eLabel, eCreate_Default);
    if Txt = nil then Exit;
    Txt.Location             := Point(MilsToCoord(X), MilsToCoord(Y));
    Txt.Orientation          := ARotate;
    Txt.FontID               := 1;             { deterministic; avoids FontManager.GetFontID allocation }
    Txt.Justification        := AJustify;
    Txt.Color                := $000000;
    Txt.Text                 := AText;
    Txt.OwnerPartId          := 1;
    Txt.OwnerPartDisplayMode := Comp.DisplayMode;
    Comp.AddSchObject(Txt);
    SchServer.RobotManager.SendMessage(Comp.I_ObjectAddress, c_BroadCast,
                                       SCHM_PrimitiveRegistration, Txt.I_ObjectAddress);
end;

{ Component parameter. Name = KEY, Text = VALUE (verified: SY_AddParam uses .Name + .Text,
  there is NO .Value setter). IsHidden := Not Visible. eParameter. }
procedure AddParameter(Comp : ISch_Component; AName : String; AValue : String;
                       X : Integer; Y : Integer; AVisible : Boolean;
                       AJustify : TTextJustification; ARotate : TRotationBy90);
var
    Prm : ISch_Parameter;
begin
    Prm := SchServer.SchObjectFactory(eParameter, eCreate_Default);
    if Prm = nil then Exit;
    Prm.IsHidden             := not AVisible;
    Prm.Name                 := AName;         { parameter KEY }
    Prm.Text                 := AValue;        { parameter VALUE/display }
    Prm.Location             := Point(MilsToCoord(X), MilsToCoord(Y));
    Prm.Orientation          := ARotate;
    Prm.FontID               := 1;
    Prm.Justification        := AJustify;
    Prm.Color                := $000000;
    Prm.OwnerPartId          := 1;
    Prm.OwnerPartDisplayMode := Comp.DisplayMode;
    Comp.AddSchObject(Prm);
    SchServer.RobotManager.SendMessage(Comp.I_ObjectAddress, c_BroadCast,
                                       SCHM_PrimitiveRegistration, Prm.I_ObjectAddress);
end;

{ Creates a fresh component (NOT the reused default), registers it, makes it current,
  and returns it. Use for every symbol after PINS_ETYPE. Mirrors the nil-fallback path
  already in GenerateSchLib + UL_Import ImportComponents. }
function NewSymbol(Lib : ISch_Lib; ARef : String; ADesc : String;
                   AParts : Integer) : ISch_Component;
var
    Comp : ISch_Component;
begin
    Result := nil;
    Comp := SchServer.SchObjectFactory(eSchComponent, eCreate_Default);
    if Comp = nil then Exit;
    Comp.LibReference         := ARef;
    Comp.Designator.Text      := 'U?';
    Comp.ComponentDescription := ADesc;
    Comp.PartCount            := AParts;     { logical part count; 1 for single-part }
    Comp.CurrentPartId        := 1;
    Comp.DisplayMode          := 0;
    Lib.AddSchComponent(Comp);
    SchServer.RobotManager.SendMessage(Lib.I_ObjectAddress, c_BroadCast,
                                       SCHM_PrimitiveRegistration, Comp.I_ObjectAddress);
    Lib.CurrentSchComponent := Comp;
    Result := Comp;
end;

{ Pin variant adding decoration slots + an explicit OwnerPartId (for DUALPART).
  Symbol_* property names are documented AD24 ISch_Pin members; eNoSymbol/Dot/Clock
  are the only enum constants used and are the safe/known ones. }
procedure AddPinDecor(Comp : ISch_Component; X : Integer; Y : Integer; Len : Integer;
                      Orient : TRotationBy90; Elec : TPinElectrical;
                      Desig : String; Nm : String; OwnerPart : Integer;
                      SInner : TPinSymbol; SOuter : TPinSymbol;
                      SInside : TPinSymbol; SOutside : TPinSymbol);
var
    Pin : ISch_Pin;
begin
    Pin := SchServer.SchObjectFactory(ePin, eCreate_Default);
    if Pin = nil then Exit;
    Pin.Location             := Point(MilsToCoord(X), MilsToCoord(Y));
    Pin.Orientation          := Orient;
    Pin.PinLength            := MilsToCoord(Len);
    Pin.Electrical           := Elec;
    Pin.Designator           := Desig;
    Pin.Name                 := Nm;
    Pin.ShowDesignator       := True;
    Pin.ShowName             := True;
    Pin.Symbol_InnerEdge     := SInner;     { "Inside Edge" slot  (binary symbol_inner_edge) }
    Pin.Symbol_OuterEdge     := SOuter;     { "Outside Edge" slot (binary symbol_outer_edge) }
    Pin.Symbol_Inner         := SInside;    { "Inside" slot  (binary symbol_inside) }
    Pin.Symbol_Outer         := SOutside;   { "Outside" slot (binary symbol_outside) }
    Pin.OwnerPartId          := OwnerPart;
    Pin.OwnerPartDisplayMode := Comp.DisplayMode;
    Comp.AddSchObject(Pin);
    SchServer.RobotManager.SendMessage(Comp.I_ObjectAddress, c_BroadCast,
                                       SCHM_PrimitiveRegistration, Pin.I_ObjectAddress);
end;

{ ==== COVERAGE-ENRICHMENT HELPERS ==========================================
  These author NON-default property values so the Rust read tests can verify
  them against a real Altium file. LineStyle/Transparent/IsSolid/AreaColor are
  PROVEN (used by AddLine/AddRect/etc. above). GraphicallyLocked/Disabled/Dimmed,
  pin SymbolLineWidth, and the Bezier factory are BEST-EFFORT AD24 names — if one
  is wrong the caller's try/except drops just that symbol. }

{ Line with an explicit LineStyle (eLineStyleSolid/Dashed/Dotted — proven enum). }
procedure AddLineStyled(Comp : ISch_Component; X1 : Integer; Y1 : Integer;
                        X2 : Integer; Y2 : Integer; Style : TLineStyle);
var Lin : ISch_Line;
begin
    Lin := SchServer.SchObjectFactory(eLine, eCreate_Default);
    if Lin = nil then Exit;
    Lin.Location             := Point(MilsToCoord(X1), MilsToCoord(Y1));
    Lin.Corner               := Point(MilsToCoord(X2), MilsToCoord(Y2));
    Lin.LineWidth            := eSmall;
    Lin.LineStyle            := Style;
    Lin.Color                := $000000;
    Lin.OwnerPartId          := 1;
    Lin.OwnerPartDisplayMode := Comp.DisplayMode;
    Comp.AddSchObject(Lin);
    SchServer.RobotManager.SendMessage(Comp.I_ObjectAddress, c_BroadCast,
                                       SCHM_PrimitiveRegistration, Lin.I_ObjectAddress);
end;

{ Rectangle with Transparent := True (proven property, non-default value). }
procedure AddRectTransparent(Comp : ISch_Component; X1 : Integer; Y1 : Integer;
                             X2 : Integer; Y2 : Integer);
var R : ISch_Rectangle;
begin
    R := SchServer.SchObjectFactory(eRectangle, eCreate_Default);
    if R = nil then Exit;
    R.Location    := Point(MilsToCoord(X1), MilsToCoord(Y1));
    R.Corner      := Point(MilsToCoord(X2), MilsToCoord(Y2));
    R.LineWidth   := eSmall;
    R.Color       := $000000;
    R.AreaColor   := $B0FFFF;
    R.IsSolid     := True;
    R.Transparent := True;         { non-default (default False) }
    R.OwnerPartId := 1;
    R.OwnerPartDisplayMode := Comp.DisplayMode;
    Comp.AddSchObject(R);
    SchServer.RobotManager.SendMessage(Comp.I_ObjectAddress, c_BroadCast, SCHM_PrimitiveRegistration, R.I_ObjectAddress);
end;

{ Pin whose (X,Y) is off the integer grid (fractional-mils location), to exercise
  the PinFrac auxiliary stream. Location is set in raw Coord units so we can add a
  sub-mil offset (1 mil = 10000 Coord). }
procedure AddPinFractional(Comp : ISch_Component; X : Integer; Y : Integer; Len : Integer;
                           Orient : TRotationBy90; Elec : TPinElectrical;
                           Desig : String; Nm : String);
var Pin : ISch_Pin;
begin
    Pin := SchServer.SchObjectFactory(ePin, eCreate_Default);
    if Pin = nil then Exit;
    { MilsToCoord(X) + 5000 puts the pin half a mil off-grid -> a non-zero PinFrac. }
    Pin.Location             := Point(MilsToCoord(X) + 5000, MilsToCoord(Y) + 3000);
    Pin.Orientation          := Orient;
    Pin.PinLength            := MilsToCoord(Len);
    Pin.Electrical           := Elec;
    Pin.Designator           := Desig;
    Pin.Name                 := Nm;
    Pin.OwnerPartId          := 1;
    Pin.OwnerPartDisplayMode := Comp.DisplayMode;
    Comp.AddSchObject(Pin);
    SchServer.RobotManager.SendMessage(Comp.I_ObjectAddress, c_BroadCast, SCHM_PrimitiveRegistration, Pin.I_ObjectAddress);
end;

{ Rectangle with the universal display/lock flags set. Names VERIFIED against the
  AD24 IDE object-model dump: GraphicallyLocked / Disabled / Dimmed are Boolean
  members of ISch_GraphicalObject (inherited by every graphic shape).
  DOCUMENTED NEGATIVE (AD24, batch 2): only GraphicallyLocked PERSISTS in the
  saved .SchLib — the fixture's Data stream carries GraphicallyLocked=T and no
  Disabled/Dimmed keys, so the read test asserts GraphicallyLocked only. The
  Disabled/Dimmed assignments below are kept as living probes in case a future
  AD version starts persisting them; do not add fixture assertions for them. }
procedure AddRectFlagged(Comp : ISch_Component; X1 : Integer; Y1 : Integer;
                         X2 : Integer; Y2 : Integer);
var R : ISch_Rectangle;
begin
    R := SchServer.SchObjectFactory(eRectangle, eCreate_Default);
    if R = nil then Exit;
    R.Location          := Point(MilsToCoord(X1), MilsToCoord(Y1));
    R.Corner            := Point(MilsToCoord(X2), MilsToCoord(Y2));
    R.LineWidth         := eSmall;
    R.Color             := $000000;
    R.AreaColor         := $B0FFFF;
    R.IsSolid           := False;
    R.GraphicallyLocked := True;
    R.Disabled          := True;
    R.Dimmed            := True;
    R.OwnerPartId       := 1;
    R.OwnerPartDisplayMode := Comp.DisplayMode;
    Comp.AddSchObject(R);
    SchServer.RobotManager.SendMessage(Comp.I_ObjectAddress, c_BroadCast, SCHM_PrimitiveRegistration, R.I_ObjectAddress);
end;

{ Filled polygon (right TRIANGLE — three vertices from the given box corners:
  (X1,Y1) (X2,Y1) (X2,Y2)) with Transparent := True. VERIFIED: ISch_Polygon HAS
  Transparent (Boolean) but has NO LineStyle — do not set LineStyle on a polygon. }
procedure AddPolygonTransparent(Comp : ISch_Component; X1 : Integer; Y1 : Integer;
                                X2 : Integer; Y2 : Integer);
var Pol : ISch_Polygon;
begin
    Pol := SchServer.SchObjectFactory(ePolygon, eCreate_Default);
    if Pol = nil then Exit;
    Pol.ClearAllVertices;
    Pol.InsertVertex(1);  Pol.Vertex[1] := Point(MilsToCoord(X1), MilsToCoord(Y1));
    Pol.InsertVertex(2);  Pol.Vertex[2] := Point(MilsToCoord(X2), MilsToCoord(Y1));
    Pol.InsertVertex(3);  Pol.Vertex[3] := Point(MilsToCoord(X2), MilsToCoord(Y2));
    Pol.LineWidth            := eSmall;
    Pol.Color                := $000000;
    Pol.AreaColor            := $00FF00;
    Pol.IsSolid              := True;
    Pol.Transparent          := True;
    Pol.OwnerPartId          := 1;
    Pol.OwnerPartDisplayMode := Comp.DisplayMode;
    Comp.AddSchObject(Pol);
    SchServer.RobotManager.SendMessage(Comp.I_ObjectAddress, c_BroadCast, SCHM_PrimitiveRegistration, Pol.I_ObjectAddress);
end;

{ Pin with a non-default symbol line width (VERIFIED: ISch_Pin.Symbol_LineWidth,
  with the underscore — TSize), to exercise the PinSymbolLineWidth aux stream. }
procedure AddPinLineWidth(Comp : ISch_Component; X : Integer; Y : Integer; Len : Integer;
                          Orient : TRotationBy90; Elec : TPinElectrical;
                          Desig : String; Nm : String; W : TSize);
var Pin : ISch_Pin;
begin
    Pin := SchServer.SchObjectFactory(ePin, eCreate_Default);
    if Pin = nil then Exit;
    Pin.Location             := Point(MilsToCoord(X), MilsToCoord(Y));
    Pin.Orientation          := Orient;
    Pin.PinLength            := MilsToCoord(Len);
    Pin.Electrical           := Elec;
    Pin.Designator           := Desig;
    Pin.Name                 := Nm;
    Pin.Symbol_LineWidth     := W;
    Pin.OwnerPartId          := 1;
    Pin.OwnerPartDisplayMode := Comp.DisplayMode;
    Comp.AddSchObject(Pin);
    SchServer.RobotManager.SendMessage(Comp.I_ObjectAddress, c_BroadCast, SCHM_PrimitiveRegistration, Pin.I_ObjectAddress);
end;

{ Cubic Bezier via 4 control points. VERIFIED: factory eBezier; control points via
  the polyline vertex model — InsertVertex(i) then SetState_Vertex(i, Point) (NOT
  Point1..4), 1-based. }
procedure AddBezier4(Comp : ISch_Component; X1 : Integer; Y1 : Integer;
                     X2 : Integer; Y2 : Integer; X3 : Integer; Y3 : Integer;
                     X4 : Integer; Y4 : Integer);
var Bez : ISch_Bezier;
begin
    Bez := SchServer.SchObjectFactory(eBezier, eCreate_Default);
    if Bez = nil then Exit;
    Bez.LineWidth := eSmall;
    Bez.Color     := $000000;
    Bez.ClearAllVertices;
    Bez.InsertVertex(1);  Bez.SetState_Vertex(1, Point(MilsToCoord(X1), MilsToCoord(Y1)));
    Bez.InsertVertex(2);  Bez.SetState_Vertex(2, Point(MilsToCoord(X2), MilsToCoord(Y2)));
    Bez.InsertVertex(3);  Bez.SetState_Vertex(3, Point(MilsToCoord(X3), MilsToCoord(Y3)));
    Bez.InsertVertex(4);  Bez.SetState_Vertex(4, Point(MilsToCoord(X4), MilsToCoord(Y4)));
    Bez.OwnerPartId          := 1;
    Bez.OwnerPartDisplayMode := Comp.DisplayMode;
    Comp.AddSchObject(Bez);
    SchServer.RobotManager.SendMessage(Comp.I_ObjectAddress, c_BroadCast, SCHM_PrimitiveRegistration, Bez.I_ObjectAddress);
end;

{ One of every shape type carrying a DISTINCT non-black Color. Every helper
  above authors Color := $000000, which is the Altium default and is therefore
  omitted from the saved record — so no shape parser's colour arm has ever been
  exercised against a real file. The colours differ from each other so an
  assertion cannot pass by matching the wrong primitive. TColor is $00BBGGRR. }
procedure AddColourShapes(Comp : ISch_Component);
var
    Lin : ISch_Line;
    Rct : ISch_Rectangle;
    RRe : ISch_RoundRectangle;
    Arc : ISch_Arc;
    Ell : ISch_Ellipse;
    Ply : ISch_Polyline;
    Pgn : ISch_Polygon;
    Pwe : ISch_Pie;
    Bez : ISch_Bezier;
    Lbl : ISch_Label;
begin
    Lin := SchServer.SchObjectFactory(eLine, eCreate_Default);
    if Lin <> nil then
    begin
        Lin.Location := Point(MilsToCoord(-200), MilsToCoord(100));
        Lin.Corner   := Point(MilsToCoord(-100), MilsToCoord(100));
        Lin.LineWidth := eSmall;
        Lin.Color     := $0000FF;                { red }
        Lin.OwnerPartId := 1;
        Lin.OwnerPartDisplayMode := Comp.DisplayMode;
        Comp.AddSchObject(Lin);
        SchServer.RobotManager.SendMessage(Comp.I_ObjectAddress, c_BroadCast, SCHM_PrimitiveRegistration, Lin.I_ObjectAddress);
    end;

    Rct := SchServer.SchObjectFactory(eRectangle, eCreate_Default);
    if Rct <> nil then
    begin
        Rct.Location := Point(MilsToCoord(-200), MilsToCoord(40));
        Rct.Corner   := Point(MilsToCoord(-100), MilsToCoord(80));
        Rct.LineWidth := eSmall;
        Rct.Color     := $00FF00;                { green }
        Rct.AreaColor := $FFFF00;
        Rct.IsSolid   := True;
        Rct.OwnerPartId := 1;
        Rct.OwnerPartDisplayMode := Comp.DisplayMode;
        Comp.AddSchObject(Rct);
        SchServer.RobotManager.SendMessage(Comp.I_ObjectAddress, c_BroadCast, SCHM_PrimitiveRegistration, Rct.I_ObjectAddress);
    end;

    RRe := SchServer.SchObjectFactory(eRoundRectangle, eCreate_Default);
    if RRe <> nil then
    begin
        RRe.Location := Point(MilsToCoord(-200), MilsToCoord(-20));
        RRe.Corner   := Point(MilsToCoord(-100), MilsToCoord(20));
        RRe.CornerXRadius := MilsToCoord(10);
        RRe.CornerYRadius := MilsToCoord(8);
        RRe.LineWidth := eSmall;
        RRe.Color     := $FF0000;                { blue }
        RRe.OwnerPartId := 1;
        RRe.OwnerPartDisplayMode := Comp.DisplayMode;
        Comp.AddSchObject(RRe);
        SchServer.RobotManager.SendMessage(Comp.I_ObjectAddress, c_BroadCast, SCHM_PrimitiveRegistration, RRe.I_ObjectAddress);
    end;

    Arc := SchServer.SchObjectFactory(eArc, eCreate_Default);
    if Arc <> nil then
    begin
        Arc.Location := Point(MilsToCoord(-150), MilsToCoord(-80));
        Arc.Radius   := MilsToCoord(30);
        Arc.LineWidth := eSmall;
        Arc.Color     := $00FFFF;                { yellow }
        Arc.StartAngle := 45.0;                  { non-zero: the plain arcs all start at 0, which is omitted }
        Arc.EndAngle   := 315.0;
        Arc.OwnerPartId := 1;
        Arc.OwnerPartDisplayMode := Comp.DisplayMode;
        Comp.AddSchObject(Arc);
        SchServer.RobotManager.SendMessage(Comp.I_ObjectAddress, c_BroadCast, SCHM_PrimitiveRegistration, Arc.I_ObjectAddress);
    end;

    Ell := SchServer.SchObjectFactory(eEllipse, eCreate_Default);
    if Ell <> nil then
    begin
        Ell.Location := Point(MilsToCoord(0), MilsToCoord(100));
        Ell.Radius   := MilsToCoord(40);
        Ell.SecondaryRadius := MilsToCoord(25);
        Ell.LineWidth := eSmall;
        Ell.Color     := $FF00FF;                { magenta }
        Ell.OwnerPartId := 1;
        Ell.OwnerPartDisplayMode := Comp.DisplayMode;
        Comp.AddSchObject(Ell);
        SchServer.RobotManager.SendMessage(Comp.I_ObjectAddress, c_BroadCast, SCHM_PrimitiveRegistration, Ell.I_ObjectAddress);
    end;

    Ply := SchServer.SchObjectFactory(ePolyline, eCreate_Default);
    if Ply <> nil then
    begin
        Ply.LineWidth := eSmall;
        Ply.Color     := $808000;                { teal }
        Ply.ClearAllVertices;
        Ply.InsertVertex(1);  Ply.Vertex[1] := Point(MilsToCoord(60),  MilsToCoord(60));
        Ply.InsertVertex(2);  Ply.Vertex[2] := Point(MilsToCoord(110), MilsToCoord(110));
        Ply.InsertVertex(3);  Ply.Vertex[3] := Point(MilsToCoord(160), MilsToCoord(60));
        Ply.OwnerPartId := 1;
        Ply.OwnerPartDisplayMode := Comp.DisplayMode;
        Comp.AddSchObject(Ply);
        SchServer.RobotManager.SendMessage(Comp.I_ObjectAddress, c_BroadCast, SCHM_PrimitiveRegistration, Ply.I_ObjectAddress);
    end;

    Pgn := SchServer.SchObjectFactory(ePolygon, eCreate_Default);
    if Pgn <> nil then
    begin
        Pgn.ClearAllVertices;
        Pgn.InsertVertex(1);  Pgn.Vertex[1] := Point(MilsToCoord(60),  MilsToCoord(0));
        Pgn.InsertVertex(2);  Pgn.Vertex[2] := Point(MilsToCoord(160), MilsToCoord(0));
        Pgn.InsertVertex(3);  Pgn.Vertex[3] := Point(MilsToCoord(160), MilsToCoord(40));
        Pgn.LineWidth := eSmall;
        Pgn.Color     := $000080;                { dark red }
        Pgn.AreaColor := $C0C0C0;
        Pgn.IsSolid   := True;
        Pgn.OwnerPartId := 1;
        Pgn.OwnerPartDisplayMode := Comp.DisplayMode;
        Comp.AddSchObject(Pgn);
        SchServer.RobotManager.SendMessage(Comp.I_ObjectAddress, c_BroadCast, SCHM_PrimitiveRegistration, Pgn.I_ObjectAddress);
    end;

    Pwe := SchServer.SchObjectFactory(ePie, eCreate_Default);
    if Pwe <> nil then
    begin
        Pwe.Location := Point(MilsToCoord(110), MilsToCoord(-60));
        Pwe.Radius   := MilsToCoord(35);
        Pwe.StartAngle := 20.0;
        Pwe.EndAngle   := 160.0;
        Pwe.LineWidth := eSmall;
        Pwe.Color     := $008080;                { olive }
        Pwe.AreaColor := $00A5FF;
        Pwe.IsSolid   := True;
        Pwe.OwnerPartId := 1;
        Pwe.OwnerPartDisplayMode := Comp.DisplayMode;
        Comp.AddSchObject(Pwe);
        SchServer.RobotManager.SendMessage(Comp.I_ObjectAddress, c_BroadCast, SCHM_PrimitiveRegistration, Pwe.I_ObjectAddress);
    end;

    Bez := SchServer.SchObjectFactory(eBezier, eCreate_Default);
    if Bez <> nil then
    begin
        Bez.LineWidth := eMedium;                { non-default width, alongside the colour }
        Bez.Color     := $804000;                { navy-ish }
        Bez.ClearAllVertices;
        Bez.InsertVertex(1);  Bez.SetState_Vertex(1, Point(MilsToCoord(-200), MilsToCoord(-140)));
        Bez.InsertVertex(2);  Bez.SetState_Vertex(2, Point(MilsToCoord(-150), MilsToCoord(-100)));
        Bez.InsertVertex(3);  Bez.SetState_Vertex(3, Point(MilsToCoord(-100), MilsToCoord(-100)));
        Bez.InsertVertex(4);  Bez.SetState_Vertex(4, Point(MilsToCoord(-50),  MilsToCoord(-140)));
        Bez.OwnerPartId := 1;
        Bez.OwnerPartDisplayMode := Comp.DisplayMode;
        Comp.AddSchObject(Bez);
        SchServer.RobotManager.SendMessage(Comp.I_ObjectAddress, c_BroadCast, SCHM_PrimitiveRegistration, Bez.I_ObjectAddress);
    end;

    Lbl := SchServer.SchObjectFactory(eLabel, eCreate_Default);
    if Lbl <> nil then
    begin
        Lbl.Location := Point(MilsToCoord(60), MilsToCoord(-120));
        Lbl.Orientation := eRotate0;
        Lbl.FontID   := 1;
        Lbl.Justification := eJustify_BottomLeft;
        Lbl.Color    := $4080FF;                 { orange }
        Lbl.Text     := 'COLOURED';
        Lbl.OwnerPartId := 1;
        Lbl.OwnerPartDisplayMode := Comp.DisplayMode;
        Comp.AddSchObject(Lbl);
        SchServer.RobotManager.SendMessage(Comp.I_ObjectAddress, c_BroadCast, SCHM_PrimitiveRegistration, Lbl.I_ObjectAddress);
    end;
end;

{ Polyline carrying every styling property it owns: a non-default LineStyle,
  both end shapes, the end-shape size, and Transparent. All five names resolve
  in the DelphiScript identifier table (see scripts/samples/COVERAGE.md for how that
  is checked); whether AD24 persists each is what this fixture establishes. }
procedure AddPolylineStyled(Comp : ISch_Component; X1 : Integer; Y1 : Integer;
                            X2 : Integer; Y2 : Integer; X3 : Integer; Y3 : Integer);
var PL : ISch_Polyline;
begin
    PL := SchServer.SchObjectFactory(ePolyline, eCreate_Default);
    if PL = nil then Exit;
    PL.LineWidth      := eSmall;
    PL.Color          := $000000;
    PL.LineStyle      := eLineStyleDashed;
    PL.StartLineShape := eLineShapeArrow;
    PL.EndLineShape   := eLineShapeSolidArrow;
    PL.LineShapeSize  := eLarge;
    PL.ClearAllVertices;
    PL.InsertVertex(1);  PL.Vertex[1] := Point(MilsToCoord(X1), MilsToCoord(Y1));
    PL.InsertVertex(2);  PL.Vertex[2] := Point(MilsToCoord(X2), MilsToCoord(Y2));
    PL.InsertVertex(3);  PL.Vertex[3] := Point(MilsToCoord(X3), MilsToCoord(Y3));
    PL.OwnerPartId := 1;
    PL.OwnerPartDisplayMode := Comp.DisplayMode;
    Comp.AddSchObject(PL);
    SchServer.RobotManager.SendMessage(Comp.I_ObjectAddress, c_BroadCast, SCHM_PrimitiveRegistration, PL.I_ObjectAddress);
end;

{ DOCUMENTED NEGATIVE (AD24): a round rectangle ACCEPTS LineStyle but does not
  persist it — unlike ISch_Line and ISch_Polyline, which both save it. The
  saved RECORD=10 carries no LineStyle key at all, so the read test asserts the
  0 default. Kept as a living probe in case a later AD version starts writing
  it; do not assert a non-zero value. }
procedure AddRoundRectStyled(Comp : ISch_Component; X1 : Integer; Y1 : Integer;
                             X2 : Integer; Y2 : Integer);
var RR : ISch_RoundRectangle;
begin
    RR := SchServer.SchObjectFactory(eRoundRectangle, eCreate_Default);
    if RR = nil then Exit;
    RR.Location      := Point(MilsToCoord(X1), MilsToCoord(Y1));
    RR.Corner        := Point(MilsToCoord(X2), MilsToCoord(Y2));
    RR.CornerXRadius := MilsToCoord(12);
    RR.CornerYRadius := MilsToCoord(12);
    RR.LineWidth     := eSmall;
    RR.Color         := $000000;
    RR.LineStyle     := eLineStyleDotted;
    RR.OwnerPartId   := 1;
    RR.OwnerPartDisplayMode := Comp.DisplayMode;
    Comp.AddSchObject(RR);
    SchServer.RobotManager.SendMessage(Comp.I_ObjectAddress, c_BroadCast, SCHM_PrimitiveRegistration, RR.I_ObjectAddress);
end;

{ DOCUMENTED NEGATIVE (AD24): an arc has NO FILL. `Arc.IsSolid := True` is a
  COMPILE error, "Undeclared identifier: IsSolid" — ISch_Arc is a stroked shape
  and carries neither IsSolid nor AreaColor. A SchLib arc record can therefore
  never gain a fill from Altium; the reader keeps the fields for hand-edited
  files only. Do not reintroduce a filled-arc helper. }

{ DOCUMENTED NEGATIVE (AD24): a pie has NO Transparent. `Pie.Transparent`
  is a COMPILE error, "Undeclared identifier: Transparent" — the property is
  real on ISch_Rectangle/RoundRectangle/Ellipse/Polygon but absent from
  ISch_Pie, which carries only IsSolid + AreaColor. Do not reintroduce a
  transparent-pie helper. }

{ Label with the mirror flag. AD24 writes IsMirrored BEFORE UniqueID here and
  AFTER it on a parameter record — the key orders genuinely differ. }
procedure AddLabelFlagged(Comp : ISch_Component; X : Integer; Y : Integer; AText : String);
var L : ISch_Label;
begin
    L := SchServer.SchObjectFactory(eLabel, eCreate_Default);
    if L = nil then Exit;
    L.Location      := Point(MilsToCoord(X), MilsToCoord(Y));
    L.Orientation   := eRotate0;
    L.FontID        := 1;
    L.Justification := eJustify_BottomLeft;
    L.Color         := $000000;
    L.Text          := AText;
    L.IsMirrored    := True;
    L.OwnerPartId   := 1;
    L.OwnerPartDisplayMode := Comp.DisplayMode;
    Comp.AddSchObject(L);
    SchServer.RobotManager.SendMessage(Comp.I_ObjectAddress, c_BroadCast, SCHM_PrimitiveRegistration, L.I_ObjectAddress);
end;

{ Parameter display properties the plain AddParameter never sets: the name shown
  beside the value, a read-only state and mirroring. All three persist. }
procedure AddParameterProps(Comp : ISch_Component; AName : String; AValue : String;
                            X : Integer; Y : Integer);
var Par : ISch_Parameter;
begin
    Par := SchServer.SchObjectFactory(eParameter, eCreate_Default);
    if Par = nil then Exit;
    Par.Location      := Point(MilsToCoord(X), MilsToCoord(Y));
    Par.Name          := AName;
    Par.Text          := AValue;
    Par.FontID        := 1;
    Par.Color         := $000000;
    Par.IsHidden      := False;
    Par.ShowName      := True;
    Par.ReadOnlyState := 1;
    Par.IsMirrored    := True;
    Par.ParamType     := eParameterType_Integer;
    Par.OwnerPartId   := 1;
    Par.OwnerPartDisplayMode := Comp.DisplayMode;
    Comp.AddSchObject(Par);
    SchServer.RobotManager.SendMessage(Comp.I_ObjectAddress, c_BroadCast, SCHM_PrimitiveRegistration, Par.I_ObjectAddress);
end;

{ DOCUMENTED NEGATIVE (AD24): a polyline's fill properties are accepted and
  then dropped. AreaColor / IsSolid / Transparent all compile on ISch_Polyline
  but the saved RECORD=6 carries none of them, unlike the rectangle and
  polygon records that do persist theirs. Kept as a living probe: the read
  test asserts the defaults, so a later AD version that starts writing them
  shows up as a failure. }
procedure AddPolylineTransparent(Comp : ISch_Component; X1 : Integer; Y1 : Integer;
                                 X2 : Integer; Y2 : Integer; X3 : Integer; Y3 : Integer);
var PL : ISch_Polyline;
begin
    PL := SchServer.SchObjectFactory(ePolyline, eCreate_Default);
    if PL = nil then Exit;
    PL.LineWidth   := eSmall;
    PL.Color       := $000000;
    PL.AreaColor   := $00FFFF;
    PL.IsSolid     := True;
    PL.Transparent := True;
    PL.ClearAllVertices;
    PL.InsertVertex(1);  PL.Vertex[1] := Point(MilsToCoord(X1), MilsToCoord(Y1));
    PL.InsertVertex(2);  PL.Vertex[2] := Point(MilsToCoord(X2), MilsToCoord(Y2));
    PL.InsertVertex(3);  PL.Vertex[3] := Point(MilsToCoord(X3), MilsToCoord(Y3));
    PL.OwnerPartId := 1;
    PL.OwnerPartDisplayMode := Comp.DisplayMode;
    Comp.AddSchObject(PL);
    SchServer.RobotManager.SendMessage(Comp.I_ObjectAddress, c_BroadCast, SCHM_PrimitiveRegistration, PL.I_ObjectAddress);
end;

{ DOCUMENTED NEGATIVE (AD24): an image has NO ShowBorder. `Img.ShowBorder` is a
  compile error, "Undeclared identifier" — the property is real on
  ISch_TextFrame but not on ISch_Image, and no authored image record carries
  the key. The reader keeps the field for hand-edited files only. }

{ Text frame with a whole-mil text margin, which is the point: the plain frame's
  margin is sub-mil, so the record carries only TextMargin_Frac and the integer
  key is never exercised.
  DOCUMENTED NEGATIVE (AD24): Transparent is accepted on a text frame and then
  not written — the saved RECORD=28 has no Transparent key.
  DOCUMENTED NEGATIVE (AD24): a text frame has NO Orientation.
  `Frm.Orientation` is a compile error, "Undeclared identifier"; the property
  is real on ISch_Label / ISch_Parameter / ISch_Pin but not on ISch_TextFrame,
  and no authored frame record carries the key. }
procedure AddTextFrameStyled(Comp : ISch_Component; X1 : Integer; Y1 : Integer;
                             X2 : Integer; Y2 : Integer; AText : String);
var Frm : ISch_TextFrame;
begin
    Frm := SchServer.SchObjectFactory(eTextFrame, eCreate_Default);
    if Frm = nil then Exit;
    Frm.Location    := Point(MilsToCoord(X1), MilsToCoord(Y1));
    Frm.Corner      := Point(MilsToCoord(X2), MilsToCoord(Y2));
    Frm.Text        := AText;
    Frm.FontID      := 1;
    Frm.Color       := $000000;
    Frm.AreaColor   := $B0FFFF;
    Frm.TextColor   := $800000;
    Frm.IsSolid     := True;
    Frm.ShowBorder  := True;
    Frm.WordWrap    := True;
    Frm.ClipToRect  := True;
    Frm.LineWidth   := eSmall;
    Frm.TextMargin  := MilsToCoord(30);
    Frm.Transparent := True;
    Frm.OwnerPartId := 1;
    Frm.OwnerPartDisplayMode := Comp.DisplayMode;
    Comp.AddSchObject(Frm);
    SchServer.RobotManager.SendMessage(Comp.I_ObjectAddress, c_BroadCast, SCHM_PrimitiveRegistration, Frm.I_ObjectAddress);
end;


{ One of each remaining shape type with GraphicallyLocked := True. The flag lives
  on ISch_GraphicalObject so it should reach every shape, but AD24 has already
  shown that whether a property is WRITTEN varies per record: Disabled and Dimmed
  are dropped, a polyline's fill is dropped, a text frame's transparency is
  dropped. Authoring one per shape is the only way to know which persist. }
procedure AddLockedShapes(Comp : ISch_Component);
var
    Lin : ISch_Line;
    Arc : ISch_Arc;
    Ell : ISch_Ellipse;
    RRe : ISch_RoundRectangle;
    Ply : ISch_Polyline;
    Pgn : ISch_Polygon;
    Pwe : ISch_Pie;
    Bez : ISch_Bezier;
    Lbl : ISch_Label;
begin
    Lin := SchServer.SchObjectFactory(eLine, eCreate_Default);
    if Lin <> nil then
    begin
        Lin.Location  := Point(MilsToCoord(-200), MilsToCoord(100));
        Lin.Corner    := Point(MilsToCoord(-120), MilsToCoord(100));
        Lin.LineWidth := eSmall;
        Lin.GraphicallyLocked := True;
        Lin.OwnerPartId := 1;
        Lin.OwnerPartDisplayMode := Comp.DisplayMode;
        Comp.AddSchObject(Lin);
        SchServer.RobotManager.SendMessage(Comp.I_ObjectAddress, c_BroadCast, SCHM_PrimitiveRegistration, Lin.I_ObjectAddress);
    end;
    Arc := SchServer.SchObjectFactory(eArc, eCreate_Default);
    if Arc <> nil then
    begin
        Arc.Location   := Point(MilsToCoord(-200), MilsToCoord(40));
        Arc.Radius     := MilsToCoord(20);
        Arc.StartAngle := 0;
        Arc.EndAngle   := 180;
        Arc.LineWidth  := eSmall;
        Arc.GraphicallyLocked := True;
        Arc.OwnerPartId := 1;
        Arc.OwnerPartDisplayMode := Comp.DisplayMode;
        Comp.AddSchObject(Arc);
        SchServer.RobotManager.SendMessage(Comp.I_ObjectAddress, c_BroadCast, SCHM_PrimitiveRegistration, Arc.I_ObjectAddress);
    end;
    Ell := SchServer.SchObjectFactory(eEllipse, eCreate_Default);
    if Ell <> nil then
    begin
        Ell.Location        := Point(MilsToCoord(-120), MilsToCoord(40));
        Ell.Radius          := MilsToCoord(25);
        Ell.SecondaryRadius := MilsToCoord(15);
        Ell.LineWidth       := eSmall;
        Ell.GraphicallyLocked := True;
        Ell.OwnerPartId := 1;
        Ell.OwnerPartDisplayMode := Comp.DisplayMode;
        Comp.AddSchObject(Ell);
        SchServer.RobotManager.SendMessage(Comp.I_ObjectAddress, c_BroadCast, SCHM_PrimitiveRegistration, Ell.I_ObjectAddress);
    end;
    RRe := SchServer.SchObjectFactory(eRoundRectangle, eCreate_Default);
    if RRe <> nil then
    begin
        RRe.Location      := Point(MilsToCoord(-200), MilsToCoord(-20));
        RRe.Corner        := Point(MilsToCoord(-120), MilsToCoord(10));
        RRe.CornerXRadius := MilsToCoord(8);
        RRe.CornerYRadius := MilsToCoord(8);
        RRe.LineWidth     := eSmall;
        RRe.GraphicallyLocked := True;
        RRe.OwnerPartId := 1;
        RRe.OwnerPartDisplayMode := Comp.DisplayMode;
        Comp.AddSchObject(RRe);
        SchServer.RobotManager.SendMessage(Comp.I_ObjectAddress, c_BroadCast, SCHM_PrimitiveRegistration, RRe.I_ObjectAddress);
    end;
    Ply := SchServer.SchObjectFactory(ePolyline, eCreate_Default);
    if Ply <> nil then
    begin
        Ply.LineWidth := eSmall;
        Ply.ClearAllVertices;
        Ply.InsertVertex(1);  Ply.Vertex[1] := Point(MilsToCoord(-100), MilsToCoord(-20));
        Ply.InsertVertex(2);  Ply.Vertex[2] := Point(MilsToCoord(-60),  MilsToCoord(10));
        Ply.InsertVertex(3);  Ply.Vertex[3] := Point(MilsToCoord(-20),  MilsToCoord(-20));
        Ply.GraphicallyLocked := True;
        Ply.OwnerPartId := 1;
        Ply.OwnerPartDisplayMode := Comp.DisplayMode;
        Comp.AddSchObject(Ply);
        SchServer.RobotManager.SendMessage(Comp.I_ObjectAddress, c_BroadCast, SCHM_PrimitiveRegistration, Ply.I_ObjectAddress);
    end;
    Pgn := SchServer.SchObjectFactory(ePolygon, eCreate_Default);
    if Pgn <> nil then
    begin
        Pgn.ClearAllVertices;
        Pgn.InsertVertex(1);  Pgn.Vertex[1] := Point(MilsToCoord(0),  MilsToCoord(-20));
        Pgn.InsertVertex(2);  Pgn.Vertex[2] := Point(MilsToCoord(60), MilsToCoord(-20));
        Pgn.InsertVertex(3);  Pgn.Vertex[3] := Point(MilsToCoord(60), MilsToCoord(10));
        Pgn.LineWidth := eSmall;
        Pgn.GraphicallyLocked := True;
        Pgn.OwnerPartId := 1;
        Pgn.OwnerPartDisplayMode := Comp.DisplayMode;
        Comp.AddSchObject(Pgn);
        SchServer.RobotManager.SendMessage(Comp.I_ObjectAddress, c_BroadCast, SCHM_PrimitiveRegistration, Pgn.I_ObjectAddress);
    end;
    Pwe := SchServer.SchObjectFactory(ePie, eCreate_Default);
    if Pwe <> nil then
    begin
        Pwe.Location   := Point(MilsToCoord(120), MilsToCoord(-20));
        Pwe.Radius     := MilsToCoord(25);
        Pwe.StartAngle := 0;
        Pwe.EndAngle   := 90;
        Pwe.LineWidth  := eSmall;
        Pwe.GraphicallyLocked := True;
        Pwe.OwnerPartId := 1;
        Pwe.OwnerPartDisplayMode := Comp.DisplayMode;
        Comp.AddSchObject(Pwe);
        SchServer.RobotManager.SendMessage(Comp.I_ObjectAddress, c_BroadCast, SCHM_PrimitiveRegistration, Pwe.I_ObjectAddress);
    end;
    Bez := SchServer.SchObjectFactory(eBezier, eCreate_Default);
    if Bez <> nil then
    begin
        Bez.LineWidth := eSmall;
        Bez.ClearAllVertices;
        Bez.InsertVertex(1);  Bez.SetState_Vertex(1, Point(MilsToCoord(-200), MilsToCoord(-80)));
        Bez.InsertVertex(2);  Bez.SetState_Vertex(2, Point(MilsToCoord(-160), MilsToCoord(-50)));
        Bez.InsertVertex(3);  Bez.SetState_Vertex(3, Point(MilsToCoord(-120), MilsToCoord(-50)));
        Bez.InsertVertex(4);  Bez.SetState_Vertex(4, Point(MilsToCoord(-80),  MilsToCoord(-80)));
        Bez.GraphicallyLocked := True;
        Bez.OwnerPartId := 1;
        Bez.OwnerPartDisplayMode := Comp.DisplayMode;
        Comp.AddSchObject(Bez);
        SchServer.RobotManager.SendMessage(Comp.I_ObjectAddress, c_BroadCast, SCHM_PrimitiveRegistration, Bez.I_ObjectAddress);
    end;
    Lbl := SchServer.SchObjectFactory(eLabel, eCreate_Default);
    if Lbl <> nil then
    begin
        Lbl.Location := Point(MilsToCoord(0), MilsToCoord(-80));
        Lbl.FontID   := 1;
        Lbl.Text     := 'LOCKED';
        Lbl.GraphicallyLocked := True;
        Lbl.OwnerPartId := 1;
        Lbl.OwnerPartDisplayMode := Comp.DisplayMode;
        Comp.AddSchObject(Lbl);
        SchServer.RobotManager.SendMessage(Comp.I_ObjectAddress, c_BroadCast, SCHM_PrimitiveRegistration, Lbl.I_ObjectAddress);
    end;
end;

{ One symbol per writing system, each carrying the script in every
  text-bearing field: the component name (which becomes the OLE storage
  name), the description, a pin name, a label and a parameter value.

  The list is broad because the project is used well beyond Latin scripts,
  but the PROTECTION comes from a handful of behaviours rather than from the
  count. Most entries are ordinary non-Latin1 BMP text taking one code unit
  per character and exercising the same path. The ones that genuinely differ:
  Arabic/Hebrew/Syriac/Thaana/N'Ko/Adlam run right to left; Devanagari,
  Tamil, Khmer and Myanmar carry combining marks, so a character is not a
  code point; Telugu and Sinhala use zero-width joiners; Thai has no word
  spacing; Vietnamese appears both precomposed and decomposed, which are
  different byte sequences for the same word; Mongolian is written
  vertically; and the last three sit beyond the BMP, so each character needs
  a SURROGATE PAIR in UTF-16 — the case most likely to break a length or
  index calculation, since a CFB storage name is UTF-16 and capped at 31
  code units. The ASCII entry is the control. }
procedure AddI18nSymbol(Lib : ISch_Lib; ARef : String; ADesc : String; AWord : String);
var Comp : ISch_Component;
begin
    Comp := NewSymbol(Lib, ARef, ADesc, 1);
    if Comp = nil then Exit;
    AddPinEx(Comp, -300, 0, 200, eRotate180, eElectricPassive, '1', AWord, True, True, False);
    AddLabel(Comp, -100, 60, AWord, eJustify_BottomLeft, eRotate0);
    AddParameter(Comp, 'Value', AWord, -100, -60, True, eJustify_BottomLeft, eRotate0);
end;

{ ---- SchLib authoring -------------------------------------------------------

  Build order step 1: PINS_ETYPE — one pin per PinElectricalType, the densest
  single-record coverage win. Expand to the remaining symbols (orient/vis/decor/
  swap, shapes, labels, params, multi-part, footprint models) over iterations. }
procedure GenerateSchLib;
var
    Lib  : ISch_Lib;
    Comp : ISch_Component;
    Doc  : IServerDocument;
begin
    Doc := CreateNewDocumentFromDocumentKind('SCHLIB');
    if Doc = nil then Exit;

    // GetCurrentSchDocument returns an ISch_Document that also implements ISch_Lib.
    Lib := SchServer.GetCurrentSchDocument;
    if Lib = nil then Exit;

    // REUSE Altium's auto-created default component (rename + author into it) so the
    // library has exactly ONE symbol. CurrentSchComponent returns that default right
    // after creation (UltraLibrarian's importer relies on the same); deleting it is
    // unreliable (RemoveSchObject is a no-op for the default). Fall back to creating a
    // component only if it is ever nil.
    Comp := Lib.CurrentSchComponent;
    if Comp = nil then
    begin
        Comp := SchServer.SchObjectFactory(eSchComponent, eCreate_Default);
        if Comp = nil then Exit;
        Lib.AddSchComponent(Comp);
        SchServer.RobotManager.SendMessage(Lib.I_ObjectAddress, c_BroadCast,
                                           SCHM_PrimitiveRegistration, Comp.I_ObjectAddress);
    end;

    Comp.LibReference         := 'PINS_ETYPE';
    Comp.Designator.Text      := 'U?';
    Comp.ComponentDescription := 'One pin per electrical type';
    Comp.PartCount            := 1;   // v0 omitted this -> our reader read part_count 0
    Comp.CurrentPartId        := 1;
    Comp.DisplayMode          := 0;

    // One pin per PinElectricalType (enum order: input, io, output, opencollector,
    // passive, hiz, openemitter, power), stacked 100 mils apart.
    AddPin(Comp,    0, eElectricInput,         '1', 'IN');
    AddPin(Comp, -100, eElectricIO,            '2', 'IO');
    AddPin(Comp, -200, eElectricOutput,        '3', 'OUT');
    AddPin(Comp, -300, eElectricOpenCollector, '4', 'OC');
    AddPin(Comp, -400, eElectricPassive,       '5', 'PAS');
    AddPin(Comp, -500, eElectricHiZ,           '6', 'HIZ');
    AddPin(Comp, -600, eElectricOpenEmitter,   '7', 'OE');
    AddPin(Comp, -700, eElectricPower,         '8', 'PWR');

    { ---- PINS_ORIENT — one pin per orientation (Tier A AddPinEx) ---- }
    try
        Comp := NewSymbol(Lib, 'PINS_ORIENT', 'One pin per orientation', 1);
        if Comp <> nil then
        begin
            AddPinEx(Comp, 0,    0, 200, eRotate0,   eElectricPassive, '1', 'R', True, True, False);
            AddPinEx(Comp, 0,  100, 200, eRotate90,  eElectricPassive, '2', 'U', True, True, False);
            AddPinEx(Comp, 0, -100, 200, eRotate180, eElectricPassive, '3', 'L', True, True, False);
            AddPinEx(Comp, 0, -200, 200, eRotate270, eElectricPassive, '4', 'D', True, True, False);
        end;
    except
    end;

    { ---- PINS_VIS — show/hide combinations (Tier A) ---- }
    try
        Comp := NewSymbol(Lib, 'PINS_VIS', 'Pin visibility combinations', 1);
        if Comp <> nil then
        begin
            AddPinEx(Comp, 0,    0, 200, eRotate180, eElectricPassive, '1', 'BOTH',  True,  True,  False);
            AddPinEx(Comp, 0, -100, 200, eRotate180, eElectricPassive, '2', 'NONLY', True,  False, False);
            AddPinEx(Comp, 0, -200, 200, eRotate180, eElectricPassive, '3', 'DONLY', False, True,  False);
            AddPinEx(Comp, 0, -300, 200, eRotate180, eElectricPassive, '4', 'HIDE',  True,  True,  True);
        end;
    except
    end;

    { ---- PINS_DECOR — clock / dot on each of the four IEEE decoration slots ---- }
    try
        Comp := NewSymbol(Lib, 'PINS_DECOR', 'Pin decoration symbols', 1);
        if Comp <> nil then
        begin
            { one pin per IEEE decoration slot, now that all four property names are confirmed
              (SInner->InnerEdge, SOuter->OuterEdge, SInside->Inner, SOutside->Outer) }
            AddPinDecor(Comp, 0,    0, 200, eRotate180, eElectricInput, '1', 'IECLK',  1,
                        eClock,    eNoSymbol, eNoSymbol, eNoSymbol);   { inner edge = clock }
            AddPinDecor(Comp, 0, -100, 200, eRotate180, eElectricInput, '2', 'OEDOT',  1,
                        eNoSymbol, eDot,      eNoSymbol, eNoSymbol);   { outer edge = dot }
            AddPinDecor(Comp, 0, -200, 200, eRotate180, eElectricInput, '3', 'INCLK',  1,
                        eNoSymbol, eNoSymbol, eClock,    eNoSymbol);   { inside = clock }
            AddPinDecor(Comp, 0, -300, 200, eRotate180, eElectricInput, '4', 'OUTDOT', 1,
                        eNoSymbol, eNoSymbol, eNoSymbol, eDot);        { outside = dot }
        end;
    except
    end;

    { ---- LINES — H / V / diagonal (Tier A) ---- }
    try
        Comp := NewSymbol(Lib, 'LINES', 'Lines: horizontal/vertical/diagonal', 1);
        if Comp <> nil then
        begin
            AddLine(Comp, 0, 0, 100,   0);
            AddLine(Comp, 0, 0,   0, 100);
            AddLine(Comp, 0, 0, 100, 100);
        end;
    except
    end;

    { ---- ARCS — full circle + quarter arc (Tier A) ---- }
    try
        Comp := NewSymbol(Lib, 'ARCS', 'Arcs: full circle + quarter', 1);
        if Comp <> nil then
        begin
            AddSchArc(Comp, 0, 0, 50, 0.0, 360.0);
            AddSchArc(Comp, 0, -200, 50, 0.0, 90.0);
        end;
    except
    end;

    { ---- POLYGONS — two filled polygon boxes (AddPolygonBox) ---- }
    try
        Comp := NewSymbol(Lib, 'POLYGONS', 'Filled polygon boxes', 1);
        if Comp <> nil then
        begin
            AddPolygonBox(Comp, -100, 0, 100, 100, $00B0FFFF);
            AddPolygonBox(Comp,  150, 0, 350, 100, $0000FF00);
        end;
    except
    end;

    { ---- RECTS — filled + unfilled rectangle (Tier A AddRect) ---- }
    try
        Comp := NewSymbol(Lib, 'RECTS', 'Rectangles: filled + unfilled', 1);
        if Comp <> nil then
        begin
            AddRect(Comp, -100, 0, 100, 100, True,  $0000FFFF);
            AddRect(Comp,  150, 0, 350, 100, False, $0000FFFF);
        end;
    except
    end;

    { ---- ROUNDRECTS — a filled rounded rectangle (AddRoundRect) ---- }
    try
        Comp := NewSymbol(Lib, 'ROUNDRECTS', 'Rounded rectangle', 1);
        if Comp <> nil then
        begin
            AddRoundRect(Comp, -100, 0, 100, 100, 20, 20, True, $0000FFFF);
        end;
    except
    end;

    { ---- ELLIPSES — a circle + an ellipse (Tier A AddEllipse) ---- }
    try
        Comp := NewSymbol(Lib, 'ELLIPSES', 'Ellipses: circle + ellipse', 1);
        if Comp <> nil then
        begin
            AddEllipse(Comp,   0, 0, 50, 50, True,  $0000FFFF);
            AddEllipse(Comp, 200, 0, 80, 40, False, $0000FFFF);
        end;
    except
    end;

    { ---- POLYLINES — an open 3-point polyline (Tier A AddPolyline3) ---- }
    try
        Comp := NewSymbol(Lib, 'POLYLINES', 'Open 3-point polyline', 1);
        if Comp <> nil then
        begin
            AddPolyline3(Comp, 0, 0, 100, 50, 0, 100);
        end;
    except
    end;

    { ---- LABELS — justifications + a rotation (Tier A) ---- }
    try
        Comp := NewSymbol(Lib, 'LABELS', 'Text labels: justify + rotate', 1);
        if Comp <> nil then
        begin
            AddLabel(Comp,   0, 100, 'LBL_BL',    eJustify_BottomLeft, eRotate0);
            AddLabel(Comp, 200, 100, 'LBL_TR',    eJustify_TopRight,   eRotate0);
            AddLabel(Comp, 100, 300, 'LBL_ROT90', eJustify_BottomLeft, eRotate90);
        end;
    except
    end;

    { ---- PARAMS — a visible + a hidden parameter (Tier A) ---- }
    try
        Comp := NewSymbol(Lib, 'PARAMS', 'Component parameters: visible + hidden', 1);
        if Comp <> nil then
        begin
            AddParameter(Comp, 'Value',   '10k',   50, 400, True,  eJustify_BottomLeft, eRotate0);
            AddParameter(Comp, 'Comment', '100nF', 50, 450, False, eJustify_BottomLeft, eRotate0);
        end;
    except
    end;

    { ---- DUALPART — 2 logical parts, 2 pins each (Tier A AddPinDecor for OwnerPartId) ---- }
    try
        Comp := NewSymbol(Lib, 'DUALPART', 'Dual-part test symbol', 2);
        if Comp <> nil then
        begin
            AddPinDecor(Comp, -300,  100, 150, eRotate0,   eElectricInput,  '1', 'INA',  1,
                        eNoSymbol, eNoSymbol, eNoSymbol, eNoSymbol);
            AddPinDecor(Comp,  300,    0, 150, eRotate180, eElectricOutput, '2', 'OUTA', 1,
                        eNoSymbol, eNoSymbol, eNoSymbol, eNoSymbol);
            AddPinDecor(Comp, -300,  100, 150, eRotate0,   eElectricInput,  '3', 'INB',  2,
                        eNoSymbol, eNoSymbol, eNoSymbol, eNoSymbol);
            AddPinDecor(Comp,  300,    0, 150, eRotate180, eElectricOutput, '4', 'OUTB', 2,
                        eNoSymbol, eNoSymbol, eNoSymbol, eNoSymbol);
        end;
    except
    end;

    { ---- EDGE — boundary-case pins: large coords, negative coords, a long name ---- }
    try
        Comp := NewSymbol(Lib, 'EDGE', 'Boundary-case pins', 1);
        if Comp <> nil then
        begin
            AddPinEx(Comp,  500,  300, 200, eRotate180, eElectricPassive, '1', 'BIG', True, True, False);
            AddPinEx(Comp, -500, -300, 200, eRotate180, eElectricPassive, '2', 'NEG', True, True, False);
            AddPinEx(Comp,    0,  200, 200, eRotate180, eElectricPassive, '3',
                     'VERY_LONG_PIN_NAME_0123456789ABCDEF', True, True, False);
        end;
    except
    end;

    { ======================================================================
      COVERAGE ENRICHMENT (scripts/samples/COVERAGE.md): exercise the non-default
      property values that the plain symbols above never set, so the Rust
      READ tests verify them against a REAL Altium file rather than only via a
      self-round-trip. Each symbol is in its own try/except: a runtime failure
      costs ONLY that symbol, the rest of the library still saves. An unresolved
      identifier is different — it is a COMPILE error and aborts the whole run,
      so check every property and enum name against the DelphiScript identifier
      table first (scripts/samples/COVERAGE.md gives the one-liner).
      ====================================================================== }

    { ---- SHAPESTYLE — non-default LineStyle lines + a transparent rectangle + a
      transparent polygon. LineStyle (line/rect), Transparent (rect/polygon) are
      VERIFIED against the AD24 object model. ---- }
    try
        Comp := NewSymbol(Lib, 'SHAPESTYLE', 'Non-default line style + transparent fills', 1);
        if Comp <> nil then
        begin
            AddLineStyled(Comp, -200, 0, 0, 0, eLineStyleDashed);    { dashed line }
            AddLineStyled(Comp, 0, 0, 200, 0, eLineStyleDotted);     { dotted line }
            AddRect(Comp, -100, -100, 100, -50, True, $00FFFF);      { solid yellow fill }
            AddRectTransparent(Comp, -100, 50, 100, 100);            { transparent rect }
            AddPolygonTransparent(Comp, -50, 120, 50, 170);          { transparent polygon }
            AddEllipseTransparent(Comp, 150, 100, 30, 20);           { transparent ellipse }
            AddRectStyled(Comp, 150, -100, 250, -50, eLineStyleDashed); { dashed rectangle - LineStyleExt }
            { RoundRect Transparent is NOT persisted by Altium on a lib round-rect
              (reads back False), so it is not authored here — honest coverage only. }
        end;
    except
    end;

    { ---- LOCKFLAGS — a rectangle with the universal display/lock flags set
      (GraphicallyLocked / Disabled / Dimmed — VERIFIED ISch_GraphicalObject). ---- }
    try
        Comp := NewSymbol(Lib, 'LOCKFLAGS', 'Graphically locked / disabled / dimmed shape', 1);
        if Comp <> nil then
        begin
            AddRectFlagged(Comp, -100, -50, 100, 50);
            AddPinLocked(Comp, -150, '1', 'LK');
        end;
    except
    end;

    { ---- LOCKFLAGS2 — GraphicallyLocked on every remaining shape type. ---- }
    try
        Comp := NewSymbol(Lib, 'LOCKFLAGS2', 'Graphically locked shapes', 1);
        if Comp <> nil then
            AddLockedShapes(Comp);
    except
    end;

    { ---- JUSTIFY — labels at BottomLeft / Center / TopRight + a rotation. The
      mid-row constant is eJustify_Center (value 4), NOT eJustify_CenterCenter. ---- }
    try
        Comp := NewSymbol(Lib, 'JUSTIFY', 'Label / parameter justification + rotation', 1);
        if Comp <> nil then
        begin
            AddLabel(Comp, -100,  100, 'BL',    eJustify_BottomLeft, eRotate0);
            AddLabel(Comp, -100,   50, 'CC',    eJustify_Center,     eRotate0);
            AddLabel(Comp, -100,    0, 'TR',    eJustify_TopRight,   eRotate0);
            AddLabel(Comp, -100,  -50, 'ROT90', eJustify_BottomLeft, eRotate90);
            AddParameter(Comp, 'Value', '1k', 100, 100, True,  eJustify_TopRight,   eRotate0);
            AddParameter(Comp, 'Tol',   '5%', 100,  50, False, eJustify_Center,     eRotate90);
        end;
    except
    end;

    { ---- FRACPINS — off-grid pins (PinFrac aux stream) + a pin with a non-default
      Symbol_LineWidth (PinSymbolLineWidth aux stream). ---- }
    try
        Comp := NewSymbol(Lib, 'FRACPINS', 'Off-grid pins + symbol line width', 1);
        if Comp <> nil then
        begin
            AddPinFractional(Comp, 5, 3, 200, eRotate180, eElectricPassive, '1', 'FRAC');
            AddPinFractional(Comp, 0, 97, 200, eRotate180, eElectricPassive, '2', 'FRAC2');
            AddPinLineWidth(Comp, 0, -100, 200, eRotate180, eElectricPassive, '3', 'WIDE', eLarge);
        end;
    except
    end;

    { ---- BEZIERSYM — a Bezier curve (not authored by any other symbol). ---- }
    try
        Comp := NewSymbol(Lib, 'BEZIERSYM', 'Bezier curve', 1);
        if Comp <> nil then
            AddBezier4(Comp, -100, 0, -50, 80, 50, 80, 100, 0);
    except
    end;

    { ---- PIESYM — a filled pie / circular sector (RECORD=9, newly implemented). ---- }
    try
        Comp := NewSymbol(Lib, 'PIESYM', 'Filled pie sector', 1);
        if Comp <> nil then
            AddPie(Comp, 0, 0, 50, 30.0, 210.0, $00FFFF);   { 30..210 deg wedge, yellow fill }
    except
    end;

    { ---- IMAGESYM — a linked image (RECORD=30, newly implemented). ---- }
    try
        Comp := NewSymbol(Lib, 'IMAGESYM', 'Linked image', 1);
        if Comp <> nil then
            AddImage(Comp, -50, -30, 50, 30, 'logo.bmp');   { 100x60 mil box linking logo.bmp }
    except
    end;

    { ---- TEXTFRAMESYM — a bordered multi-line text frame (RECORD=28, newly implemented). ---- }
    try
        Comp := NewSymbol(Lib, 'TEXTFRAMESYM', 'Text frame', 1);
        if Comp <> nil then
            AddTextFrame(Comp, -100, -50, 100, 50, 'Frame text');   { 200x100 mil box }
    except
    end;

    { ---- EMBIMGSYM — an EMBEDDED image whose bytes land in /Storage. ---- }
    try
        Comp := NewSymbol(Lib, 'EMBIMGSYM', 'Embedded image', 1);
        if Comp <> nil then
            AddImageEmbedded(Comp, -20, -20, 20, 20, OUT_DIR + 'embed.bmp');
    except
    end;

    { ---- SWAPPIN — a pin carrying SwapId_Pin/SwapId_Part + DefaultValue (batch 4a). ---- }
    try
        Comp := NewSymbol(Lib, 'SWAPPIN', 'Pin swap ids + default value', 1);
        if Comp <> nil then
            AddPinSwap(Comp, 0, '1', 'SWP');
    except
    end;

    { ---- FRACSHAPES — off-grid rectangle + arc exercising the shape *_Frac keys (batch 4a). ---- }
    try
        Comp := NewSymbol(Lib, 'FRACSHAPES', 'Off-grid shapes', 1);
        if Comp <> nil then
        begin
            AddRectFrac(Comp, -55, -25, 55, 25);
            AddSchArcFrac(Comp, 0, 0, 40);
        end;
    except
    end;

    { ---- DISPMODE — a two-display-mode symbol with a shape in each mode (batch 4b). ---- }
    try
        Comp := NewSymbol(Lib, 'DISPMODE', 'Alternate display mode', 1);
        if Comp <> nil then
        begin
            Comp.DisplayModeCount := 2;
            AddRectMode(Comp, -50, -25, 50, 25, 0);   { normal mode }
            AddRectMode(Comp, -60, -30, 60, 30, 1);   { first alternate (de-Morgan) mode }
            AddPinMode(Comp, -100, '1', 'M0', 0);     { normal-mode pin }
            AddPinMode(Comp, -200, '2', 'M1', 1);     { alternate-mode pin: the pin-record byte }
        end;
    except
    end;

    { ---- UNINAME — a symbol whose NAME is outside Windows-1252 (issue #323). ----

      Records are stored as Windows-1252, but a CFB storage name is UTF-16, so the
      component storage and the FileHeader's LibRef entry are written through
      different encodings. This is the ground truth for what Altium itself does:
      which storage name it picks, and whether it promotes LibRef / LibReference to
      a %UTF8% key. Chr(N) truncates modulo 256 (see TEXT_LONG's documented
      negative), so the name is a literal — this file is UTF-8. If DelphiScript
      mangles the literal, the generated sample shows that instead, which is also
      worth knowing. }
    try
        Comp := NewSymbol(Lib, 'Резистор', 'описание Ω', 1);
        if Comp <> nil then
            AddRect(Comp, -50, -25, 50, 25, False, $FFFFFF);
    except
    end;

    { ---- SHAPECOLOR — one of every shape type in a distinct non-black colour.
      Every other symbol authors black, which Altium omits as the default, so
      this is the only golden coverage of the shape parsers' colour arms. ---- }
    try
        Comp := NewSymbol(Lib, 'SHAPECOLOR', 'Non-default colour on every shape', 1);
        if Comp <> nil then
            AddColourShapes(Comp);
    except
    end;

    { ---- SHAPESTYLE2 — the styling properties no other symbol reaches. Only the
      polyline is authored today: it is the single unproven interface/property
      family this run risks, and an unresolved name would cost every other
      symbol in the run. The staged probes below follow one run at a time. ---- }
    try
        Comp := NewSymbol(Lib, 'SHAPESTYLE2', 'Remaining shape styling properties', 1);
        if Comp <> nil then
        begin
            AddPolylineStyled(Comp, -150, 80, -100, 130, -50, 80);
            AddPolylineTransparent(Comp, 40, 80, 90, 130, 140, 80);
            AddTextFrameStyled(Comp, 180, -40, 260, 60, 'FRAME2');
            AddRoundRectStyled(Comp, -150, 0, -50, 50);
            AddLabelFlagged(Comp, -150, -80, 'MIRRORED');
            AddParameterProps(Comp, 'Rating', '10V', 50, -80);
        end;
    except
    end;

    // ELLARC (batch 5): elliptical arcs (RECORD=11), on-grid and off-grid, so the
    // record kind has a golden at all — its display flags and _Frac keys had
    // nothing to be verified against.
    try
        Comp := NewSymbol(Lib, 'ELLARC', 'Elliptical arcs', 1);
        if Comp <> nil then
        begin
            AddEllipticalArc(Comp, -60, 0, 50, 30, 0, 270, False, False);
            AddEllipticalArc(Comp,  60, 0, 50, 30, 45, 315, True, True);
        end;
    except
    end;

    // IMPLCHAIN (batch 5): footprint model links (RECORD=44/45/46/48). Three
    // links: the current one with a datafile path, a plain non-current one,
    // and one flagged the way a UI-authored link is (IntegratedModel +
    // DatabaseModel), so the replay of that form has a golden.
    try
        Comp := NewSymbol(Lib, 'IMPLCHAIN', 'Footprint model links', 1);
        if Comp <> nil then
        begin
            AddPinEx(Comp, -300, 0, 200, eRotate180, eElectricPassive, '1', 'A', True, True, False);
            AddImplementation(Comp, 'SOIC-8', 'Narrow body', 'Footprints.PcbLib', True, False);
            AddImplementation(Comp, 'SOIC-8-WIDE', '', '', False, False);
            AddImplementation(Comp, 'DIP-8', 'Through-hole', '', False, True);
        end;
    except
    end;

    // IEEESYM (batch 6): IEEE symbols (RECORD=3) — a plain dot, a mirrored and
    // rotated clock, and a larger, coloured, locked active-low input — so the
    // record kind has a golden and its keys are Altium's, not this crate's.
    try
        Comp := NewSymbol(Lib, 'IEEESYM', 'IEEE symbols', 1);
        if Comp <> nil then
        begin
            AddIeeeSymbol(Comp, -100,  0, eDot,            100, False, eRotate0,   $000000, False);
            AddIeeeSymbol(Comp,    0,  0, eClock,          100, True,  eRotate90,  $000000, False);
            AddIeeeSymbol(Comp,  100,  0, eActiveLowInput, 200, False, eRotate0,   $FF0000, True);
        end;
    except
    end;

    // FRACSHAPES2 (batch 5): the off-grid shapes FRACSHAPES did not cover.
    try
        Comp := NewSymbol(Lib, 'FRACSHAPES2', 'Off-grid shapes, every other kind', 1);
        if Comp <> nil then
            AddFracShapes(Comp);
    except
    end;


    { Every word below reaches Altium as its UTF-8 bytes read through the
      machine's ANSI page (this file is clean UTF-8; the engine's strings are
      ANSI), and Altium stores that widened text faithfully. Five of them
      (_JV, _BN, _CR, _IU, _SB) hold bytes 0x8E or 0xA6, which Altium escapes
      in a record's plain value (a broken bar as 0x8E, a literal 0x8E
      doubled); the Rust reader takes each value from its %UTF8% twin and
      resolves all of them to the real words.

      DOCUMENTED NEGATIVE (AD24, 2026-08-16) -- two other constructions:
      1. Wide Chr() (Chr($A997) + ...): Chr truncates to its LOW BYTE, so
         every field degrades to byte garbage.
      2. UTF-8 byte Chr() (Chr($EA) + Chr($A6) + ...): storage names and
         SectionKeys come out byte-perfect, but every TEXT record re-encodes
         the byte-chars as UTF-8, double-widening the name.
      Do NOT retry Chr(). }
    { ---- I18N — one symbol per writing system, so a non-Latin name is a
      tested case rather than an assumption. See AddI18nSymbol for why the
      list is shaped the way it is. ---- }
    try
        AddI18nSymbol(Lib, 'Resistor_LA', 'Script: Latin, ASCII control', 'Resistor');
    except
    end;
    try
        AddI18nSymbol(Lib, 'Résistance_L1', 'Script: Latin-1 supplement, precomposed', 'Résistance');
    except
    end;
    try
        AddI18nSymbol(Lib, 'Điện trở_VI', 'Script: Latin, Vietnamese diacritic stacking (NFC)', 'Điện trở');
    except
    end;
    try
        AddI18nSymbol(Lib, 'Điện trở_VD', 'Script: Latin, Vietnamese decomposed (NFD)', 'Điện trở');
    except
    end;
    try
        AddI18nSymbol(Lib, 'Резистор_RU', 'Script: Cyrillic', 'Резистор');
    except
    end;
    try
        AddI18nSymbol(Lib, 'Αντίσταση_EL', 'Script: Greek', 'Αντίσταση');
    except
    end;
    try
        AddI18nSymbol(Lib, 'Դիմադրիչ_HY', 'Script: Armenian', 'Դիմադրիչ');
    except
    end;
    try
        AddI18nSymbol(Lib, 'რეზისტორი_KA', 'Script: Georgian', 'რეზისტორი');
    except
    end;
    try
        AddI18nSymbol(Lib, '电阻_ZH', 'Script: Han, simplified', '电阻');
    except
    end;
    try
        AddI18nSymbol(Lib, '電阻_TW', 'Script: Han, traditional', '電阻');
    except
    end;
    try
        AddI18nSymbol(Lib, '抵抗器カナ_JA', 'Script: Japanese, kanji and kana', '抵抗器カナ');
    except
    end;
    try
        AddI18nSymbol(Lib, '저항기_KO', 'Script: Hangul', '저항기');
    except
    end;
    try
        AddI18nSymbol(Lib, 'ㄉㄧㄢˋㄗㄨˇ_BO', 'Script: Bopomofo', 'ㄉㄧㄢˋㄗㄨˇ');
    except
    end;
    try
        AddI18nSymbol(Lib, 'مقاومة_AR', 'Script: Arabic, right to left', 'مقاومة');
    except
    end;
    try
        AddI18nSymbol(Lib, 'مقاومت_FA', 'Script: Arabic, Persian letters', 'مقاومت');
    except
    end;
    try
        AddI18nSymbol(Lib, 'נגד_HE', 'Script: Hebrew, right to left', 'נגד');
    except
    end;
    try
        AddI18nSymbol(Lib, 'ܣܘܪܝܝܐ_SY', 'Script: Syriac, right to left', 'ܣܘܪܝܝܐ');
    except
    end;
    try
        AddI18nSymbol(Lib, 'ދިވެހި_DV', 'Script: Thaana, right to left', 'ދިވެހި');
    except
    end;
    try
        AddI18nSymbol(Lib, 'ߒߞߏ_NK', 'Script: N''Ko, right to left', 'ߒߞߏ');
    except
    end;
    try
        AddI18nSymbol(Lib, 'प्रतिरोधक_HI', 'Script: Devanagari, combining marks', 'प्रतिरोधक');
    except
    end;
    try
        AddI18nSymbol(Lib, 'রোধক_BN', 'Script: Bengali', 'রোধক');
    except
    end;
    try
        AddI18nSymbol(Lib, 'ਰੋਧਕ_PA', 'Script: Gurmukhi', 'ਰੋਧਕ');
    except
    end;
    try
        AddI18nSymbol(Lib, 'અવરોધક_GU', 'Script: Gujarati', 'અવરોધક');
    except
    end;
    try
        AddI18nSymbol(Lib, 'ପ୍ରତିରୋଧକ_OR', 'Script: Odia', 'ପ୍ରତିରୋଧକ');
    except
    end;
    try
        AddI18nSymbol(Lib, 'மின்தடை_TA', 'Script: Tamil', 'மின்தடை');
    except
    end;
    try
        AddI18nSymbol(Lib, 'నిరోధకం_TE', 'Script: Telugu, zero-width joiner', 'నిరోధకం');
    except
    end;
    try
        AddI18nSymbol(Lib, 'ಪ್ರತಿರೋಧಕ_KN', 'Script: Kannada', 'ಪ್ರತಿರೋಧಕ');
    except
    end;
    try
        AddI18nSymbol(Lib, 'പ്രതിരോധകം_ML', 'Script: Malayalam', 'പ്രതിരോധകം');
    except
    end;
    try
        AddI18nSymbol(Lib, 'ප්‍රතිරෝධකය_SI', 'Script: Sinhala', 'ප්‍රතිරෝධකය');
    except
    end;
    try
        AddI18nSymbol(Lib, 'ᱚᱞ ᱪᱤᱠᱤ_SA', 'Script: Ol Chiki, Santali', 'ᱚᱞ ᱪᱤᱠᱤ');
    except
    end;
    try
        AddI18nSymbol(Lib, 'ꯃꯤꯇꯩ_MN', 'Script: Meetei Mayek, Manipuri', 'ꯃꯤꯇꯩ');
    except
    end;
    try
        AddI18nSymbol(Lib, 'ตัวต้านทาน_TH', 'Script: Thai, no word spacing', 'ตัวต้านทาน');
    except
    end;
    try
        AddI18nSymbol(Lib, 'ຕົວຕ້ານທານ_LO', 'Script: Lao', 'ຕົວຕ້ານທານ');
    except
    end;
    try
        AddI18nSymbol(Lib, 'រេស៊ីស្ទ័រ_KM', 'Script: Khmer, stacked consonants', 'រេស៊ីស្ទ័រ');
    except
    end;
    try
        AddI18nSymbol(Lib, 'လျှပ်ခုခံ_MY', 'Script: Myanmar', 'လျှပ်ခုခံ');
    except
    end;
    try
        AddI18nSymbol(Lib, 'ꦗꦮ_JV', 'Script: Javanese', 'ꦗꦮ');
    except
    end;
    try
        AddI18nSymbol(Lib, 'ᬩᬮᬶ_BA', 'Script: Balinese', 'ᬩᬮᬶ');
    except
    end;
    try
        AddI18nSymbol(Lib, 'ᮞᮥᮔ᮪ᮓ_SU', 'Script: Sundanese', 'ᮞᮥᮔ᮪ᮓ');
    except
    end;
    try
        AddI18nSymbol(Lib, 'ᨅᨔ_BU', 'Script: Buginese, Lontara', 'ᨅᨔ');
    except
    end;
    try
        AddI18nSymbol(Lib, 'ᜊᜌ᜔ᜊᜌᜒᜈ᜔_TL', 'Script: Tagalog, Baybayin', 'ᜊᜌ᜔ᜊᜌᜒᜈ᜔');
    except
    end;
    try
        AddI18nSymbol(Lib, 'ꨌꨩꩌ_CH', 'Script: Cham', 'ꨌꨩꩌ');
    except
    end;
    try
        AddI18nSymbol(Lib, 'བོད་ཡིག_BD', 'Script: Tibetan', 'བོད་ཡིག');
    except
    end;
    try
        AddI18nSymbol(Lib, 'ᠮᠣᠩᠭᠣᠯ_MO', 'Script: Mongolian, written vertically', 'ᠮᠣᠩᠭᠣᠯ');
    except
    end;
    try
        AddI18nSymbol(Lib, 'አማርኛ_AM', 'Script: Ethiopic, Amharic', 'አማርኛ');
    except
    end;
    try
        AddI18nSymbol(Lib, 'ⵜⵉⴼⵉⵏⴰⵖ_TI', 'Script: Tifinagh, Berber', 'ⵜⵉⴼⵉⵏⴰⵖ');
    except
    end;
    try
        AddI18nSymbol(Lib, 'ꕙꔤ_VA', 'Script: Vai', 'ꕙꔤ');
    except
    end;
    try
        AddI18nSymbol(Lib, 'ꆈꌠ_YI', 'Script: Yi', 'ꆈꌠ');
    except
    end;
    try
        AddI18nSymbol(Lib, 'ᏣᎳᎩ_CR', 'Script: Cherokee', 'ᏣᎳᎩ');
    except
    end;
    try
        AddI18nSymbol(Lib, 'ᐃᓄᒃᑎᑐᑦ_IU', 'Script: Canadian Aboriginal Syllabics, Inuktitut', 'ᐃᓄᒃᑎᑐᑦ');
    except
    end;
    try
        AddI18nSymbol(Lib, 'ⲕⲟⲡⲧⲓⲕⲟⲛ_CO', 'Script: Coptic', 'ⲕⲟⲡⲧⲓⲕⲟⲛ');
    except
    end;
    try
        AddI18nSymbol(Lib, '𠮷野_SB', 'Script: Han beyond the BMP: surrogate pair', '𠮷野');
    except
    end;
    try
        AddI18nSymbol(Lib, '𞤀𞤣𞤤𞤢𞤥_AD', 'Script: Adlam beyond the BMP, right to left', '𞤀𞤣𞤤𞤢𞤥');
    except
    end;
    try
        AddI18nSymbol(Lib, '𐒰𐓑𐓘_OS', 'Script: Osage beyond the BMP', '𐒰𐓑𐓘');
    except
    end;

    Lib.CurrentSchComponent := Comp;
    Lib.GraphicallyInvalidate;
    // IServerDocument has no DoFileSaveAs; use DoSafeChangeFileNameAndSave.
    Doc.SetModified(True);
    Doc.DoSafeChangeFileNameAndSave(OUT_DIR + 'symbols.SchLib', 'SCHLIB');
end;

{ Opens a library previously saved to the bridge dir and resaves it through
  Altium's own reader and writer, touching no string literals at all.

  DOCUMENTED NEGATIVE (run 4, 2026-08-16): a resave does not turn a
  script's widened names into real text. Altium holds the widened form, and
  the resaved records came out as yet another variant of it (replacement
  characters included) — so the repo never re-opens a golden in AD, and real
  text is authored in the AD UI instead (manual/i18n5.SchLib), where input
  goes straight to a wide string. }
procedure ResaveRun;
var
    Doc : IServerDocument;
begin
    try
        Doc := Client.OpenDocument('SCHLIB', OUT_DIR + 'resave_input.SchLib');
        if Doc = nil then
        begin
            WriteResponse('error', 'OpenDocument returned nil for resave_input.SchLib');
            Exit;
        end;
        Client.ShowDocument(Doc);
        Doc.SetModified(True);
        Doc.DoSafeChangeFileNameAndSave(OUT_DIR + 'resave_output.SchLib', 'SCHLIB');
        WriteResponse('ok', 'resaved SchLib through Altium reader+writer');
    except
        WriteResponse('error', 'exception during resave (see Altium)');
    end;
end;

procedure Run;
begin
    if not DirectoryExists(OUT_DIR) then ForceDirectories(OUT_DIR);
    try
        GeneratePcbLib;
        GenerateSchLib;
        WriteResponse('ok', 'generated PADS.PcbLib + SYMBOLS.SchLib');
    except
        WriteResponse('error', 'exception during generation (see Altium)');
    end;
end;
