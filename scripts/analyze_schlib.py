#!/usr/bin/env python3
"""Analyze SchLib binary format for reverse engineering."""

import sys
from pathlib import Path
from io import BytesIO

try:
    import olefile
except ImportError:
    print("Please install olefile: pip install olefile")
    sys.exit(1)


def parse_properties(data: bytes) -> dict:
    """Parse pipe-delimited key=value properties."""
    props = {}
    try:
        text = data.decode('windows-1252')
        for part in text.split('|'):
            if '=' in part:
                key, value = part.split('=', 1)
                props[key] = value
    except Exception:
        pass
    return props


def parse_binary_pin(data: bytes) -> dict:
    """Parse binary pin record."""
    props = {}
    reader = BytesIO(data)

    def read_i32():
        return int.from_bytes(reader.read(4), 'little', signed=True)

    def read_i16(signed=False):
        return int.from_bytes(reader.read(2), 'little', signed=signed)

    def read_i8():
        return int.from_bytes(reader.read(1), 'little')

    props['RECORD'] = str(read_i32())
    reader.read(1)  # Unknown byte
    props['OwnerPartId'] = read_i16(signed=True)
    props['OwnerPartDisplayMode'] = read_i8()
    props['Symbol_InnerEdge'] = read_i8()
    props['Symbol_OuterEdge'] = read_i8()
    props['Symbol_Inside'] = read_i8()
    props['Symbol_Outside'] = read_i8()

    desc_len = read_i8()
    reader.read(1)  # Unknown byte
    props['Description'] = reader.read(desc_len).decode('utf-8') if desc_len else ''

    props['Electrical_Type'] = read_i8()

    flags = read_i8()
    props['Rotated'] = bool(flags & 0x01)
    props['Flipped'] = bool(flags & 0x02)
    props['Hide'] = bool(flags & 0x04)
    props['Show_Name'] = bool(flags & 0x08)
    props['Show_Designator'] = bool(flags & 0x10)

    props['Length'] = read_i16()
    props['Location.X'] = read_i16(signed=True)
    props['Location.Y'] = read_i16(signed=True)
    props['Color'] = read_i32()

    name_len = read_i8()
    props['Name'] = reader.read(name_len).decode('utf-8') if name_len else ''

    designator_len = read_i8()
    props['Designator'] = reader.read(designator_len).decode('utf-8') if designator_len else ''

    return props


def analyze_schlib(filepath: str):
    """Analyze a SchLib file."""
    ole = olefile.OleFileIO(filepath)

    print(f"=== Analyzing: {filepath} ===")
    print()

    # List all streams
    print("=== OLE Streams ===")
    for entry in ole.listdir():
        path = '/'.join(entry)
        size = ole.get_size(path)
        print(f"  {path}: {size} bytes")
    print()

    # Read FileHeader
    print("=== FileHeader ===")
    header_data = ole.openstream('FileHeader').read()
    # First 4 bytes are length
    length = int.from_bytes(header_data[:4], 'little')
    header_props = parse_properties(header_data[4:4+length])
    for key, value in sorted(header_props.items()):
        print(f"  {key} = {value}")
    print()

    # Get component count
    comp_count = int(header_props.get('CompCount', 0))
    print(f"Component count: {comp_count}")

    # Get component names from header
    comp_names = []
    for i in range(comp_count):
        name = header_props.get(f'LibRef{i}', f'Unknown{i}')
        comp_names.append(name)
        print(f"  Component {i}: {name}")
    print()

    # Analyze each component's Data stream
    for comp_name in comp_names:
        stream_path = f"{comp_name}/Data"
        print(f"=== Component: {comp_name} ===")

        try:
            data = ole.openstream(stream_path).read()
            print(f"Data stream size: {len(data)} bytes")
            print()

            # Parse records using SchLib format:
            # [RecordLength:2 LE][RecordType:2 BE][data:RecordLength]
            # RecordType 0 = text, RecordType 1 = binary pin
            offset = 0
            record_num = 0
            records_by_type = {}

            while offset < len(data) - 4:
                # Read header (4 bytes)
                record_length = int.from_bytes(data[offset:offset+2], 'little')
                record_type = int.from_bytes(data[offset+2:offset+4], 'big')

                if record_length == 0:
                    print(f"  [End marker at offset {offset:#x}]")
                    break

                record_data = data[offset+4:offset+4+record_length]

                if record_type == 0:
                    # Text record (pipe-delimited)
                    props = parse_properties(record_data)
                    rec_id = props.get('RECORD', 'unknown')
                elif record_type == 1:
                    # Binary pin record
                    props = parse_binary_pin(record_data)
                    rec_id = f"2-binary"  # Pin type is 2
                else:
                    props = {'_raw': record_data[:50].hex()}
                    rec_id = f'unknown-type-{record_type}'

                # Track record types
                if rec_id not in records_by_type:
                    records_by_type[rec_id] = []
                records_by_type[rec_id].append(props)

                # Print first few records in detail
                if record_num < 8:
                    print(f"  Record {record_num} (type={record_type}, RECORD={rec_id}, len={record_length}):")
                    for key, value in sorted(props.items())[:12]:
                        print(f"    {key} = {value}")
                    if len(props) > 12:
                        print(f"    ... and {len(props)-12} more properties")
                    print()

                offset += 4 + record_length
                record_num += 1

            print(f"  Total records: {record_num}")
            print()
            print("  Record type summary:")
            for rtype, records in sorted(records_by_type.items()):
                print(f"    RECORD={rtype}: {len(records)} records")

                # Show example properties for each type
                if records:
                    example = records[0]
                    interesting_keys = [k for k in example.keys()
                                       if k not in ['RECORD', 'IndexInSheet', 'OwnerPartId',
                                                   'OwnerPartDisplayMode', 'UniqueID', '_raw']]
                    if interesting_keys:
                        print(f"      Keys: {', '.join(interesting_keys[:10])}")

            print()

        except Exception as e:
            import traceback
            print(f"  Error reading Data stream: {e}")
            traceback.print_exc()
            print()

    ole.close()


def main():
    script_dir = Path(__file__).parent
    default_path = script_dir / "sample.SchLib"

    if len(sys.argv) > 1:
        filepath = sys.argv[1]
    elif default_path.exists():
        filepath = str(default_path)
    else:
        print("Usage: python analyze_schlib.py [path/to/file.SchLib]")
        sys.exit(1)

    analyze_schlib(filepath)


if __name__ == "__main__":
    main()
