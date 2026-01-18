#!/usr/bin/env python3
"""
Analyze Altium PcbLib binary format.

This script reads a .PcbLib file and dumps its structure to help
reverse-engineer the binary format for primitives (pads, tracks, arcs, etc.).

Usage:
    python analyze_pcblib.py <path_to_pcblib>

Requirements:
    pip install olefile
"""

import sys
import struct
from pathlib import Path

try:
    import olefile
except ImportError:
    print("Error: olefile not installed. Run: pip install olefile")
    sys.exit(1)


def hex_dump(data: bytes, offset: int = 0, length: int = 256) -> str:
    """Format bytes as hex dump with ASCII."""
    lines = []
    for i in range(0, min(len(data), length), 16):
        hex_part = " ".join(f"{b:02x}" for b in data[i : i + 16])
        ascii_part = "".join(
            chr(b) if 32 <= b < 127 else "." for b in data[i : i + 16]
        )
        lines.append(f"  {offset + i:04x}: {hex_part:<48} |{ascii_part}|")
    return "\n".join(lines)


def parse_string_block(data: bytes, offset: int) -> tuple[str, int]:
    """Parse a length-prefixed string block."""
    if offset + 4 > len(data):
        return "", offset

    block_len = struct.unpack_from("<I", data, offset)[0]
    if offset + 4 + block_len > len(data):
        return "", offset

    # Inside the block: [str_len:1][string:str_len]
    if block_len > 0:
        str_len = data[offset + 4]
        if str_len + 5 <= block_len + 4:
            try:
                s = data[offset + 5 : offset + 5 + str_len].decode("ascii")
                return s, offset + 4 + block_len
            except Exception:
                pass

    return "", offset + 4 + block_len


def parse_footprint_data(name: str, data: bytes):
    """Parse footprint Data stream.

    Format (based on pyAltiumLib):
    1. Name block: [block_len:4][str_len:1][name:str_len]
    2. Primitives: [record_type:1][blocks...] until record_type=0
    """
    print(f"\n{'='*60}")
    print(f"Footprint: {name}")
    print(f"Data size: {len(data)} bytes")
    print(f"{'='*60}")

    if len(data) < 5:
        print("  Data too short")
        return

    # Name block: [block_len:4][str_len:1][component_name:str_len]
    block_len = struct.unpack_from("<I", data, 0)[0]
    print(f"\nName block length: {block_len}")

    if block_len + 4 > len(data):
        print("  Invalid block length!")
        return

    # Parse component name from within the block
    name_len = data[4]
    comp_name = data[5:5 + name_len].decode("ascii", errors="replace")
    print(f"Component name: {comp_name} ({name_len} chars)")

    # After name block, primitives start (no separate count - just loop until RecordID=0)
    offset = 4 + block_len
    print(f"\nPrimitives start at offset 0x{offset:04x}")

    if offset >= len(data):
        return

    # Show raw bytes around the record area
    print(f"\nRaw bytes at primitive start:")
    print(hex_dump(data[offset:], offset, 128))

    # Parse primitives until RecordID = 0
    print(f"\nParsing primitives:")
    prim_num = 0

    while offset < len(data):
        record_type = data[offset]

        if record_type == 0:
            print(f"\n[END] RecordID=0 at offset 0x{offset:04x} - stream complete")
            break

        record_names = {
            0x01: "Arc",
            0x02: "Pad",
            0x03: "Via",
            0x04: "Track",
            0x05: "Text",
            0x06: "Fill",
            0x0B: "Region",
            0x0C: "ComponentBody",
        }
        rname = record_names.get(record_type, f"Unknown(0x{record_type:02x})")
        print(f"\n[{prim_num}] {rname} (type 0x{record_type:02x}) at offset 0x{offset:04x}")
        offset += 1

        # Parse blocks for this primitive
        if record_type == 0x02:  # Pad
            offset = parse_pad_blocks_v2(data, offset)
        elif record_type == 0x04:  # Track
            offset = parse_track_blocks(data, offset)
        elif record_type == 0x01:  # Arc
            offset = parse_arc_blocks(data, offset)
        elif record_type == 0x05:  # Text
            offset = parse_text_blocks(data, offset)
        elif record_type == 0x0B:  # Region
            offset = parse_region_blocks(data, offset)
        elif record_type == 0x06:  # Fill
            offset = parse_fill_blocks(data, offset)
        elif record_type == 0x0C:  # ComponentBody
            offset = parse_component_body_blocks(data, offset)
        else:
            # Skip unknown primitive - try to find next record type
            print(f"    Skipping unknown primitive, raw bytes:")
            print(hex_dump(data[offset:], offset, 64))
            break

        prim_num += 1

    print(f"\nFinal offset: 0x{offset:04x}")
    print(f"Remaining bytes: {len(data) - offset}")


