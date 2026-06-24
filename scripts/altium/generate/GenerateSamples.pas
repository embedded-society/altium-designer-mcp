{ GenerateSamples.pas — on-site golden-library authoring for altium-designer-mcp.

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
  generate -> read the golden back with the Rust tests -> add the next feature /
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
    Client.OpenNewDocumentOfKind('PCBLIB');
    Lib := PCBServer.GetCurrentPCBLibrary;
    if Lib = nil then Exit;

    Comp := PCBServer.CreatePCBLibComp;
    Comp.Name := 'PAD_SMD';

    Pad := PCBServer.PCBObjectFactory(ePadObject, eNoDimension, eCreate_Default);
    Pad.X        := MMsToCoord(0.0);
    Pad.Y        := MMsToCoord(0.0);
    Pad.TopXSize := MMsToCoord(1.0);
    Pad.TopYSize := MMsToCoord(0.6);
    Pad.Layer    := eTopLayer;
    Pad.Name     := '1';
    Comp.AddPCBObject(Pad);

    // TODO(iterate): one footprint per feature — non-round holes (slot/square),
    //   corner-radius / oblong pads (the 651-byte size/shape cases), tracks, arcs,
    //   regions (incl. holes), text, vias, fills, ComponentBody 3D models.

    Lib.RegisterComponent(Comp);
    Lib.Board.ViewManager_FullUpdate;

    Doc := Client.GetCurrentDocument;
    if Doc <> nil then Doc.DoFileSaveAs(OUT_DIR + 'PADS.PcbLib', True);
end;

{ ---- SchLib authoring -------------------------------------------------------

  v0 seed: one symbol with a couple of pins, saved to OUT_DIR. Expand to cover
  the per-record features (pins, rectangles, lines, arcs, labels, parameters,
  designator placement, multi-part symbols, footprint models) over iterations. }
procedure GenerateSchLib;
var
    Lib  : ISch_Lib;
    Comp : ISch_Component;
    Pin  : ISch_Pin;
    Doc  : IServerDocument;
begin
    Client.OpenNewDocumentOfKind('SCHLIB');
    Lib := SchServer.GetCurrentSchDocument;
    if Lib = nil then Exit;

    Comp := SchServer.SchObjectFactory(eSchComponent, eCreate_Default);
    Comp.LibReference := 'SYM_BASIC';
    Comp.Designator.Text := 'U?';

    Pin := SchServer.SchObjectFactory(ePin, eCreate_Default);
    Pin.Designator := '1';
    Pin.Name       := 'A';
    Pin.Location    := Point(MilsToCoord(0), MilsToCoord(0));
    Comp.AddSchObject(Pin);

    // TODO(iterate): rectangles, lines, arcs, labels, parameters, multi-part
    //   symbols, footprint model references, designator placement.

    Lib.AddSchComponent(Comp);

    Doc := Client.GetCurrentDocument;
    if Doc <> nil then Doc.DoFileSaveAs(OUT_DIR + 'SYMBOLS.SchLib', True);
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
