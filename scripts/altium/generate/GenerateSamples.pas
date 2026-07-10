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

{ ==== PcbLib COVERAGE-ENRICHMENT HELPERS (verified AD24 names) ============== }

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

{ ---- PcbLib authoring -------------------------------------------------------

  Footprints: PAD_SHAPES, PAD_HOLES, VIAS, TRACKS, ARCS, REGIONS, FILLS, TEXT_STROKE,
  TEXT_WIN1252. Each new footprint is wrapped in try/except so one failing primitive
  doesn't abort the whole script (a missing footprint then shows up as a failed read
  test). Blind/buried vias, stacks and 3D bodies follow in later batches. }
procedure GeneratePcbLib;
var
    Lib   : IPCB_Library;
    DefFP : IPCB_LibComponent;
    Comp  : IPCB_LibComponent;
    Doc   : IServerDocument;
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
  angle) and adds IsSolid + Transparent + AreaColor. }
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
      COVERAGE ENRICHMENT (docs/FIXTURE_COVERAGE.md): exercise the non-default
      property values that the plain symbols above never set, so the Rust
      READ tests verify them against a REAL Altium file rather than only via a
      self-round-trip. Each symbol is in its own try/except: an unverified AD24
      property name fails ONLY that symbol, the rest of the library still saves.
      Property names not already proven by a helper above are best-effort (from
      AltiumSharp DTOs); on-site failures are expected to be iterated.
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
            AddRectFlagged(Comp, -100, -50, 100, 50);
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

    Lib.CurrentSchComponent := Comp;
    Lib.GraphicallyInvalidate;
    // IServerDocument has no DoFileSaveAs; use DoSafeChangeFileNameAndSave.
    Doc.SetModified(True);
    Doc.DoSafeChangeFileNameAndSave(OUT_DIR + 'symbols.SchLib', 'SCHLIB');
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