def read_block(data: bytes, offset: int) -> tuple[bytes, int]:
    """Read a length-prefixed block and return (block_data, new_offset)."""
    if offset + 4 > len(data):
        return b"", offset
    block_len = struct.unpack_from("<I", data, offset)[0]
    if block_len > 100000 or offset + 4 + block_len > len(data):
        return b"", offset
    block_data = data[offset + 4 : offset + 4 + block_len]
    return block_data, offset + 4 + block_len


def read_string_from_block(block_data: bytes) -> str:
    """Read a length-prefixed string from block data."""
    if len(block_data) < 1:
        return ""
    str_len = block_data[0]
    if str_len + 1 > len(block_data):
        return ""
    try:
        return block_data[1:1 + str_len].decode("windows-1252")
    except Exception:
        return ""


def parse_pad_blocks_v2(data: bytes, offset: int) -> int:
    """Parse pad primitive blocks using pyAltiumLib format.

    Pad structure (6 blocks):
    - Block 0: Designator string [len:4][str_len:1][designator]
    - Block 1: Unknown (skipped)
    - Block 2: Unknown string ("|&|0") [len:4][str_len:1][string]
    - Block 3: Unknown (skipped)
    - Block 4: Geometry data (coordinates, sizes, shapes)
    - Block 5: Per-layer data (optional)
    """
    # Block 0: Designator
    block0, offset = read_block(data, offset)
    designator = read_string_from_block(block0)
    print(f"    Designator: '{designator}'")

    # Block 1: Unknown
    block1, offset = read_block(data, offset)
    print(f"    Block 1 (unknown): {len(block1)} bytes")

    # Block 2: Unknown string (usually "|&|0")
    block2, offset = read_block(data, offset)
    block2_str = read_string_from_block(block2)
    print(f"    Block 2 (string): '{block2_str}'")

    # Block 3: Unknown
    block3, offset = read_block(data, offset)
    print(f"    Block 3 (unknown): {len(block3)} bytes")

    # Block 4: Geometry data
    block4, offset = read_block(data, offset)
    print(f"    Block 4 (geometry): {len(block4)} bytes")
    if len(block4) >= 45:
        parse_pad_geometry(block4)

    # Block 5: Per-layer data (optional)
    block5, offset = read_block(data, offset)
    if len(block5) > 0:
        print(f"    Block 5 (layer data): {len(block5)} bytes")

    return offset


