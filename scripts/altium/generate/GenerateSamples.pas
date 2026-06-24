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

{ ---- PcbLib authoring -------------------------------------------------------

  Build order: PAD_SHAPES (four SMD pads, one per TShape) + PAD_HOLES (through-hole
  pads, one per hole shape — non-round holes exercise the 651-byte size/shape block).
  Expand to stacks, vias, tracks, arcs, regions, text, fills and 3D bodies later. }
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
    Doc.DoSafeChangeFileNameAndSave(OUT_DIR + 'pads.PcbLib', 'PCBLIB');
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
