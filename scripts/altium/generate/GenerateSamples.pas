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

{ ---- PcbLib authoring -------------------------------------------------------

  v0 seed: one footprint with a single SMD pad, saved to OUT_DIR. Expand the
  placement (pad shapes x hole types, tracks, arcs, regions, text, vias, fills,
  3D bodies — one footprint per feature) over iterations. }
procedure GeneratePcbLib;
var
    Lib  : IPCB_Library;
    Comp : IPCB_LibComponent;
    Pad  : IPCB_Pad;
    Doc  : IServerDocument;
begin
    // CreateNewDocumentFromDocumentKind is the global that creates + focuses a blank
    // document and returns its IServerDocument (Client.OpenNewDocumentOfKind, used in
    // the v0, does not exist in AD24).
    Doc := CreateNewDocumentFromDocumentKind('PCBLIB');
    if Doc = nil then Exit;

    Lib := PCBServer.GetCurrentPCBLibrary;   // the new doc is focused
    if Lib = nil then Exit;

    Comp := PCBServer.CreatePCBLibComp;
    Comp.Name := 'PAD_SMD';
    Lib.RegisterComponent(Comp);             // register before mutating

    // Primitive creation runs inside a Pre/PostProcess transaction, with a board-
    // registration broadcast so the editor registers each new object.
    PCBServer.PreProcess;

    Pad := PCBServer.PCBObjectFactory(ePadObject, eNoDimension, eCreate_Default);
    Pad.X        := MMsToCoord(0.0);
    Pad.Y        := MMsToCoord(0.0);
    Pad.TopXSize := MMsToCoord(1.0);
    Pad.TopYSize := MMsToCoord(0.6);
    Pad.Layer    := eTopLayer;               // SMD pad -> top copper only
    Pad.Name     := '1';
    Comp.AddPCBObject(Pad);
    // Altium's own constant is spelled PCBM_BoardRegisteration (the typo is real).
    PCBServer.SendMessageToRobots(Comp.I_ObjectAddress, c_Broadcast,
                                  PCBM_BoardRegisteration, Pad.I_ObjectAddress);

    PCBServer.PostProcess;

    // TODO(iterate): one footprint per feature — non-round holes (slot/square),
    //   corner-radius / oblong pads (the 651-byte size/shape cases), tracks, arcs,
    //   regions (incl. holes), text, vias, fills, ComponentBody 3D models.

    Lib.CurrentComponent := Comp;
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