def parse_pad_geometry(data: bytes):
    """Parse pad geometry block to extract coordinates and dimensions."""
    if len(data) < 45:
        print("      (geometry block too short)")
        return

    # First 13 bytes: common header
    layer = data[0]
    print(f"      Layer: {layer}")

    # After 13-byte header: coordinates and dimensions
    # Location (X, Y) as signed 32-bit integers
    loc_x = struct.unpack_from("<i", data, 13)[0]
    loc_y = struct.unpack_from("<i", data, 17)[0]

    # Size top (X, Y)
    size_top_x = struct.unpack_from("<i", data, 21)[0]
    size_top_y = struct.unpack_from("<i", data, 25)[0]

    # Size middle (X, Y)
    size_mid_x = struct.unpack_from("<i", data, 29)[0]
    size_mid_y = struct.unpack_from("<i", data, 33)[0]

    # Size bottom (X, Y)
    size_bot_x = struct.unpack_from("<i", data, 37)[0]
    size_bot_y = struct.unpack_from("<i", data, 41)[0]

    # Hole size
    hole_size = struct.unpack_from("<i", data, 45)[0] if len(data) > 48 else 0

    # Convert from internal units to mm
    # Altium internal units: 10000 = 1 mil, so 1mm = 10000/0.0254 = 393700.787
    def to_mm(val):
        return val / 10000.0 * 0.0254

    print(f"      Location: ({to_mm(loc_x):.4f}, {to_mm(loc_y):.4f}) mm")
    print(f"      Size Top: ({to_mm(size_top_x):.4f} x {to_mm(size_top_y):.4f}) mm")
    print(f"      Size Mid: ({to_mm(size_mid_x):.4f} x {to_mm(size_mid_y):.4f}) mm")
    print(f"      Size Bot: ({to_mm(size_bot_x):.4f} x {to_mm(size_bot_y):.4f}) mm")
    if hole_size > 0:
        print(f"      Hole: {to_mm(hole_size):.4f} mm")

    # Shapes (after hole size)
    if len(data) > 51:
        shape_top = data[49]
        shape_mid = data[50]
        shape_bot = data[51]
        shape_names = {1: "Round", 2: "Rectangular", 3: "Octagon"}
        print(f"      Shapes: Top={shape_names.get(shape_top, shape_top)}, "
              f"Mid={shape_names.get(shape_mid, shape_mid)}, "
              f"Bot={shape_names.get(shape_bot, shape_bot)}")

    # Rotation (8-byte double after shapes)
    if len(data) > 59:
        rotation = struct.unpack_from("<d", data, 52)[0]
        print(f"      Rotation: {rotation:.2f} deg")


def parse_track_blocks(data: bytes, offset: int) -> int:
    """Parse track primitive blocks and return new offset."""
    # Track has a single block with geometry data
    block, offset = read_block(data, offset)
    print(f"    Track block: {len(block)} bytes")

    if len(block) >= 45:
        # First 13 bytes: common header
        layer = block[0]
        print(f"      Layer: {layer}")

        # Coordinates as signed 32-bit integers
        x1 = struct.unpack_from("<i", block, 13)[0]
        y1 = struct.unpack_from("<i", block, 17)[0]
        x2 = struct.unpack_from("<i", block, 21)[0]
        y2 = struct.unpack_from("<i", block, 25)[0]
        width = struct.unpack_from("<i", block, 29)[0]

        def to_mm(val):
            return val / 10000.0 * 0.0254

        print(f"      Start: ({to_mm(x1):.4f}, {to_mm(y1):.4f}) mm")
        print(f"      End: ({to_mm(x2):.4f}, {to_mm(y2):.4f}) mm")
        print(f"      Width: {to_mm(width):.4f} mm")
    else:
        preview = " ".join(f"{b:02x}" for b in block[:min(48, len(block))])
        print(f"      Raw: {preview}")

    return offset


def parse_arc_blocks(data: bytes, offset: int) -> int:
    """Parse arc primitive blocks and return new offset."""
    # Arc has a single block with geometry data
    block, offset = read_block(data, offset)
    print(f"    Arc block: {len(block)} bytes")

    if len(block) >= 45:
        layer = block[0]
        print(f"      Layer: {layer}")

        # Center coordinates
        cx = struct.unpack_from("<i", block, 13)[0]
        cy = struct.unpack_from("<i", block, 17)[0]
        radius = struct.unpack_from("<i", block, 21)[0]

        def to_mm(val):
            return val / 10000.0 * 0.0254

        print(f"      Center: ({to_mm(cx):.4f}, {to_mm(cy):.4f}) mm")
        print(f"      Radius: {to_mm(radius):.4f} mm")

        # Angles as doubles
        if len(block) >= 45:
            start_angle = struct.unpack_from("<d", block, 25)[0]
            end_angle = struct.unpack_from("<d", block, 33)[0]
            print(f"      Angles: {start_angle:.2f} to {end_angle:.2f} deg")
    else:
        preview = " ".join(f"{b:02x}" for b in block[:min(48, len(block))])
        print(f"      Raw: {preview}")

    return offset


def parse_text_blocks(data: bytes, offset: int) -> int:
    """Parse text primitive blocks (2 blocks)."""
    # Block 0: Geometry
    block0, offset = read_block(data, offset)
    print(f"    Text geometry block: {len(block0)} bytes")

    if len(block0) >= 25:
        layer = block0[0]
        print(f"      Layer: {layer}")

        def to_mm(val):
            return val / 10000.0 * 0.0254

        x = struct.unpack_from("<i", block0, 13)[0]
        y = struct.unpack_from("<i", block0, 17)[0]
        height = struct.unpack_from("<i", block0, 21)[0]
        print(f"      Position: ({to_mm(x):.4f}, {to_mm(y):.4f}) mm")
        print(f"      Height: {to_mm(height):.4f} mm")

        if len(block0) > 35:
            rotation = struct.unpack_from("<d", block0, 27)[0]
            print(f"      Rotation: {rotation:.2f} deg")

    # Block 1: Text content
    block1, offset = read_block(data, offset)
    content = read_string_from_block(block1)
    print(f"    Text content block: {len(block1)} bytes, content: '{content}'")

    return offset


def parse_region_blocks(data: bytes, offset: int) -> int:
    """Parse region primitive blocks (2 blocks)."""
    # Block 0: Properties
    block0, offset = read_block(data, offset)
    print(f"    Region properties block: {len(block0)} bytes")
    print(f"      Raw hex: {' '.join(f'{b:02x}' for b in block0[:min(64, len(block0))])}")

    if len(block0) >= 13:
        layer = block0[0]
        print(f"      Layer: {layer}")

    # Block 1: Vertices
    block1, offset = read_block(data, offset)
    print(f"    Region vertices block: {len(block1)} bytes")
    print(f"      Raw hex: {' '.join(f'{b:02x}' for b in block1[:min(128, len(block1))])}")

    if len(block1) >= 4:
        vertex_count = struct.unpack_from("<I", block1, 0)[0]
        print(f"      Vertex count: {vertex_count}")

        def to_mm(val):
            return val / 10000.0 * 0.0254

        # Each vertex is 2 doubles (16 bytes)
        for i in range(vertex_count):
            base = 4 + i * 16
            if base + 16 <= len(block1):
                x = struct.unpack_from("<d", block1, base)[0]
                y = struct.unpack_from("<d", block1, base + 8)[0]
                print(f"      Vertex[{i}]: ({to_mm(int(x)):.4f}, {to_mm(int(y)):.4f}) mm  [raw: x={x}, y={y}]")

    return offset


def parse_fill_blocks(data: bytes, offset: int) -> int:
    """Parse fill primitive blocks (1 block)."""
    block, offset = read_block(data, offset)
    print(f"    Fill block: {len(block)} bytes")

    if len(block) >= 37:
        layer = block[0]
        print(f"      Layer: {layer}")

        def to_mm(val):
            return val / 10000.0 * 0.0254

        x1 = struct.unpack_from("<i", block, 13)[0]
        y1 = struct.unpack_from("<i", block, 17)[0]
        x2 = struct.unpack_from("<i", block, 21)[0]
        y2 = struct.unpack_from("<i", block, 25)[0]
        print(f"      Corner 1: ({to_mm(x1):.4f}, {to_mm(y1):.4f}) mm")
        print(f"      Corner 2: ({to_mm(x2):.4f}, {to_mm(y2):.4f}) mm")

        if len(block) > 36:
            rotation = struct.unpack_from("<d", block, 29)[0]
            print(f"      Rotation: {rotation:.2f} deg")

    return offset


def parse_component_body_params(block: bytes) -> dict:
    """Parse ComponentBody parameter string from block 0."""
    params = {}
    try:
        block_str = block.decode('latin1')
        # Find V7_LAYER which starts the parameter string
        v7_idx = block_str.find('V7_LAYER')
        if v7_idx >= 0:
            params_str = block_str[v7_idx:]
            for pair in params_str.split('|'):
                if '=' in pair:
                    key, val = pair.split('=', 1)
                    # Clean up null bytes
                    val = val.rstrip('\x00')
                    params[key] = val
    except Exception:
        pass
    return params


def parse_component_body_blocks(data: bytes, offset: int) -> int:
    """Parse component body primitive blocks (3 blocks)."""
    for i in range(3):
        block, offset = read_block(data, offset)
        print(f"    ComponentBody block {i}: {len(block)} bytes")

        if i == 0 and len(block) > 40:
            params = parse_component_body_params(block)
            if params:
                print("      Key parameters:")
                for key in ['MODELID', 'MODEL.NAME', 'MODEL.EMBED',
                           'MODEL.3D.ROTX', 'MODEL.3D.ROTY', 'MODEL.3D.ROTZ',
                           'MODEL.3D.DZ', 'STANDOFFHEIGHT', 'OVERALLHEIGHT']:
                    if key in params:
                        print(f"        {key}: {params[key]}")

        if len(block) > 0 and len(block) < 100:
            preview = " ".join(f"{b:02x}" for b in block[:min(32, len(block))])
            print(f"      Raw: {preview}")

    return offset


def analyze_pcblib(filepath: Path):
    """Analyze a PcbLib file."""
    print(f"\n{'#'*70}")
    print(f"# Analyzing: {filepath}")
    print(f"{'#'*70}")

    ole = olefile.OleFileIO(str(filepath))

    # List all streams
    print("\nOLE Structure:")
    for entry in ole.listdir():
        path = "/".join(entry)
        size = ole.get_size(entry)
        print(f"  /{path} ({size} bytes)")

    # Read FileHeader
    if ole.exists("FileHeader"):
        data = ole.openstream("FileHeader").read()
        print(f"\nFileHeader ({len(data)} bytes):")
        print(f"  {data[:100]}")

    # Find and analyze footprint storages
    footprint_count = 0
    for entry in ole.listdir():
        # Look for entries with Data and Parameters
        if len(entry) == 2 and entry[1] == "Data":
            storage_name = entry[0]

            # Skip Library and other special storages
            if storage_name in ["Library", "FileVersionInfo"]:
                continue

            # Check if it has Parameters (indicates footprint)
            params_entry = [storage_name, "Parameters"]
            if ole.exists(params_entry):
                # Read Parameters
                params_data = ole.openstream(params_entry).read()
                print(f"\n\nParameters for {storage_name}:")
                params_str = params_data.decode('ascii', errors='ignore')[:200]
                print(f"  {params_str}")

                # Read Data
                data = ole.openstream(entry).read()
                parse_footprint_data(storage_name, data)

                footprint_count += 1

                # Only analyze first few footprints
                if footprint_count >= 5:
                    print("\n... (more footprints exist)")
                    break

    ole.close()


def main():
    if len(sys.argv) < 2:
        # Default to sample file in scripts folder
        script_dir = Path(__file__).parent
        default_path = script_dir / "sample.PcbLib"
        if default_path.exists():
            analyze_pcblib(default_path)
        else:
            print(f"Usage: {sys.argv[0]} <path_to_pcblib>")
            print(f"  Or place a sample file at: {default_path}")
            sys.exit(1)
    else:
        filepath = Path(sys.argv[1])
        if not filepath.exists():
            print(f"Error: File not found: {filepath}")
            sys.exit(1)
        analyze_pcblib(filepath)


if __name__ == "__main__":
    main()
